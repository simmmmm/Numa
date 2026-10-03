mod buffers;
mod develop;
#[cfg(target_os = "linux")]
pub mod display;
pub mod render;

pub use develop::{develop, Developed, Job, Mosaic};
pub use render::{forget_proxies, render, render_enabled, Output, Pixels, Rendered};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub fn enabled() -> bool {
    std::env::var("NUMA_GPU").as_deref() != Ok("0")
}

pub fn warm_up(frugal: bool, marker: Option<std::path::PathBuf>) {
    let frugal = lane(frugal);

    if !enabled() || making(frugal).try_lock().is_err() || !matches!(*lock(frugal), Slot::Untried) {
        return;
    }
    if let Some(marker) = &marker {
        if std::fs::write(marker, b"").is_err() {
            log::warn!("GPU: the safe-mode marker could not be written; decoding on the processor");
            return;
        }
    }
    let _ = std::thread::Builder::new()
        .name("numa-gpu".into())
        .spawn(move || {
            made(frugal);
            if let Some(marker) = marker {
                let _ = std::fs::remove_file(marker);
            }
        });
}

pub fn open_now(frugal: bool) -> bool {
    enabled() && made(lane(frugal)).is_some()
}

pub fn release() {
    for frugal in [false, true] {
        let _making = making(frugal).lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut slot = lock(frugal);
        if matches!(*slot, Slot::Ready(_)) {
            *slot = Slot::Untried;
        }
    }
}

pub fn broken() -> bool {
    BROKEN.load(Ordering::Relaxed)
}

static BROKEN: AtomicBool = AtomicBool::new(false);

pub fn describe(frugal: bool) -> Option<String> {
    ready(frugal).map(|gpu| gpu.name.clone())
}

pub fn discrete(frugal: bool) -> bool {
    ready(frugal).is_some_and(|gpu| gpu.discrete)
}

pub fn limits(frugal: bool) -> Option<String> {
    ready(frugal).map(|gpu| gpu.limits.clone())
}

fn ready(frugal: bool) -> Option<Arc<Gpu>> {
    if !enabled() {
        return None;
    }
    match &*lock(lane(frugal)) {
        Slot::Ready(gpu) if !gpu.lost.load(Ordering::Relaxed) => Some(gpu.clone()),
        _ => None,
    }
}

fn made(frugal: bool) -> Option<Arc<Gpu>> {
    let _making = making(frugal).lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if matches!(*lock(frugal), Slot::Untried) {
        let made = open(frugal).map_or(Slot::None, |gpu| Slot::Ready(Arc::new(gpu)));
        *lock(frugal) = made;
    }
    match &*lock(frugal) {
        Slot::Ready(gpu) => Some(gpu.clone()),
        _ => None,
    }
}

fn lane(frugal: bool) -> bool {
    frugal && !cfg!(target_vendor = "apple")
}

enum Slot {
    Untried,

    None,
    Ready(Arc<Gpu>),
}

fn slot(frugal: bool) -> &'static Mutex<Slot> {
    static FAST: Mutex<Slot> = Mutex::new(Slot::Untried);
    static FRUGAL: Mutex<Slot> = Mutex::new(Slot::Untried);
    if frugal {
        &FRUGAL
    } else {
        &FAST
    }
}

fn making(frugal: bool) -> &'static Mutex<()> {
    static FAST: Mutex<()> = Mutex::new(());
    static FRUGAL: Mutex<()> = Mutex::new(());
    if frugal {
        &FRUGAL
    } else {
        &FAST
    }
}

fn lock(frugal: bool) -> std::sync::MutexGuard<'static, Slot> {
    slot(frugal).lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    pipelines: Pipelines,

    half: OnceLock<Option<Pipelines>>,
    name: String,
    limits: String,

    lost: Arc<AtomicBool>,

    max_binding: u64,

    mapped: bool,

    band_stride: u64,

    tile_pixels: usize,

    render: Option<render::State>,

    dmabuf: bool,

    pci: (u32, u32),

    discrete: bool,
}

