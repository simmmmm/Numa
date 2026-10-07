use std::ffi::{c_char, c_int, c_ulong, c_void, CStr};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crate::io::raw;

const WAIT_MS: c_int = 400;

const RETRY: Duration = Duration::from_secs(2);

static LOOKING: Mutex<()> = Mutex::new(());

fn looking() -> std::sync::MutexGuard<'static, ()> {
    LOOKING.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

const EVENT_FILE_ADDED: c_int = 2;
const FILE_TYPE_NORMAL: c_int = 1;
const ERROR_TIMEOUT: c_int = -10;
const ERROR_IO_USB_CLAIM: c_int = -53;
const ERROR_CAMERA_BUSY: c_int = -110;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {

    Connected(String),

    Waiting(Waiting),

    Arrived(PathBuf),

    Failed(String, String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Waiting {

    NoCamera,

    Held,

    Lost,

    NotShooting,
}

pub struct Session {
    stop: Arc<AtomicBool>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn available() -> bool {
    std::env::var_os(STAND_IN).is_some() || lib().is_some()
}

const STAND_IN: &str = "NUMA_TETHER_FROM";

fn stand_in_name(from: &Path) -> String {
    from.file_name().map_or_else(|| "Stand-in".into(), |name| name.to_string_lossy().into_owned())
}

pub fn cameras() -> Vec<String> {
    if let Some(from) = std::env::var_os(STAND_IN) {
        return vec![stand_in_name(Path::new(&from))];
    }
    let Some(lib) = lib() else { return Vec::new() };
    let _one = looking();
    unsafe {
        let context = (lib.context_new)();
        let found = detect(lib, context);
        (lib.context_unref)(context);
        found.iter().map(|name| tidy(name)).collect()
    }
}

pub fn start(folder: PathBuf, told: impl Fn(Event) + Send + 'static) -> Option<Session> {
    let stop = Arc::new(AtomicBool::new(false));
    let running = stop.clone();
    let thread = std::thread::Builder::new().name("tether".into());
    match std::env::var_os(STAND_IN) {
        Some(from) => thread.spawn(move || stand_in(Path::new(&from), &folder, &running, &told)).ok()?,
        None => {
            let lib = lib()?;
            thread.spawn(move || run(lib, &folder, &running, &told)).ok()?
        }
    };
    Some(Session { stop })
}

fn stand_in(from: &Path, folder: &Path, stop: &AtomicBool, told: &dyn Fn(Event)) {
    let mut taken: Vec<PathBuf> = std::fs::read_dir(from).into_iter().flatten().flatten().map(|entry| entry.path()).filter(|path| raw::is_supported(path)).collect();
    taken.sort();
    told(Event::Connected(stand_in_name(from)));
    for path in taken {
        nap(stop, Duration::from_secs(3));
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        match std::fs::read(&path).and_then(|bytes| place(folder, &name, &bytes)) {
            Ok(path) => told(Event::Arrived(path)),
            Err(err) => told(Event::Failed(name, err.to_string())),
        }
    }
}

fn run(lib: &'static Lib, folder: &Path, stop: &AtomicBool, told: &dyn Fn(Event)) {
    let context = unsafe { (lib.context_new)() };
    let mut camera: Option<Camera> = None;
    let mut said: Option<Waiting> = None;

    let mut fetched = std::collections::HashSet::new();
    while !stop.load(Ordering::Relaxed) {
        let Some(open) = &camera else {
            match Camera::open(lib, context) {
                Ok((open, name)) => {
                    let shooting = open.shoots();
                    camera = Some(open);
                    fetched.clear();
                    said = None;
                    told(Event::Connected(name));
                    if !shooting {
                        said = Some(Waiting::NotShooting);
                        told(Event::Waiting(Waiting::NotShooting));
                    }
                }
                Err(why) => {
                    if said != Some(why) {
                        said = Some(why);
                        told(Event::Waiting(why));
                    }
                    nap(stop, RETRY);
                }
            }
            continue;
        };
        match open.next_file() {
            Ok(Some((on_camera, name))) => {
                let shown = name.to_string_lossy().into_owned();

                let key = (on_camera, name);
                if raw::is_supported(Path::new(&shown)) && !fetched.contains(&key) {
                    match open.fetch(&key.0, &key.1).and_then(|bytes| place(folder, &shown, &bytes).map_err(|err| err.to_string())) {
                        Ok(path) => {
                            fetched.insert(key);
                            told(Event::Arrived(path));
                        }
                        Err(why) => told(Event::Failed(shown, why)),
                    }
                }
            }
            Ok(None) => {}
            Err(_) => {
                camera = None;
                said = Some(Waiting::Lost);
                told(Event::Waiting(Waiting::Lost));
            }
        }
    }
    drop(camera);
    unsafe { (lib.context_unref)(context) };
}

fn nap(stop: &AtomicBool, length: Duration) {
    let step = Duration::from_millis(100);
    let mut slept = Duration::ZERO;
    while slept < length && !stop.load(Ordering::Relaxed) {
        std::thread::sleep(step);
        slept += step;
    }
}

fn place(folder: &Path, name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let name = Path::new(name).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| "photo".into());
    std::fs::create_dir_all(folder)?;

    let partial = folder.join(format!(".{name}.part"));
    let mut file = std::fs::File::create(&partial)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    let path = Path::new(&name);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let extension = path.extension().map(|ext| format!(".{}", ext.to_string_lossy())).unwrap_or_default();
    let mut to = folder.join(&name);
    let mut suffix = 2;

    loop {
        match std::fs::hard_link(&partial, &to) {
            Ok(()) => break,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                to = folder.join(format!("{stem}-{suffix}{extension}"));
                suffix += 1;
            }
            Err(err) => {
                let _ = std::fs::remove_file(&partial);
                return Err(err);
            }
        }
    }
    std::fs::remove_file(&partial)?;
    Ok(to)
}

fn tidy(name: &str) -> String {
    let name = name.replace(':', " ");
    let mut words = name.split_whitespace();
    let (Some(first), Some(second)) = (words.next(), words.next()) else { return name.trim().to_string() };
    let repeated = second.to_lowercase().starts_with(&first.to_lowercase());
    let rest: Vec<&str> = words.collect();
    let start = if repeated { vec![second] } else { vec![first, second] };
    start.into_iter().chain(rest).collect::<Vec<_>>().join(" ")
}

struct Camera {
    lib: &'static Lib,
    camera: *mut c_void,
    context: *mut c_void,
}

#[repr(C)]
struct FilePath {
    name: [c_char; 128],
    folder: [c_char; 1024],
}

impl Camera {

    fn open(lib: &'static Lib, context: *mut c_void) -> Result<(Camera, String), Waiting> {
        let _one = looking();
        let names = unsafe { detect(lib, context) };
        let Some(name) = names.first() else { return Err(Waiting::NoCamera) };
        let mut camera = std::ptr::null_mut();
        unsafe {
            if (lib.camera_new)(&mut camera) < 0 {
                return Err(Waiting::Lost);
            }
            let result = (lib.camera_init)(camera, context);
            if result < 0 {
                log::warn!("tethering: the camera did not open: {}", error(lib, result));
                (lib.camera_unref)(camera);
                return Err(match result {
                    ERROR_IO_USB_CLAIM | ERROR_CAMERA_BUSY => Waiting::Held,
                    _ => Waiting::Lost,
                });
            }
        }
        Ok((Camera { lib, camera, context }, tidy(name)))
    }

    fn shoots(&self) -> bool {
        let mut root = std::ptr::null_mut();
        if unsafe { (self.lib.camera_get_config)(self.camera, &mut root, self.context) } < 0 || root.is_null() {
            return true;
        }
        let mut capture = std::ptr::null_mut();
        let shoots = unsafe { (self.lib.widget_get_child_by_name)(root, c"capturesettings".as_ptr(), &mut capture) } < 0
            || unsafe { (self.lib.widget_count_children)(capture) } > 0;
        unsafe { (self.lib.widget_free)(root) };
        shoots
    }

    fn next_file(&self) -> Result<Option<(std::ffi::CString, std::ffi::CString)>, c_int> {
        let mut kind: c_int = 0;
        let mut data: *mut c_void = std::ptr::null_mut();
        let result = unsafe { (self.lib.camera_wait_for_event)(self.camera, WAIT_MS, &mut kind, &mut data, self.context) };
        let file = (result >= 0 && kind == EVENT_FILE_ADDED && !data.is_null()).then(|| unsafe {
            let path = &*(data as *const FilePath);
            (CStr::from_ptr(path.folder.as_ptr()).to_owned(), CStr::from_ptr(path.name.as_ptr()).to_owned())
        });

        if !data.is_null() {
            unsafe { libc::free(data) };
        }
        match result {
            ERROR_TIMEOUT => Ok(None),
            result if result < 0 => {
                log::warn!("tethering: the camera stopped answering: {}", error(self.lib, result));
                Err(result)
            }
            _ => Ok(file),
        }
    }

    fn fetch(&self, folder: &CStr, name: &CStr) -> Result<Vec<u8>, String> {
        let lib = self.lib;
        unsafe {
            let mut file = std::ptr::null_mut();
            if (lib.file_new)(&mut file) < 0 {
                return Err("out of memory".into());
            }
            let result = (lib.camera_file_get)(self.camera, folder.as_ptr(), name.as_ptr(), FILE_TYPE_NORMAL, file, self.context);
            let fetched = match result {
                result if result < 0 => Err(error(lib, result)),
                _ => {
                    let (mut data, mut size): (*const c_char, c_ulong) = (std::ptr::null(), 0);
                    (lib.file_get_data_and_size)(file, &mut data, &mut size);
                    match data.is_null() {
                        true => Err("the camera sent nothing".into()),
                        false => Ok(std::slice::from_raw_parts(data.cast::<u8>(), size as usize).to_vec()),
                    }
                }
            };
            (lib.file_unref)(file);
            fetched
        }
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        unsafe {
            (self.lib.camera_exit)(self.camera, self.context);
            (self.lib.camera_unref)(self.camera);
        }
    }
}

fn error(lib: &Lib, result: c_int) -> String {
    let text = unsafe { (lib.result_as_string)(result) };
    match text.is_null() {
        true => format!("error {result}"),
        false => unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned(),
    }
}

unsafe fn detect(lib: &Lib, context: *mut c_void) -> Vec<String> {
    let mut list = std::ptr::null_mut();
    if (lib.list_new)(&mut list) < 0 {
        return Vec::new();
    }
    let mut names = Vec::new();
    if (lib.camera_autodetect)(list, context) >= 0 {
        for at in 0..(lib.list_count)(list).max(0) {
            let mut name = std::ptr::null();
            if (lib.list_get_name)(list, at, &mut name) >= 0 && !name.is_null() {
                names.push(CStr::from_ptr(name).to_string_lossy().into_owned());
            }
        }
    }
    (lib.list_unref)(list);
    names
}

struct Lib {
    context_new: unsafe extern "C" fn() -> *mut c_void,
    context_unref: unsafe extern "C" fn(*mut c_void),
    camera_new: unsafe extern "C" fn(*mut *mut c_void) -> c_int,
    camera_init: unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int,
    camera_exit: unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int,
    camera_unref: unsafe extern "C" fn(*mut c_void) -> c_int,
    camera_autodetect: unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int,
    camera_get_config: unsafe extern "C" fn(*mut c_void, *mut *mut c_void, *mut c_void) -> c_int,
    widget_get_child_by_name: unsafe extern "C" fn(*mut c_void, *const c_char, *mut *mut c_void) -> c_int,
    widget_count_children: unsafe extern "C" fn(*mut c_void) -> c_int,
    widget_free: unsafe extern "C" fn(*mut c_void) -> c_int,
    camera_wait_for_event: unsafe extern "C" fn(*mut c_void, c_int, *mut c_int, *mut *mut c_void, *mut c_void) -> c_int,
    camera_file_get: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, c_int, *mut c_void, *mut c_void) -> c_int,
    file_new: unsafe extern "C" fn(*mut *mut c_void) -> c_int,
    file_get_data_and_size: unsafe extern "C" fn(*mut c_void, *mut *const c_char, *mut c_ulong) -> c_int,
    file_unref: unsafe extern "C" fn(*mut c_void) -> c_int,
    list_new: unsafe extern "C" fn(*mut *mut c_void) -> c_int,
    list_count: unsafe extern "C" fn(*mut c_void) -> c_int,
    list_get_name: unsafe extern "C" fn(*mut c_void, c_int, *mut *const c_char) -> c_int,
    list_unref: unsafe extern "C" fn(*mut c_void) -> c_int,
    result_as_string: unsafe extern "C" fn(c_int) -> *const c_char,
}

