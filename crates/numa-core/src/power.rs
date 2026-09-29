use std::sync::atomic::{AtomicBool, Ordering};

static FRUGAL: AtomicBool = AtomicBool::new(false);

pub fn frugal() -> bool {
    FRUGAL.load(Ordering::Relaxed)
}

pub fn set_frugal(on: bool) {
    FRUGAL.store(on, Ordering::Relaxed);
}

pub fn background<R: Send>(work: impl FnOnce() -> R + Send) -> R {
    static ALL: std::sync::OnceLock<Option<rayon::ThreadPool>> = std::sync::OnceLock::new();
    static QUARTER: std::sync::OnceLock<Option<rayon::ThreadPool>> = std::sync::OnceLock::new();
    let (pool, share) = if frugal() { (&QUARTER, 4) } else { (&ALL, 1) };
    let pool = pool.get_or_init(|| {
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        rayon::ThreadPoolBuilder::new()
            .num_threads((cores / share).max(1))
            .thread_name(|k| format!("numa-background-{k}"))
            .start_handler(|_| lower_priority())
            .build()
            .ok()
    });
    match pool {
        Some(pool) => pool.install(work),
        None => work(),
    }
}

pub fn start_threads() {
    let _ = rayon::ThreadPoolBuilder::new()
        .start_handler(|_| waited_for())
        .build_global();
}

#[cfg(target_os = "linux")]
fn lower_priority() {

    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
}

#[cfg(target_vendor = "apple")]
fn lower_priority() {

    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_BACKGROUND, 0);
    }
}

#[cfg(target_vendor = "apple")]
fn waited_for() {

    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INITIATED, 0);
    }
}

#[cfg(not(any(target_os = "linux", target_vendor = "apple")))]
fn lower_priority() {}

static PHONE: AtomicBool = AtomicBool::new(false);

pub fn phone() -> bool {
    PHONE.load(Ordering::Relaxed)
}

pub fn set_phone(on: bool) {
    PHONE.store(on, Ordering::Relaxed);
}

#[cfg(target_os = "ios")]
pub fn available_memory() -> Option<u64> {
    unsafe extern "C" {
        fn os_proc_available_memory() -> usize;
    }

    let room = unsafe { os_proc_available_memory() } as u64;
    (room > 0).then_some(room)
}

#[cfg(not(target_os = "ios"))]
pub fn available_memory() -> Option<u64> {
    None
}
#[cfg(not(target_vendor = "apple"))]
fn waited_for() {}