type Pipelines = Vec<(&'static str, wgpu::ComputePipeline)>;

impl Gpu {

    fn pipelines(&self, half: bool) -> Option<&Pipelines> {
        match half {
            false => Some(&self.pipelines),
            true => self
                .half
                .get_or_init(|| {
                    let features = self.device.features().contains(wgpu::Features::SHADER_F16);
                    features.then(|| compile(&self.device, &self.layout, true)).flatten()
                })
                .as_ref(),
        }
    }
}

const ENTRIES: &[&str] = &[
    "scale", "ppg_green", "ppg_rb", "mk_green", "mk_solitary", "mk_rb", "mk_blocks", "mk_drv", "mk_homo", "mk_final",
    "geometry", "false_colour", "samples", "proxy", "full",
];

const BINDINGS: u32 = 16;

fn open(frugal: bool) -> Option<Gpu> {
    let started = std::time::Instant::now();
    let mut description = wgpu::InstanceDescriptor::new_without_display_handle();

    description.flags = wgpu::InstanceFlags::from_build_config().with_env();

    description.memory_budget_thresholds.for_resource_creation = Some(90);
    let instance = wgpu::Instance::new(description);

    let adapters = block_on(instance.enumerate_adapters(wgpu::Backends::all()));
    let adapter = choose(adapters, frugal)?;
    let info = adapter.get_info();
    let limits = adapter.limits();
    if limits.max_storage_buffers_per_shader_stage < BINDINGS {
        log::info!("GPU: {} binds too few buffers; decoding on the processor", info.name);
        return None;
    }

    let apple = cfg!(target_vendor = "apple");
    let features = adapter.features()
        & match (apple, half_always()) {
            (true, _) => wgpu::Features::MAPPABLE_PRIMARY_BUFFERS | wgpu::Features::SHADER_F16,
            (false, true) => wgpu::Features::SHADER_F16,
            (false, false) => wgpu::Features::empty(),
        };
    let mapped = features.contains(wgpu::Features::MAPPABLE_PRIMARY_BUFFERS);
    let max_binding = u64::from(limits.max_storage_buffer_binding_size)
        .min(limits.max_buffer_size)
        .min(env_bytes("NUMA_GPU_MAX_BUFFER").unwrap_or(u64::MAX));
    let described = format!(
        "{} ({:?}, {:?}): max buffer {} MB, max storage binding {} MB, {} storage buffers a stage{}{}",
        info.name,
        info.device_type,
        info.backend,
        limits.max_buffer_size >> 20,
        u64::from(limits.max_storage_buffer_binding_size) >> 20,
        limits.max_storage_buffers_per_shader_stage,
        if mapped { ", unified memory" } else { "" },
        if features.contains(wgpu::Features::SHADER_F16) { ", f16" } else { "" },
    );
    log::info!("GPU: {described}");

    let dmabuf = cfg!(target_os = "linux")
        && render::render_enabled()
        && std::env::var("NUMA_GPU_DMABUF").as_deref() == Ok("1")
        && adapter.features().contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF);
    let (device, queue) = match block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("numa develop"),
        required_features: features | if dmabuf { wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF } else { wgpu::Features::empty() },
        required_limits: limits.clone(),
        experimental_features: wgpu::ExperimentalFeatures::default(),

        memory_hints: wgpu::MemoryHints::Manual { suballocated_device_memory_block_size: (4 << 20)..(4 << 20) },
        trace: wgpu::Trace::Off,
    })) {
        Ok(pair) => pair,
        Err(err) => {
            log::info!("GPU: {} refused a device ({err}); decoding on the processor", info.name);
            return None;
        }
    };

    device.on_uncaptured_error(Arc::new(|error| log::warn!("GPU: {error}")));
    let lost = Arc::new(AtomicBool::new(false));
    let flag = lost.clone();
    device.set_device_lost_callback(move |reason, message| {
        flag.store(true, Ordering::Relaxed);
        log::warn!("GPU: the device was lost ({reason:?}: {message}); decoding on the processor from now on");
    });

    let layout = bind_group_layout(&device);
    let Some(pipelines) = compile(&device, &layout, false) else {
        BROKEN.store(true, Ordering::Relaxed);
        return None;
    };
    let render = render::render_enabled().then(|| render::build(&device)).flatten();

    log::info!(
        "GPU: {} ({:?}, {:?}) ready in {:.0} ms{}",
        info.name,
        info.device_type,
        info.backend,
        started.elapsed().as_secs_f32() * 1000.0,
        if frugal { ", frugal" } else { "" }
    );
    Some(Gpu {
        device,
        queue,
        layout,
        pipelines,
        half: OnceLock::new(),
        name: info.name,
        limits: described,
        lost,

        max_binding,
        mapped,
        band_stride: u64::from(limits.min_storage_buffer_offset_alignment).max(16),

        tile_pixels: env_bytes("NUMA_GPU_TILE").map_or(
            match info.device_type {
                _ if apple => 8 << 20,
                wgpu::DeviceType::DiscreteGpu => 8 << 20,
                _ => 1 << 20,
            },
            |pixels| pixels as usize,
        ),
        render,
        dmabuf,
        pci: (info.vendor, info.device),
        discrete: info.device_type == wgpu::DeviceType::DiscreteGpu && !mapped,
    })
}

