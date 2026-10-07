use std::ffi::{c_int, c_void, CString};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

const CRASH: &str = "crash.txt";
const PANIC: &str = "last-panic.txt";
const FATAL: [c_int; 5] = [libc::SIGSEGV, libc::SIGABRT, libc::SIGBUS, libc::SIGILL, libc::SIGFPE];

struct Mark {
    path: CString,
    header: String,
    previous: [libc::sigaction; FATAL.len()],
}

static MARK: OnceLock<Mark> = OnceLock::new();
static HEADER_WRITTEN: AtomicBool = AtomicBool::new(false);

static ENDED: AtomicBool = AtomicBool::new(false);
static LAST_TIME: Mutex<Option<String>> = Mutex::new(None);

pub fn install() {
    let dir = numa::core::paths::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    *LAST_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = take(&dir);

    let panic_path = dir.join(PANIC);
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);

        let nested = info.payload_as_str().filter(|message| message.starts_with("panic in a "));
        let file = std::fs::OpenOptions::new().create(true).write(true).append(nested.is_some()).truncate(nested.is_none()).open(&panic_path);
        if let Ok(mut file) = file {
            use std::io::Write;
            let _ = match nested {
                Some(message) => writeln!(file, "Then: {message}"),
                None => write!(file, "{}", panic_report(info)),
            };
        }
    }));

    let Ok(path) = CString::new(dir.join(CRASH).into_os_string().into_encoded_bytes()) else { return };

    let mut previous: [libc::sigaction; FATAL.len()] = unsafe { std::mem::zeroed() };
    for (signal, old) in FATAL.iter().zip(previous.iter_mut()) {

        unsafe { libc::sigaction(*signal, std::ptr::null(), old) };
    }
    if MARK.set(Mark { path, header: header(), previous }).is_err() {
        return;
    }
    for signal in FATAL {

        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_fatal as extern "C" fn(c_int, *mut libc::siginfo_t, *mut c_void) as usize;
            action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(signal, &action, std::ptr::null_mut());
        }
    }
}

pub fn last_time() -> Option<String> {
    LAST_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take()
}

pub fn clean_exit() {
    ENDED.store(true, Ordering::SeqCst);
    let dir = numa::core::paths::data_dir();
    let _ = std::fs::remove_file(dir.join(CRASH));
    let _ = std::fs::remove_file(dir.join(PANIC));
}

fn take(dir: &Path) -> Option<String> {
    let crash = std::fs::read_to_string(dir.join(CRASH)).ok();
    let panic = std::fs::read_to_string(dir.join(PANIC)).ok();
    let _ = std::fs::remove_file(dir.join(CRASH));
    let _ = std::fs::remove_file(dir.join(PANIC));
    let crash = crash?;
    Some(match panic {
        Some(panic) => format!("{}\n\n{}", crash.trim_end(), panic.trim_end()),
        None => crash.trim_end().to_string(),
    })
}

extern "C" fn on_fatal(signal: c_int, info: *mut libc::siginfo_t, context: *mut c_void) {
    let Some(mark) = MARK.get() else { return };
    let Some(index) = FATAL.iter().position(|fatal| *fatal == signal) else { return };

    unsafe {
        if !ENDED.load(Ordering::SeqCst) {
            let file = libc::open(mark.path.as_ptr(), libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND | libc::O_CLOEXEC, 0o644);
            if file >= 0 {
                if !HEADER_WRITTEN.swap(true, Ordering::SeqCst) {
                    libc::write(file, mark.header.as_ptr().cast(), mark.header.len());
                }
                let line = stopped_by(signal);
                libc::write(file, line.as_ptr().cast(), line.len());
                libc::close(file);
            }
        }

        let previous = &mark.previous[index];
        libc::sigaction(signal, previous, std::ptr::null_mut());
        match previous.sa_sigaction {
            libc::SIG_DFL | libc::SIG_IGN => {}
            handler if previous.sa_flags & libc::SA_SIGINFO != 0 => {
                let handler: extern "C" fn(c_int, *mut libc::siginfo_t, *mut c_void) = std::mem::transmute(handler);
                handler(signal, info, context);
            }
            handler => {
                let handler: extern "C" fn(c_int) = std::mem::transmute(handler);
                handler(signal);
            }
        }
        let mut now: libc::sigaction = std::mem::zeroed();
        libc::sigaction(signal, std::ptr::null(), &mut now);
        if now.sa_sigaction == libc::SIG_DFL {
            libc::raise(signal);
        }
    }
}

