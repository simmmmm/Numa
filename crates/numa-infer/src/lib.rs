use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

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
}

static CARD: Mutex<()> = Mutex::new(());

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

impl Model {

    pub fn load(path: &Path) -> Option<Model> {
        let built = if on_gpu(path) && gpu_registered() {
            let _card = CARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            match build(path, true) {
                Ok(session) => {
                    gpu_stood();
                    Ok((session, true))
                }

                Err(err) => {
                    log::warn!("{}: not on the GPU: {err}", path.display());
                    gpu_fell_back(&err.to_string());
                    build(path, false).map(|session| (session, false))
                }
            }
        } else {
            build(path, false).map(|session| (session, false))
        };
        match built {
            Ok((session, on_card)) => Some(Model { session: Mutex::new(session), on_card }),
            Err(err) => {
                log::warn!("{}: {err}", path.display());
                None
            }
        }
    }

    pub fn on_card(&self) -> bool {
        self.on_card
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

        let _card = self.on_card.then(|| CARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
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

fn build(path: &Path, on_gpu: bool) -> Result<Session, ort::Error> {
    let mut builder = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(threads())?;
    if on_gpu {
        let environment = ort::environment::Environment::current()?;
        let devices: Vec<_> =
            environment.devices().filter(|device| device.ep().is_ok_and(|ep| ep == WEBGPU)).collect();

        if devices.is_empty() {
            return Err(ort::Error::new("the WebGPU provider offered no device"));
        }
        builder = builder.with_devices(devices, None)?;
    }
    builder.commit_from_file(path)
}

fn threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
}

#[cfg(test)]
mod tests {
    use super::on_gpu;
    use std::path::Path;

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
