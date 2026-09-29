pub mod rewrite;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use ndarray::ArrayD;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::{Session, SessionInputValue};
use ort::value::Tensor;

pub const GPU_PLUGIN: &str = "libonnxruntime_providers_webgpu.so";

const WEBGPU: &str = "WebGpuExecutionProvider";

const ON_GPU: &[&str] = &[
    "efficientvit_seg_b2_ade20k_1024.onnx",
    "sam_encoder.onnx",
    "vision_encoder.onnx",
    "scunet_color_real_psnr.onnx",
    "scunet_color_real_psnr_fp16.onnx",

    "realplksr_x2.onnx",

    "lama_fp32.onnx",

    "restormer_motion_deblurring.onnx",

    "isnet.onnx",
    "isnet-general-use.onnx",

    "birefnet_f32.onnx",
    "birefnet.onnx",

    "vitmatte_small.onnx",
];

struct Gpu {

    plugin: PathBuf,

    guard: PathBuf,
}

static GPU: OnceLock<Gpu> = OnceLock::new();

static REPORT: Mutex<Option<String>> = Mutex::new(None);

pub struct Model {
    session: Mutex<Session>,

    on_card: bool,

    edge: Option<usize>,

    sized: Option<(usize, usize)>,
}

static CARD: RwLock<()> = RwLock::new(());

pub fn try_card() -> Option<std::sync::RwLockReadGuard<'static, ()>> {
    match CARD.try_read() {
        Ok(guard) => Some(guard),
        Err(std::sync::TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => None,
    }
}

pub fn card_within(patience: std::time::Duration) -> Option<std::sync::RwLockReadGuard<'static, ()>> {
    let deadline = std::time::Instant::now() + patience;
    loop {
        if let Some(guard) = try_card() {
            return Some(guard);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }

        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

pub enum Input {
    F32(ArrayD<f32>),
    I64(ArrayD<i64>),
}

impl From<ArrayD<f32>> for Input {
    fn from(array: ArrayD<f32>) -> Self {
        Input::F32(array)
    }
}

impl From<ArrayD<i64>> for Input {
    fn from(array: ArrayD<i64>) -> Self {
        Input::I64(array)
    }
}

pub struct Kept {
    slot: Mutex<Option<Option<Arc<Model>>>>,

    used: AtomicU64,
}

static KEPT: Mutex<Vec<&'static Kept>> = Mutex::new(Vec::new());

fn clock() -> u64 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

impl Kept {
    pub const fn new() -> Self {
        Self { slot: Mutex::new(None), used: AtomicU64::new(0) }
    }

    pub fn get_or_init(&'static self, load: impl FnOnce() -> Option<Model>) -> Option<Arc<Model>> {
        let mut slot = self.slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.used.store(clock(), Ordering::Relaxed);
        if slot.is_none() {
            *slot = Some(load().map(Arc::new));
            let mut kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if !kept.iter().any(|other| std::ptr::eq(*other, self)) {
                kept.push(self);
            }
        }
        slot.clone().flatten()
    }

    pub fn release(&self) {
        self.slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
    }

    pub fn loaded(&self) -> Option<bool> {
        self.slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref().map(Option::is_some)
    }
}

impl Default for Kept {
    fn default() -> Self {
        Self::new()
    }
}

pub fn release_all() {
    release_idle(std::time::Duration::ZERO);
}

pub fn release_idle(idle: std::time::Duration) -> usize {
    let now = clock();

    let kept = std::mem::take(&mut *KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
    let mut stay = Vec::new();
    let mut released = 0;
    for model in kept {
        let mut slot = model.slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if idle.is_zero() || now.saturating_sub(model.used.load(Ordering::Relaxed)) >= idle.as_millis() as u64 {
            released += slot.take().is_some_and(|loaded| loaded.is_some()) as usize;
        } else {
            stay.push(model);
        }
    }
    let mut kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    for model in stay {
        if !kept.iter().any(|other| std::ptr::eq(*other, model)) {
            kept.push(model);
        }
    }
    released
}

impl Model {

    pub fn load(path: &Path) -> Option<Model> {
        Self::load_at(path, None)
    }

    pub fn load_sized(path: &Path, size: (usize, usize)) -> Option<Model> {
        Self::load_at(path, Some(size))
    }

    pub fn answers(&self, size: (usize, usize)) -> bool {
        self.sized.is_none_or(|sized| sized == size)
    }

    fn load_at(path: &Path, size: Option<(usize, usize)>) -> Option<Model> {
        let built = if on_gpu(path) && gpu_registered() {
            let _card = CARD.write().unwrap_or_else(|poisoned| poisoned.into_inner());
            match build(path, true, size) {
                Ok(session) => {
                    gpu_stood();
                    Ok((session, true))
                }

                Err(err) => {
                    log::warn!("{}: not on the GPU: {err}", path.display());
                    gpu_fell_back(&err.to_string());
                    build(path, false, None).map(|session| (session, false))
                }
            }
        } else {
            build(path, false, None).map(|session| (session, false))
        };
        match built {
            Ok((session, on_card)) => {
                let sized = size.filter(|_| on_card);
                Some(Model { session: Mutex::new(session), on_card, edge: fixed_edge(path), sized })
            }
            Err(err) => {
                log::warn!("{}: {err}", path.display());
                None
            }
        }
    }

    pub fn on_card(&self) -> bool {
        self.on_card
    }

    pub fn edge(&self) -> Option<usize> {
        self.edge
    }

    pub fn side(&self) -> Option<usize> {
        let session = self.session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let dims = session.inputs().first()?.dtype().tensor_shape()?.to_vec();
        match dims[..] {
            [_, _, height, width] if height == width && height > 0 => Some(height as usize),
            _ => None,
        }
    }

    pub fn describe(&self) -> Vec<String> {
        let session = self.session.lock().expect("model");
        let shape = |dims: &[i64]| {
            dims.iter()
                .map(|d| match *d < 0 {
                    true => "?".to_string(),
                    false => d.to_string(),
                })
                .collect::<Vec<_>>()
                .join("x")
        };
        session
            .inputs()
            .iter()
            .map(|port| {
                let dims = port.dtype().tensor_shape().map(|d| shape(d)).unwrap_or_default();
                format!("in  {} {dims}", port.name())
            })
            .chain(session.outputs().iter().map(|port| {
                let dims = port.dtype().tensor_shape().map(|d| shape(d)).unwrap_or_default();
                format!("out {} {dims}", port.name())
            }))
            .collect()
    }

    pub fn run(&self, inputs: Vec<Input>) -> Result<Vec<ArrayD<f32>>, ort::Error> {
        let values = inputs
            .into_iter()
            .map(|input| match input {
                Input::F32(array) => Tensor::from_array(array).map(SessionInputValue::from),
                Input::I64(array) => Tensor::from_array(array).map(SessionInputValue::from),
            })
            .collect::<Result<Vec<_>, _>>()?;

        let _card = self.on_card.then(|| CARD.write().unwrap_or_else(|poisoned| poisoned.into_inner()));
        let mut session = self.session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let outputs = session.run(&values[..])?;
        (0..outputs.len())
            .map(|index| outputs[index].try_extract_array::<f32>().map(|view| view.to_owned()))
            .collect()
    }
}

pub fn enable_gpu(models: &Path, guard: &Path) {
    let plugin = gpu_plugin(models).unwrap_or_else(|| models.join(GPU_PLUGIN));
    let _ = GPU.set(Gpu { plugin, guard: guard.to_path_buf() });
}

pub fn gpu_plugin(models: &Path) -> Option<PathBuf> {
    let shipped = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.parent()?.join("lib")));
    first_plugin(shipped.as_deref(), models)
}

