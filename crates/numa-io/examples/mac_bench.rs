#![cfg_attr(not(target_vendor = "apple"), allow(dead_code, unused_imports))]

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const EDGE: u32 = 2400;

static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Catch;

impl log::Log for Catch {
    fn enabled(&self, meta: &log::Metadata) -> bool {
        meta.level() <= log::Level::Warn || meta.target().starts_with("numa")
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let line = format!("{} {}", record.target(), record.args());
            eprintln!("{line}");
            LINES.lock().unwrap().push(line);
        }
    }
    fn flush(&self) {}
}

#[cfg(not(target_vendor = "apple"))]
fn main() {
    eprintln!("mac_bench measures on a Mac");
}

#[cfg(target_vendor = "apple")]
fn main() {
    log::set_logger(&Catch).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let mut args = std::env::args().skip(1);
    let mode = args.next().expect("a mode");
    let paths: Vec<String> = args.collect();
    let env = |name: &str, default: usize| std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default);
    let (warm, passes, gap) = (env("WARM", 1), env("PASSES", 3), env("GAP_MS", 250) as u64);

    unsafe { std::env::set_var("NUMA_GPU", if mode.starts_with("metal") { "1" } else { "0" }) };
    let qos = apple::qos_class(std::env::var("NUMA_QOS").ok().as_deref());
    apple::set_qos(qos);
    let mut pool = rayon::ThreadPoolBuilder::new().start_handler(move |_| apple::set_qos(qos));
    if let Some(n) = std::env::var("NUMA_THREADS").ok().and_then(|v| v.parse().ok()) {
        pool = pool.num_threads(n);
    }
    pool.build_global().unwrap();

    let meter = apple::Meter::new();
    let init = Instant::now();
    if mode.starts_with("metal") && !numa_gpu::open_now(false) {
        println!("{}", serde_json::json!({ "mode": mode, "error": "no card" }));
        std::process::exit(2);
    }
    let init_ms = init.elapsed().as_secs_f64() * 1000.0;

    let before = meter.snap();
    std::thread::sleep(Duration::from_millis(800));
    let idle = meter.delta(&before, &meter.snap());

    for path in &paths {
        let body = Path::new(path).file_stem().unwrap().to_string_lossy().to_string();
        for pass in 0..warm + passes {
            LINES.lock().unwrap().clear();
            let before = meter.snap();
            let result = run(&mode, Path::new(path));
            let after = meter.snap();
            let delta = meter.delta(&before, &after);
            let laps: Vec<String> = std::mem::take(&mut *LINES.lock().unwrap());
            println!(
                "{}",
                serde_json::json!({
                    "mode": mode, "body": body, "pass": pass, "warm": pass < warm,
                    "threads": rayon::current_num_threads(), "qos": std::env::var("NUMA_QOS").unwrap_or_default(),
                    "size": result.as_ref().ok(), "error": result.as_ref().err(),
                    "m": delta, "laps": laps, "env": knobs(),
                })
            );
            std::thread::sleep(Duration::from_millis(gap));
        }
    }
    let usage = apple::rusage();
    println!(
        "{}",
        serde_json::json!({
            "final": true, "mode": mode, "paths": paths.len(), "init_ms": init_ms, "idle": idle,
            "peak_footprint_mb": usage.get(28).map(|v| *v as f64 / 1048576.0),
            "rusage_raw": usage, "threads": rayon::current_num_threads(),
            "gpu": numa_gpu::describe(false),
        })
    );
}

fn knobs() -> String {
    ["NUMA_GPU_MAX_BUFFER", "NUMA_GPU_BUDGET", "NUMA_GPU_TILE", "NUMA_GPU_HALF"]
        .iter()
        .filter_map(|name| std::env::var(name).ok().map(|value| format!("{}={value}", &name[5..])))
        .collect::<Vec<_>>()
        .join(" ")
}

