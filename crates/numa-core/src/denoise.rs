use image::{ImageBuffer, Rgb};

pub type Denoised = ImageBuffer<Rgb<u16>, Vec<u16>>;

pub const MODEL: &str = "scunet_color_real_psnr.onnx";

pub const WEIGHTS: &str = "scunet_color_real_psnr.onnx.data";

pub const MODEL_HALF: &str = "scunet_color_real_psnr_fp16.onnx";

pub const SHARPEN_MODEL: &str = "restormer_motion_deblurring.onnx";