fn compile(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, half: bool) -> Option<Pipelines> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let source = concat!(include_str!("develop.wgsl"), include_str!("demosaic.wgsl"));
    let source = match half {
        true => format!("enable f16;\n{}", source.replacen("alias Plane = f32;", "alias Plane = f16;", 1)),
        false => source.to_string(),
    };
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("develop"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("develop"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    let pipelines = ENTRIES
        .iter()
        .map(|entry| {
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: wgpu::PipelineCompilationOptions {

                    zero_initialize_workgroup_memory: false,
                    ..Default::default()
                },
                cache: None,
            });
            (*entry, pipeline)
        })
        .collect();
    if let Some(error) = block_on(scope.pop()) {
        log::warn!("GPU: the shaders did not build{} ({error}); decoding on the processor", if half { " in f16" } else { "" });
        return None;
    }
    Some(pipelines)
}

fn half_always() -> bool {
    std::env::var("NUMA_GPU_HALF").as_deref() == Ok("1")
}

fn env_bytes(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.parse().ok()
}

fn choose(adapters: Vec<wgpu::Adapter>, frugal: bool) -> Option<wgpu::Adapter> {
    use wgpu::DeviceType::{DiscreteGpu, IntegratedGpu};
    let wanted: &[wgpu::DeviceType] = if frugal { &[IntegratedGpu] } else { &[DiscreteGpu, IntegratedGpu] };
    let found = wanted
        .iter()
        .find_map(|kind| adapters.iter().find(|adapter| adapter.get_info().device_type == *kind).cloned());
    if found.is_none() {
        log::info!("GPU: no {} card; decoding on the processor", if frugal { "integrated" } else { "suitable" });
    }
    found
}

fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let storage = |binding: u32, read_only: bool, has_dynamic_offset: bool| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset,
            min_binding_size: None,
        },
        count: None,
    };
    let entries: Vec<_> = (0..BINDINGS).map(|binding| storage(binding, binding <= 3, binding == 1)).collect();
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("develop"), entries: &entries })
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        std::thread::yield_now();
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn the_card_is_ready_to_everyone_asking_at_once() {
        if !super::open_now(false) {
            println!("skipped: no card");
            return;
        }
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| assert!((0..20_000).all(|_| super::describe(false).is_some())));
            }
        });
    }
}