fn run(mode: &str, path: &Path) -> Result<(u32, u32), String> {
    let image = match mode {
        "decode" => {
            let source = rawler::rawsource::RawSource::new(path).map_err(|e| e.to_string())?;
            let decoder = rawler::get_decoder(&source).map_err(|e| e.to_string())?;
            let raw = decoder.raw_image(&source, &rawler::decoders::RawDecodeParams::default(), false).map_err(|e| e.to_string())?;
            return Ok((raw.width as u32, raw.height as u32));
        }
        "cpu-full" | "metal-full" => numa_io::raw::decode_linear_best(path)?,
        "metal-proxy" => numa_io::raw::proxy_from_card(path, EDGE)?.0,
        "mosaic-proxy" => numa_io::raw::proxy_from_mosaic(path, EDGE)?.0,
        _ => return Err(format!("no mode {mode}")),
    };
    Ok((image.width, image.height))
}

mod apple {
    use serde_json::{json, Map, Value};
    use std::ffi::{c_char, c_void, CStr};
    use std::time::Instant;

    type Ref = *const c_void;
    const UTF8: u32 = 0x0800_0100;

    #[cfg_attr(target_vendor = "apple", link(name = "IOReport", kind = "dylib"))]
    unsafe extern "C" {
        fn IOReportCopyChannelsInGroup(group: Ref, subgroup: Ref, a: u64, b: u64, c: u64) -> Ref;
        fn IOReportCreateSubscription(a: Ref, desired: Ref, subbed: *mut Ref, id: u64, b: Ref) -> Ref;
        fn IOReportCreateSamples(sub: Ref, subbed: Ref, a: Ref) -> Ref;
        fn IOReportCreateSamplesDelta(previous: Ref, current: Ref, a: Ref) -> Ref;
        fn IOReportChannelGetGroup(item: Ref) -> Ref;
        fn IOReportChannelGetChannelName(item: Ref) -> Ref;
        fn IOReportChannelGetUnitLabel(item: Ref) -> Ref;
        fn IOReportSimpleGetIntegerValue(item: Ref, index: i32) -> i64;
    }

    #[cfg_attr(target_vendor = "apple", link(name = "CoreFoundation", kind = "framework"))]
    unsafe extern "C" {
        fn CFStringCreateWithCString(alloc: Ref, text: *const c_char, encoding: u32) -> Ref;
        fn CFStringGetCString(text: Ref, buffer: *mut c_char, size: isize, encoding: u32) -> u8;
        fn CFDictionaryGetCount(dict: Ref) -> isize;
        fn CFDictionaryGetValue(dict: Ref, key: Ref) -> Ref;
        fn CFDictionaryCreateMutableCopy(alloc: Ref, capacity: isize, dict: Ref) -> Ref;
        fn CFArrayGetCount(array: Ref) -> isize;
        fn CFArrayGetValueAtIndex(array: Ref, index: isize) -> Ref;
        fn CFRelease(any: Ref);
    }