fn first_plugin(shipped: Option<&Path>, models: &Path) -> Option<PathBuf> {
    shipped.into_iter().chain([models]).map(|dir| dir.join(GPU_PLUGIN)).find(|path| path.is_file())
}

pub fn gpu_report() -> Option<String> {
    REPORT.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
}

fn on_gpu(path: &Path) -> bool {

    #[cfg(feature = "dump-switches")]
    if let Ok(extra) = std::env::var("NUMA_GPU_ALSO") {
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        if extra.split(',').any(|wanted| wanted.trim() == name) {
            return true;
        }
    }
    path.file_name().is_some_and(|name| ON_GPU.iter().any(|wanted| name == *wanted))
}

fn gpu_registered() -> bool {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    *REGISTERED.get_or_init(|| {
        let Some(gpu) = GPU.get() else { return false };
        if !gpu.plugin.is_file() {
            return false;
        }

        if let Err(err) = std::fs::write(&gpu.guard, b"") {
            log::warn!("no GPU: the safe-mode marker could not be written: {err}");
            return false;
        }
        match ort::environment::Environment::current()
            .and_then(|env| env.register_ep_library(WEBGPU, &gpu.plugin))
        {
            Ok(_) => true,
            Err(err) => {
                log::warn!("no GPU: {} could not be registered: {err}", gpu.plugin.display());
                gpu_fell_back(&err.to_string());
                false
            }
        }
    })
}

fn gpu_stood() {
    let Some(gpu) = GPU.get() else { return };
    let _ = std::fs::remove_file(&gpu.guard);
    let mut report = REPORT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if report.is_none() {
        let line = format!("GPU: masks and denoise on {} (WebGPU)", device_name());
        log::info!("{line}");
        *report = Some(line);
    }
}