fn stopped_by(signal: c_int) -> &'static str {
    match signal {
        libc::SIGSEGV => "Stopped by SIGSEGV: memory it may not touch.\n",
        libc::SIGABRT => "Stopped by SIGABRT: aborted, as a panic in a GTK callback does.\n",
        libc::SIGBUS => "Stopped by SIGBUS: memory that is not there.\n",
        libc::SIGILL => "Stopped by SIGILL: an instruction this processor does not have.\n",
        _ => "Stopped by SIGFPE: an arithmetic fault.\n",
    }
}

fn header() -> String {
    let how = if std::env::var_os("FLATPAK_ID").is_some() {
        "Flatpak"
    } else if std::env::var_os("APPIMAGE").is_some() {
        "AppImage"
    } else if std::env::current_exe().is_ok_and(|exe| exe.starts_with("/usr")) {
        "a distribution's package"
    } else {
        "built from source"
    };
    let mut lines = vec![
        format!("Numa {} ({how})", env!("CARGO_PKG_VERSION")),
        format!(
            "GTK {}.{}.{}, libadwaita {}.{}.{}",
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version(),
            adw::major_version(),
            adw::minor_version(),
            adw::micro_version()
        ),
        format!("Renderer: {}", std::env::var("GSK_RENDERER").unwrap_or_else(|_| "default".to_string())),
    ];
    lines.extend(build());
    lines.join("\n") + "\n"
}

fn build() -> Option<String> {
    use std::io::Read;
    const NOTE: [u8; 16] = [4, 0, 0, 0, 20, 0, 0, 0, 3, 0, 0, 0, b'G', b'N', b'U', 0];
    let mut head = Vec::new();
    std::fs::File::open("/proc/self/exe").ok()?.take(64 << 10).read_to_end(&mut head).ok()?;
    let at = head.windows(NOTE.len()).position(|window| window == NOTE)? + NOTE.len();
    let id: String = head.get(at..at + 20)?.iter().map(|byte| format!("{byte:02x}")).collect();
    let exe = std::fs::read_link("/proc/self/exe").ok()?;
    let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
    let base = maps.lines().find(|line| line.ends_with(exe.to_str().unwrap_or("\0")))?.split('-').next()?.to_string();
    Some(format!("Build {id}, loaded at 0x{base}"))
}

fn panic_report(info: &std::panic::PanicHookInfo) -> String {
    let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
    let at = info.location().map_or(String::new(), |at| format!(" at {}:{}", tilde(at.file()), at.line()));
    let message = scrub(info.payload_as_str().unwrap_or("(no message)"));

    let trace = format!("{:#}", std::backtrace::Backtrace::force_capture());
    format!("Panicked on thread '{thread}'{at}:\n{message}\n\n{}\n", short(&tilde(&trace)))
}

fn short(trace: &str) -> String {
    let lines: Vec<&str> = trace.lines().collect();
    let mut from = lines.iter().rposition(|line| line.contains("__rust_end_short_backtrace")).map_or(0, |at| at + 1);
    while lines.get(from).is_some_and(|line| line.trim_start().starts_with("at ")) {
        from += 1;
    }
    let to = lines.iter().position(|line| line.contains("__rust_begin_short_backtrace")).filter(|&to| to > from);
    lines[from..to.unwrap_or(lines.len())].iter().map(|line| unhashed(line)).collect::<Vec<_>>().join("\n")
}

fn unhashed(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(at) = rest.find('[') {
        let hash = rest.get(at + 1..at + 18).is_some_and(|hash| hash.ends_with(']') && hash[..16].bytes().all(|b| b.is_ascii_hexdigit()));
        let skip = if hash { 18 } else { 1 };
        out.push_str(&rest[..if hash { at } else { at + 1 }]);
        rest = &rest[at + skip..];
    }
    out + rest
}