fn lib() -> Option<&'static Lib> {
    static LIB: OnceLock<Option<Lib>> = OnceLock::new();
    LIB.get_or_init(|| unsafe {

        let handle = libc::dlopen(c"libgphoto2.so.6".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
        if handle.is_null() {
            return None;
        }
        macro_rules! symbol {
            ($name:literal) => {{
                let found = libc::dlsym(handle, concat!($name, "\0").as_ptr().cast());
                if found.is_null() {
                    log::warn!("tethering: libgphoto2 has no {}", $name);
                    return None;
                }
                std::mem::transmute(found)
            }};
        }
        Some(Lib {
            context_new: symbol!("gp_context_new"),
            context_unref: symbol!("gp_context_unref"),
            camera_new: symbol!("gp_camera_new"),
            camera_init: symbol!("gp_camera_init"),
            camera_exit: symbol!("gp_camera_exit"),
            camera_unref: symbol!("gp_camera_unref"),
            camera_autodetect: symbol!("gp_camera_autodetect"),
            camera_get_config: symbol!("gp_camera_get_config"),
            widget_get_child_by_name: symbol!("gp_widget_get_child_by_name"),
            widget_count_children: symbol!("gp_widget_count_children"),
            widget_free: symbol!("gp_widget_free"),
            camera_wait_for_event: symbol!("gp_camera_wait_for_event"),
            camera_file_get: symbol!("gp_camera_file_get"),
            file_new: symbol!("gp_file_new"),
            file_get_data_and_size: symbol!("gp_file_get_data_and_size"),
            file_unref: symbol!("gp_file_unref"),
            list_new: symbol!("gp_list_new"),
            list_count: symbol!("gp_list_count"),
            list_get_name: symbol!("gp_list_get_name"),
            list_unref: symbol!("gp_list_unref"),
            result_as_string: symbol!("gp_result_as_string"),
        })
    })
    .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_photograph_of_the_same_name_is_kept_beside_the_first() {
        let folder = std::env::temp_dir().join(format!("numa-tether-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let first = place(&folder, "DSCF0001.RAF", b"one").unwrap();
        let second = place(&folder, "DSCF0001.RAF", b"two").unwrap();

        let third = place(&folder, "../../DSCF0002.JPG", b"three").unwrap();
        assert_eq!(first, folder.join("DSCF0001.RAF"));
        assert_eq!(second, folder.join("DSCF0001-2.RAF"));
        assert_eq!(third, folder.join("DSCF0002.JPG"));
        assert_eq!(std::fs::read(&first).unwrap(), b"one");
        assert_eq!(std::fs::read(&second).unwrap(), b"two");

        let hidden = std::fs::read_dir(&folder).unwrap().flatten().filter(|entry| entry.file_name().to_string_lossy().starts_with('.')).count();
        assert_eq!(hidden, 0);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_session_says_where_it_is() {
        let (send, receive) = std::sync::mpsc::channel();
        let Some(session) = start(std::env::temp_dir(), move |event| {
            let _ = send.send(event);
        }) else {
            return;
        };
        let event = receive.recv_timeout(Duration::from_secs(10)).expect("a session that says nothing");
        assert!(matches!(event, Event::Waiting(_) | Event::Connected(_)), "{event:?}");
        drop(session);
        let _ = cameras();
    }

    #[test]
    fn a_camera_is_named_by_its_maker_once() {
        assert_eq!(tidy("Fuji Fujifilm X-T5"), "Fujifilm X-T5");
        assert_eq!(tidy("Fuji:Fujifilm X-H2"), "Fujifilm X-H2");
        assert_eq!(tidy("Canon EOS R6"), "Canon EOS R6");
        assert_eq!(tidy("Sony Alpha-A7 IV (Control)"), "Sony Alpha-A7 IV (Control)");
        assert_eq!(tidy("Mass Storage Camera"), "Mass Storage Camera");
    }
}
