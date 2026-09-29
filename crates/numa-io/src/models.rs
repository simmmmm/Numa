pub type ModelFile = (&'static str, &'static str, u64, &'static str);

pub const MODELS: [(&str, &str, &[ModelFile]); if cfg!(target_vendor = "apple") { 11 } else { 13 }] = [
    ("EfficientViT-Seg B2", "Sky, greenery and the other found masks · Apache-2.0 · 61 MB", &[(
        "efficientvit_seg_b2_ade20k_1024.onnx",
        "https://github.com/simmmmm/Numa/releases/download/models/efficientvit_seg_b2_ade20k_1024.onnx",
        61_277_554, "39f11050777fe5562292ca2bfbac344515da28128d4330c6d8c514ca5d5e6efa",
    )]),

    ("SlimSAM", "Click to select · Apache-2.0 · 40 MB", &[
        ("sam_encoder.onnx", "https://huggingface.co/Xenova/slimsam-77-uniform/resolve/main/onnx/vision_encoder.onnx", 23_276_014, "9f8433273a6750b587779baa0cf5508111001bf7e7acfcf585d370139fd366d0"),
        ("sam_decoder.onnx", "https://huggingface.co/Xenova/slimsam-77-uniform/resolve/main/onnx/prompt_encoder_mask_decoder.onnx", 16_557_892, "f4514391764fbd56e08e119060d874ecd7d52994bfb1968af159e12d4943b5bb"),
    ]),
    SUBJECT,

    #[cfg(not(target_vendor = "apple"))]
    ("ViTMatte-S", "Hair and fur in Refine edge · Apache-2.0 · 104 MB", &[(
        "vitmatte_small.onnx",
        "https://huggingface.co/Xenova/vitmatte-small-composition-1k/resolve/main/onnx/model.onnx",
        103_885_865, "bf28d2e0be2c073286e88d60ad649d7123da2749a2d99133fd1098d5887e0225",
    )]),
    ("YuNet", "Finding faces · MIT · 0.2 MB", &[(
        "face_detection_yunet_2023mar.onnx",
        "https://media.githubusercontent.com/media/opencv/opencv_zoo/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx",
        232_589, "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4",
    )]),

    #[cfg(not(target_vendor = "apple"))]
    ("SFace", "Recognising people · Apache-2.0 · 39 MB", &[(
        "face_recognition_sface_2021dec.onnx",
        "https://media.githubusercontent.com/media/opencv/opencv_zoo/main/models/face_recognition_sface/face_recognition_sface_2021dec.onnx",
        38_696_353, "0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79",
    )]),

    ("Open-closed eye", "Eyes closed, in culling · Apache-2.0 · 0.05 MB", &[(
        "open_closed_eye.onnx",
        "https://storage.openvinotoolkit.org/repositories/open_model_zoo/public/2022.1/open-closed-eye-0001/open_closed_eye.onnx",
        46_164, "4daa100034482525a26c9afb9297c16580a531189e66e3d2b2ac7d32becfd593",
    )]),
    ("PP-ResNet50", "Naming the animal in a subject mask · Apache-2.0 · 103 MB", &[(
        "image_classification_ppresnet50_2022jan.onnx",
        "https://media.githubusercontent.com/media/opencv/opencv_zoo/main/models/image_classification_ppresnet/image_classification_ppresnet50_2022jan.onnx",
        102_567_035, "ad5486b0de6c2171ea4d28c734c2fb7c5f64fcdbd97180a0ef515cf4b766a405",
    )]),

    ("LaMa", "Remove: what is under a spot, filled from around it · Apache-2.0 · 208 MB", &[(
        "lama_fp32.onnx",
        "https://huggingface.co/Carve/LaMa-ONNX/resolve/main/lama_fp32.onnx",
        208_044_816, "1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6",
    )]),

    ("YOLOX-s", "Remove people: finding the passers-by · Apache-2.0 · 36 MB", &[(
        "yolox_s.onnx",
        "https://huggingface.co/Heliosoph/yolox-onnx/resolve/main/yolox_s.onnx",
        35_858_002, "c5c2d13e59ae883e6af3b45daea64af4833a4951c92d116ec270d9ddbe998063",
    )]),

    ("Restormer", "AI sharpen: undoing a hand that moved · MIT · 107 MB", &[(
        "restormer_motion_deblurring.onnx",
        "https://github.com/simmmmm/Numa/releases/download/models/restormer_motion_deblurring.onnx",
        107_114_300, "cbaeb199a2a1f2b3d2008cef37cbe411ff9e388bb0700065ff8af25de1545001",
    )]),

    ("RealPLKSR", "Super Resolution: export at twice the size · MIT · 30 MB", &[(
        "realplksr_x2.onnx",
        "https://huggingface.co/darktable-org/upscale-realplksr-onnx/resolve/main/onnx/model_x2.onnx",
        29_627_920, "d7abb65092f3808d3aa255ffdab42b1d672883915d90adbdd321204168b9f293",
    )]),

    ("SCUNet", "AI denoise · Apache-2.0 · 77 MB", &[
        ("scunet_color_real_psnr.onnx", "https://huggingface.co/Heliosoph/scunet-onnx/resolve/main/scunet_color_real_psnr.onnx", 3_798_678, "231be201ab413dbc999d7951caa9844846b93a12a40a41e037d6b5888ed4e88c"),
        ("scunet_color_real_psnr.onnx.data", "https://huggingface.co/Heliosoph/scunet-onnx/resolve/main/scunet_color_real_psnr.onnx.data", 73_138_176, "98825ea1210b641c71e5f052f582c70c49fd44b35387ebe2c034268c17df3feb"),
    ]),
];

#[cfg(not(target_os = "ios"))]
const SUBJECT: (&str, &str, &[ModelFile]) = ("BiRefNet", "The subject, and its edges · MIT · 973 MB", &[(
    "birefnet_f32.onnx",
    "https://huggingface.co/onnx-community/BiRefNet-ONNX/resolve/main/onnx/model.onnx",
    972_666_916, "58f621f00f5d756097615970a88a791584600dcf7c45b18a0a6267535a1ebd3c",
)]);

#[cfg(target_os = "ios")]
const SUBJECT: (&str, &str, &[ModelFile]) = ("BiRefNet lite", "The subject, and its edges · MIT · 192 MB", &[(
    "birefnet_lite_512.onnx",
    "https://github.com/simmmmm/Numa/releases/download/models/birefnet_lite_512.onnx",
    191_762_503, "2c87483165f66ed7980c738eede87a0bd598e088bd7511716e318403e3920dad",
)]);

pub const PROFILES: ModelFile = (
    "rawtherapee-dcpprofiles-5.13.tar.gz",
    "https://github.com/simmmmm/Numa/releases/download/models/rawtherapee-dcpprofiles-5.13.tar.gz",
    67_267_725, "482c0f664fea223028be5291a6ad1577e58d555e49f185f19f4ffacedb8f5c50",
);

pub const MIRROR: &str = "https://github.com/simmmmm/Numa/releases/download/models";

pub const AUTHORS: [(&str, &str); 15] = [
    ("EfficientViT-Seg B2", "MIT Han Lab (Han Cai et al.)"),
    ("SlimSAM", "Zigeng Chen et al., from Meta's Segment Anything"),
    ("BiRefNet", "Peng Zheng et al."),
    ("BiRefNet lite", "Peng Zheng et al."),
    ("IS-Net", "Xuebin Qin et al.; ONNX by rembg"),
    ("ViTMatte-S", "Jingfeng Yao et al. (HUST Vision Lab)"),
    ("YuNet", "Shiqi Yu et al., OpenCV Zoo"),
    ("SFace", "Yaoyao Zhong et al., OpenCV Zoo"),
    ("Open-closed eye", "Intel, Open Model Zoo"),
    ("PP-ResNet50", "PaddlePaddle Authors, OpenCV Zoo"),
    ("LaMa", "Roman Suvorov et al."),
    ("YOLOX-s", "Megvii"),
    ("Restormer", "Syed Waqas Zamir et al."),
    ("RealPLKSR", "Dongheon Lee; ONNX by darktable"),
    ("SCUNet", "Kai Zhang et al."),
];

pub fn licence(description: &str) -> &str {
    description.split(" · ").nth(1).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_has_an_author_and_a_licence() {
        for (name, description, _) in MODELS {
            assert!(AUTHORS.iter().any(|(model, _)| *model == name), "{name} has no author");
            assert!(["MIT", "Apache-2.0", "MIT, Apache-2.0"].contains(&licence(description)), "{name}: {description}");
        }
    }

    #[test]
    #[cfg(target_vendor = "apple")]
    fn nothing_noncommercial_is_offered_where_numa_is_sold() {
        for (name, description, files) in MODELS {
            assert!(!["ViTMatte-S", "SFace", "IS-Net"].contains(&name), "{name}");
            assert!(!description.contains("IS-Net"), "{name}: {description}");
            assert!(files.iter().all(|(file, ..)| !file.contains("isnet") && !file.contains("sface") && !file.contains("vitmatte")), "{name}");
        }
    }

    #[test]
    fn refine_edge_is_vitmatte_on_linux() {
        assert!(MODELS.iter().any(|(name, _, files)| *name == "ViTMatte-S" && files[0].0 == "vitmatte_small.onnx"));
    }
}