fn gpu_fell_back(why: &str) {
    if let Some(gpu) = GPU.get() {
        let _ = std::fs::remove_file(&gpu.guard);
    }
    let first = why.lines().next().unwrap_or(why);
    let mut report = REPORT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *report = Some(format!("GPU: fell back to the processor — {first}"));
}

fn device_name() -> String {
    ort::environment::Environment::current()
        .ok()
        .and_then(|env| {
            env.devices().find(|device| device.ep().is_ok_and(|ep| ep == WEBGPU)).map(|device| {
                let hardware = device.hardware_device();
                let vendor = hardware.vendor().unwrap_or_default();
                match vendor.trim() {
                    "" => format!("the {:?}", hardware.ty()),
                    vendor => format!("the {vendor} {:?}", hardware.ty()),
                }
            })
        })
        .unwrap_or_else(|| "a graphics card".to_string())
}

fn build(path: &Path, on_gpu: bool, size: Option<(usize, usize)>) -> Result<Session, ort::Error> {
    let mut builder = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(threads())?

        .with_intra_op_spinning(false)?;
    if on_gpu {
        let environment = ort::environment::Environment::current()?;

        let mut webgpu: Vec<_> =
            environment.devices().filter(|device| device.ep().is_ok_and(|ep| ep == WEBGPU)).collect();
        if webgpu.is_empty() {
            return Err(ort::Error::new("the WebGPU provider offered no device"));
        }

        let software = |device: &ort::device::Device| {
            let hardware = device.hardware_device();
            let vendor = hardware.vendor().unwrap_or_default().to_lowercase();
            ["llvmpipe", "lavapipe", "swiftshader", "software"].iter().any(|name| vendor.contains(name))
                || format!("{:?}", hardware.ty()).to_lowercase().contains("cpu")
        };
        let at = webgpu.iter().position(|device| !software(device)).unwrap_or(0);
        builder = builder.with_devices(vec![webgpu.swap_remove(at)], None)?;

        builder = builder.with_dimension_override("batch_size", 1)?.with_dimension_override("batch", 1)?;

        if let Some((height, width)) = size {
            builder = builder.with_dimension_override("height", height as i64)?.with_dimension_override("width", width as i64)?;
        }
    }

    #[cfg(target_os = "ios")]
    if on_core_ml(path) {
        use ort::ep::coreml::{ComputeUnits, ModelFormat};
        let edge = CORE_ML_EDGE as i64;
        let builder = builder
            .with_dimension_override("batch", 1)?
            .with_dimension_override("height", edge)?
            .with_dimension_override("width", edge)?;
        let compiled = numa_core_cache().join("coreml");
        let _ = std::fs::create_dir_all(&compiled);
        let coreml = ort::ep::CoreML::default()
            .with_model_format(ModelFormat::MLProgram)
            .with_compute_units(ComputeUnits::All)
            .with_model_cache_dir(compiled.display());

        let refused = match builder.with_execution_providers([coreml.build()]) {
            Ok(mut with_core_ml) => match with_core_ml.commit_from_file(path) {
                Ok(session) => return Ok(session),
                Err(err) => err.to_string(),
            },
            Err(err) => err.to_string(),
        };
        log::warn!("{}: not through Core ML: {refused}", path.display());
        return Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(threads())?
            .with_intra_op_spinning(false)?
            .with_dimension_override("batch", 1)?
            .with_dimension_override("height", edge)?
            .with_dimension_override("width", edge)?
            .commit_from_file(path);
    }

    if !on_gpu && path.file_name().is_some_and(|name| LEAN.iter().any(|lean| name == *lean)) {
        builder = builder.with_execution_providers([ort::ep::CPU::default().with_arena_allocator(false).build()])?;
    }
    builder.commit_from_file(path)
}

const LEAN: &[&str] = &[
    "birefnet_f32.onnx",
    "birefnet.onnx",
    "birefnet_lite.onnx",
    "birefnet_lite_512.onnx",
    "sam_encoder.onnx",
    "vision_encoder.onnx",
    "isnet.onnx",
    "efficientvit_seg_b2_ade20k_1024.onnx",
    "vitmatte_small.onnx",
    "sam_decoder.onnx",
];

#[cfg(target_os = "ios")]
const ON_CORE_ML: &[&str] = &[
    "scunet_color_real_psnr.onnx",
    "scunet_color_real_psnr_fp16.onnx",
    "restormer_motion_deblurring.onnx",
    "realplksr_x2.onnx",
    "lama_fp32.onnx",
];

#[cfg(target_os = "ios")]
fn on_core_ml(path: &Path) -> bool {
    path.file_name().is_some_and(|name| ON_CORE_ML.iter().any(|wanted| name == *wanted))
}

#[cfg(target_os = "ios")]
const CORE_ML_EDGE: usize = 256;