fn tilde(text: &str) -> String {
    match dirs::home_dir() {
        Some(home) if home.as_os_str().len() > 1 => text.replace(home.to_string_lossy().as_ref(), "~"),
        _ => text.to_string(),
    }
}

fn scrub(message: &str) -> String {
    let ours = [
        (numa::core::paths::data_dir(), "<numa data>"),
        (numa::core::paths::cache_dir(), "<numa cache>"),
    ];
    let mut text = message.to_string();
    for (dir, name) in &ours {
        text = text.replace(dir.to_string_lossy().as_ref(), name);
    }
    let text = tilde(&text);
    let mut out = String::new();
    let mut rest = text.as_str();

    while let Some(start) = path_start(rest) {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let end = tail.find(['"', '\'', '`', '\n']).into_iter().chain(tail.find(": ")).min().unwrap_or(tail.len());
        out.push_str("<path>");
        rest = &tail[end..];
    }
    out.push_str(rest);
    out.split_inclusive(char::is_whitespace)
        .map(|word| {
            let bare = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '_' && c != '-');
            match bare.contains('.') && numa::io::raw::is_supported(Path::new(bare)) {
                true => word.replace(bare, "<photo>"),
                false => word.to_string(),
            }
        })
        .collect()
}

fn path_start(text: &str) -> Option<usize> {
    text.char_indices().find_map(|(at, c)| {
        let before = text[..at].chars().next_back();
        let boundary = before.is_none_or(|b| b.is_whitespace() || "\"'`(=:".contains(b));
        let path = c == '/' || (c == '~' && text[at..].starts_with("~/"));
        (boundary && path).then_some(at)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_keeps_no_path_and_no_photograph() {
        let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
        let data = numa::core::paths::data_dir().to_string_lossy().to_string();
        let message = format!(
            "could not read \"{home}/Pictures/Japan 2026/DSCF1234.RAF\": invalid data\n\
             no model at {data}/models/sam.onnx\n\
             IMG_0042.HEIC is not a photograph /mnt/fotos/x.jpg: gone"
        );
        let scrubbed = scrub(&message);
        assert_eq!(
            scrubbed,
            "could not read \"<path>\": invalid data\n\
             no model at <numa data>/models/sam.onnx\n\
             <photo> is not a photograph <path>: gone"
        );
        assert!(!scrubbed.contains("Japan") && !scrubbed.contains(&home));
    }

    #[test]
    fn a_short_trace_is_std_s_cut() {
        let trace = "   0: 0x1 - std::backtrace::Backtrace::force_capture\n\
                     \x20  1: 0x2 - std::sys::backtrace::__rust_end_short_backtrace\n\
                     \x20            at /rustc/library/std/src/sys/backtrace.rs:9\n\
                     \x20  2: 0x3 - core::panicking::panic_fmt\n\
                     \x20  3: 0x4 - numa::ui::window::open\n\
                     \x20  4: 0x5 - std::sys::backtrace::__rust_begin_short_backtrace\n\
                     \x20  5: 0x6 - main";
        assert_eq!(short(trace), "   2: 0x3 - core::panicking::panic_fmt\n   3: 0x4 - numa::ui::window::open");
        assert_eq!(short("   0: 0x1 - main"), "   0: 0x1 - main");
        assert_eq!(
            short("   8: 0x1 - glib[d5952ddba3edcba4]::source::<numa[a123f7d87ba89bf7]::main>[2]"),
            "   8: 0x1 - glib::source::<numa::main>[2]"
        );
    }

    #[test]
    fn only_a_crash_is_reported_and_both_files_go() {
        let dir = std::env::temp_dir().join(format!("numa-crash-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(PANIC), "Panicked on thread 'rawler'").unwrap();
        assert_eq!(take(&dir), None, "a caught panic without a crash is not a report");
        assert!(!dir.join(PANIC).exists());

        std::fs::write(dir.join(CRASH), "Numa 0.36.0\nStopped by SIGABRT\n").unwrap();
        std::fs::write(dir.join(PANIC), "Panicked on thread 'main'\n").unwrap();
        assert_eq!(take(&dir).as_deref(), Some("Numa 0.36.0\nStopped by SIGABRT\n\nPanicked on thread 'main'"));
        assert!(!dir.join(CRASH).exists() && !dir.join(PANIC).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