    unsafe extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut u64) -> i32;
        fn pthread_set_qos_class_self_np(class: u32, relative: i32) -> i32;
    }

    pub fn qos_class(name: Option<&str>) -> Option<u32> {
        match name? {
            "ui" => Some(0x21),
            "in" => Some(0x19),
            "ut" => Some(0x11),
            "bg" => Some(0x09),
            _ => None,
        }
    }

    pub fn set_qos(class: Option<u32>) {
        #[cfg(target_vendor = "apple")]
        if let Some(class) = class {

            unsafe { pthread_set_qos_class_self_np(class, 0) };
        }
        #[cfg(not(target_vendor = "apple"))]
        let _ = class;
    }

    pub fn rusage() -> Vec<u64> {
        #[cfg(target_vendor = "apple")]
        {
            let mut buffer = [0u64; 2 + 80];

            if unsafe { proc_pid_rusage(std::process::id() as i32, 6, buffer.as_mut_ptr()) } == 0 {
                return buffer[2..].to_vec();
            }
        }
        Vec::new()
    }

    fn string(text: Ref) -> String {
        if text.is_null() {
            return String::new();
        }
        let mut buffer = [0 as c_char; 256];

        match unsafe { CFStringGetCString(text, buffer.as_mut_ptr(), 256, UTF8) } {
            0 => String::new(),

            _ => unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy().into_owned(),
        }
    }

    fn cf(text: &CStr) -> Ref {

        unsafe { CFStringCreateWithCString(std::ptr::null(), text.as_ptr(), UTF8) }
    }

    pub struct Snap {
        at: Instant,
        cpu_ms: f32,
        usage: Vec<u64>,
        sample: Ref,
    }

    pub struct Meter {
        subscription: Ref,
        channels: Ref,
    }

    impl Meter {
        pub fn new() -> Self {
            let (mut subscription, mut channels): (Ref, Ref) = (std::ptr::null(), std::ptr::null());
            #[cfg(target_vendor = "apple")]

            unsafe {
                let group = cf(c"Energy Model");
                let found = IOReportCopyChannelsInGroup(group, std::ptr::null(), 0, 0, 0);
                if !found.is_null() {
                    let desired = CFDictionaryCreateMutableCopy(std::ptr::null(), CFDictionaryGetCount(found), found);
                    subscription = IOReportCreateSubscription(std::ptr::null(), desired, &mut channels, 0, std::ptr::null());
                }
            }
            if subscription.is_null() {
                eprintln!("mac_bench: no IOReport energy");
            }
            Self { subscription, channels }
        }

        pub fn snap(&self) -> Snap {
            let sample = match self.subscription.is_null() {
                true => std::ptr::null(),

                false => unsafe { IOReportCreateSamples(self.subscription, self.channels, std::ptr::null()) },
            };
            Snap { at: Instant::now(), cpu_ms: numa_io::raw::cpu_ms(), usage: rusage(), sample }
        }

        pub fn delta(&self, a: &Snap, b: &Snap) -> Value {
            let mut out = Map::new();
            out.insert("ms".into(), json!(b.at.duration_since(a.at).as_secs_f64() * 1000.0));
            out.insert("cpu_ms".into(), json!(b.cpu_ms - a.cpu_ms));
            if a.usage.len() > 41 && b.usage.len() > 41 {
                let d = |k: usize| b.usage[k].wrapping_sub(a.usage[k]);
                out.insert("ru".into(), json!({ "ut": d(0), "st": d(1), "upt": d(36), "spt": d(37), "e_nj": d(40), "pe_nj": d(41), "billed": d(31) }));
            }
            if !a.sample.is_null() && !b.sample.is_null() {
                let mut energy = Map::new();

                unsafe {
                    let delta = IOReportCreateSamplesDelta(a.sample, b.sample, std::ptr::null());
                    let key = cf(c"IOReportChannels");
                    let items = CFDictionaryGetValue(delta, key);
                    if !items.is_null() {
                        for k in 0..CFArrayGetCount(items) {
                            let item = CFArrayGetValueAtIndex(items, k);
                            if string(IOReportChannelGetGroup(item)) != "Energy Model" {
                                continue;
                            }
                            let value = IOReportSimpleGetIntegerValue(item, 0) as f64;
                            let scale = match string(IOReportChannelGetUnitLabel(item)).trim() {
                                "mJ" => 1e6,
                                "uJ" | "µJ" => 1e3,
                                "nJ" => 1.0,
                                _ => f64::NAN,
                            };
                            let name = string(IOReportChannelGetChannelName(item));
                            let total = energy.get(&name).and_then(Value::as_f64).unwrap_or(0.0);
                            energy.insert(name, json!(total + value * scale));
                        }
                    }
                    CFRelease(key);
                    CFRelease(delta);
                }
                out.insert("ior".into(), Value::Object(energy));
            }
            Value::Object(out)
        }
    }

    impl Drop for Snap {
        fn drop(&mut self) {
            if !self.sample.is_null() {

                unsafe { CFRelease(self.sample) };
            }
        }
    }
}