fn fixed_edge(_path: &Path) -> Option<usize> {
    #[cfg(target_os = "ios")]
    if on_core_ml(_path) {
        return Some(CORE_ML_EDGE);
    }
    None
}

#[cfg(target_os = "ios")]
fn numa_core_cache() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Library/Caches/numa")
}

fn threads() -> usize {
    if let Some(asked) = std::env::var("NUMA_MODEL_THREADS").ok().and_then(|value| value.parse().ok()) {
        return asked;
    }
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
}

#[cfg(test)]
mod tests {
    use super::on_gpu;
    use std::path::Path;

    #[test]
    fn idle_models_are_let_go() {
        static RECENT: super::Kept = super::Kept::new();
        static LOADS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let load = || {
            LOADS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            None
        };
        RECENT.get_or_init(load);
        assert_eq!(RECENT.loaded(), Some(false));
        super::release_idle(std::time::Duration::from_secs(600));
        assert_eq!(RECENT.loaded(), Some(false), "asked for a moment ago");
        super::release_idle(std::time::Duration::ZERO);
        assert_eq!(RECENT.loaded(), None, "let go");
        RECENT.get_or_init(load);
        assert_eq!(LOADS.load(std::sync::atomic::Ordering::Relaxed), 2, "asked again");
    }

    #[test]
    fn the_shipped_plugin_comes_first() {
        let root = std::env::temp_dir().join("numa-gpu-plugin-test");
        let _ = std::fs::remove_dir_all(&root);
        let (shipped, models) = (root.join("lib"), root.join("models"));
        std::fs::create_dir_all(&shipped).unwrap();
        std::fs::create_dir_all(&models).unwrap();
        assert_eq!(super::first_plugin(Some(&shipped), &models), None);
        std::fs::write(models.join(super::GPU_PLUGIN), b"").unwrap();
        assert_eq!(super::first_plugin(Some(&shipped), &models), Some(models.join(super::GPU_PLUGIN)));
        std::fs::write(shipped.join(super::GPU_PLUGIN), b"").unwrap();
        assert_eq!(super::first_plugin(Some(&shipped), &models), Some(shipped.join(super::GPU_PLUGIN)));
        assert_eq!(super::first_plugin(None, &models), Some(models.join(super::GPU_PLUGIN)));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_the_heavy_models_ask_for_the_gpu() {
        let dir = Path::new("/home/someone/.local/share/numa/models");
        for name in [
            "efficientvit_seg_b2_ade20k_1024.onnx",
            "sam_encoder.onnx",
            "vision_encoder.onnx",
            "scunet_color_real_psnr.onnx",
            "isnet.onnx",
            "isnet-general-use.onnx",
            "birefnet_f32.onnx",
            "birefnet.onnx",
            "vitmatte_small.onnx",
        ] {
            assert!(on_gpu(&dir.join(name)), "{name} should be on the GPU");
        }
        for name in [
            "sam_decoder.onnx",
            "prompt_encoder_mask_decoder.onnx",
            "face_detection_yunet_2023mar.onnx",
            "face_recognition_sface_2021dec.onnx",
            "image_classification_ppresnet50_2022jan.onnx",
            "scunet_color_real_psnr.onnx.data",
        ] {
            assert!(!on_gpu(&dir.join(name)), "{name} was measured slower on the GPU");
        }
    }

    #[test]
    #[ignore]
    fn the_other_two_models() {
        if let Ok(path) = std::env::var("FACES") {
            let photo = image::open(path).unwrap().to_rgb8();
            let started = std::time::Instant::now();
            let faces = numa_cull::faces::detect(&photo);
            println!("faces: {:?} in {:?}", faces.as_ref().map(|f| f.len()), started.elapsed());
            for face in faces.unwrap_or_default() {
                println!("  {face:?}");
            }
        }

        if let Ok(path) = std::env::var("RAF") {
            for one in path.split(':') {
                let image = numa_io::raw::load_scaled(std::path::Path::new(one), 640).unwrap();
                let faces = numa_cull::faces::detect(&image);
                println!("faces in {one} at the cull's size: {:?}", faces.map(|f| f.len()));
            }
        }
        if let Ok(path) = std::env::var("BIRD") {
            let photo = image::open(path).unwrap().to_rgb8();
            let started = std::time::Instant::now();
            let embedding = numa_render::sam::encode(&photo).expect("sam installed");
            println!("sam encode in {:?}", started.elapsed());
            let started = std::time::Instant::now();
            let alpha = embedding.at(0.565, 0.34).expect("a mask");
            let share: f64 = alpha.data.iter().map(|v| *v as f64).sum::<f64>() / alpha.data.len() as f64;
            println!("sam click on the bird: {:.2}% of the frame in {:?}", share * 100.0, started.elapsed());
        }
    }
}
