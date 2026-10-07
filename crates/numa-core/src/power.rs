use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
    run_on(pool, work)
}

fn run_on<R: Send>(pool: &Option<rayon::ThreadPool>, work: impl FnOnce() -> R + Send) -> R {
    match pool {
        Some(pool) => pool.install(|| stoppable(None, work)),
        None => work(),
    }
}

thread_local! {

    static STOP: std::cell::RefCell<Option<Arc<AtomicBool>>> = const { std::cell::RefCell::new(None) };
}

pub fn stoppable<R>(stop: Option<Arc<AtomicBool>>, work: impl FnOnce() -> R) -> R {
    struct Back(Option<Arc<AtomicBool>>);
    impl Drop for Back {
        fn drop(&mut self) {
            STOP.set(self.0.take());
        }
    }
    let _back = Back(STOP.replace(stop));
    work()
}

pub fn stopped() -> bool {
    STOP.with_borrow(|stop| stop.as_ref().is_some_and(|stop| stop.load(Ordering::Relaxed)))
}

pub const QUIET_THREADS: usize = 2;

const REST: u32 = 2;

pub fn quietly<R: Send>(work: impl FnOnce() -> R + Send) -> R {
    static LANE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    static POOL: std::sync::OnceLock<Option<rayon::ThreadPool>> = std::sync::OnceLock::new();
    let _turn = LANE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let pool = POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(QUIET_THREADS)
            .thread_name(|k| format!("numa-quiet-{k}"))
            .start_handler(|_| lowest_priority())
            .build()
            .ok()
    });
    let started = std::time::Instant::now();
    let result = run_on(pool, work);

    std::thread::sleep((started.elapsed() * REST).min(std::time::Duration::from_secs(30)));
    result
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

#[cfg(target_os = "linux")]
fn lowest_priority() {

    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 19);
    }
}

#[cfg(not(target_os = "linux"))]
fn lowest_priority() {
    lower_priority();
}

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

#[cfg(test)]
mod tests {
    use super::{quietly, run_on, stoppable, stopped};
    use std::sync::{atomic::AtomicBool, Arc};
    use std::time::{Duration, Instant};

    #[test]
    fn a_job_run_inside_a_let_go_decode_is_not_let_go() {
        let pool = Some(rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap());
        let other = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
        let (pool, other) = (&pool, &other);
        let (waiting, waited) = std::sync::mpsc::channel();
        let (answer, answered) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let let_go = scope.spawn(move || {
                run_on(pool, move || {
                    stoppable(Some(Arc::new(AtomicBool::new(true))), move || {

                        let inside = other.install(move || {
                            waiting.send(()).unwrap();
                            answered.recv_timeout(Duration::from_secs(10)).is_ok()
                        });
                        (inside, stopped())
                    })
                })
            });
            waited.recv().unwrap();
            let seen = run_on(pool, stopped);
            let _ = answer.send(());
            let (inside, own) = let_go.join().unwrap();
            assert!(inside, "the second job was to run inside the first one's wait");
            assert!(!seen, "the second job saw the first one's flag");
            assert!(own, "the first job lost its flag");
        });
    }

    #[test]
    fn a_quiet_job_rests_and_the_next_waits_for_it() {
        let started = Instant::now();
        std::thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| quietly(|| std::thread::sleep(Duration::from_millis(40))));
            }
        });

        assert!(started.elapsed() >= Duration::from_millis(240), "{:?}", started.elapsed());
    }
}
