use std::path::Path;

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
    ("ViTMatte-S", "Hair and fur in Refine edge · non-commercial use only · 104 MB", &[(
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
    ("SFace", "Recognising people · non-commercial use only · 39 MB", &[(
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

pub type Packed = (&'static str, &'static str, u64, &'static str, &'static str);

pub const PACKED: &[Packed] = &[
    ("efficientvit_seg_b2_ade20k_1024.onnx", "efficientvit_seg_b2_ade20k_1024.onnx.zst", 51_821_845, "3a4d705e967243ac951de9c83b6c9451ab2fced60a495eca1e6e50981facede4", "39f11050777fe5562292ca2bfbac344515da28128d4330c6d8c514ca5d5e6efa"),
    ("sam_encoder.onnx", "sam_encoder.onnx.zst", 19_081_276, "43d7de6873157d21be2ce164fea33464c49739dc1c9a612dc63f3f7e9446d859", "9f8433273a6750b587779baa0cf5508111001bf7e7acfcf585d370139fd366d0"),
    ("sam_decoder.onnx", "sam_decoder.onnx.zst", 13_904_033, "fd1a3339e6eb6f468f59c247b3ea44a68e5b0350ace8334e3460925d653fb2bc", "f4514391764fbd56e08e119060d874ecd7d52994bfb1968af159e12d4943b5bb"),
    ("birefnet_f32.onnx", "birefnet_f32.onnx.zst", 742_416_609, "8ffff21f5b0815568c34857b79209a127d901a6f830b2ecddcb4b7e9b5c3c0cc", "58f621f00f5d756097615970a88a791584600dcf7c45b18a0a6267535a1ebd3c"),
    ("birefnet_lite_512.onnx", "birefnet_lite_512.onnx.zst", 149_494_085, "093d8866a83144615a0c5ad6c87ee8efbc19b515e9138f83bb4725313cd6b171", "2c87483165f66ed7980c738eede87a0bd598e088bd7511716e318403e3920dad"),
    ("vitmatte_small.onnx", "vitmatte_small.onnx.zst", 87_512_545, "2945b13f812d0535bdca863b21f2f21829ce9663ebe3a8d4fccc13a4c56820e7", "bf28d2e0be2c073286e88d60ad649d7123da2749a2d99133fd1098d5887e0225"),
    ("face_detection_yunet_2023mar.onnx", "face_detection_yunet_2023mar.onnx.zst", 188_220, "a5b7d4ed05b0f1dc8e202c23c11bfc3ae1101cebb0ab6d0dab0b56759ee0c8b6", "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4"),
    ("face_recognition_sface_2021dec.onnx", "face_recognition_sface_2021dec.onnx.zst", 32_596_224, "bea569f389d3707249e5a056221c2d5bf9c1b6882217e3ec156b13d3895014f0", "0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79"),
    ("open_closed_eye.onnx", "open_closed_eye.onnx.zst", 39_339, "794f1adfd86e7a0d51a33121997ef5e89046112c73380f3daa50e27a22ea5a6d", "4daa100034482525a26c9afb9297c16580a531189e66e3d2b2ac7d32becfd593"),
    ("image_classification_ppresnet50_2022jan.onnx", "image_classification_ppresnet50_2022jan.onnx.zst", 85_931_191, "83b56b8f602a0191dcfe3859ddb0fd89865fe90e1309a9151d7ebaead4a0cab4", "ad5486b0de6c2171ea4d28c734c2fb7c5f64fcdbd97180a0ef515cf4b766a405"),
    ("lama_fp32.onnx", "lama_fp32.onnx.zst", 172_962_575, "25d302af6a071c684d5469f070e45cc9bc28f1021f40f60a5505cccce3b23c6f", "1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6"),
    ("yolox_s.onnx", "yolox_s.onnx.zst", 30_367_755, "53f1f3cdec5e717032b1c21b7b08400065437f7716d7a7c03f7fd8684347c1c3", "c5c2d13e59ae883e6af3b45daea64af4833a4951c92d116ec270d9ddbe998063"),
    ("restormer_motion_deblurring.onnx", "restormer_motion_deblurring.onnx.zst", 88_553_974, "d68c154b6eda0aa3c1e21e11a0b17519dfe4247d0c84bf65453091f32d4a963b", "cbaeb199a2a1f2b3d2008cef37cbe411ff9e388bb0700065ff8af25de1545001"),
    ("realplksr_x2.onnx", "realplksr_x2.onnx.zst", 24_860_003, "d1aee382dce4e67f4d6dc96267c4a78dea29fa7d422bcbbce2a46450e1f3c39d", "d7abb65092f3808d3aa255ffdab42b1d672883915d90adbdd321204168b9f293"),
    ("scunet_color_real_psnr.onnx", "scunet_color_real_psnr.onnx.zst", 101_178, "5f86b2c3e887f87d8d099bc1f1ad734f999d75063c4fb39779441e83e08bf54a", "231be201ab413dbc999d7951caa9844846b93a12a40a41e037d6b5888ed4e88c"),
    ("scunet_color_real_psnr.onnx.data", "scunet_color_real_psnr.onnx.data.f16.zst", 31_709_426, "2c045fe18cd8491163daa9fa2795976bc2b70057e32dd3c5db283aa7bdb8b875", "5a4e1f2b5fcadee1f840fe5322252b22048ad3a977d3d2281ace9c44204cf0b5"),
    ("scunet_color_real_psnr_fp16.onnx", "scunet_color_real_psnr_fp16.onnx.zst", 31_543_155, "7c4bf59460263f8eee39bf5b070b683417e18e0012b981afdbe2b0956452ba2c", "83bd516b28c9d6a3fb733d33fc81c840407764d82d273d43b98903bd21e231cc"),
];

pub fn packed(file: &str) -> Option<&'static Packed> {
    PACKED.iter().find(|(name, ..)| *name == file)
}

pub fn unpack(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::io::{Error, Read, Write};
    let input = std::io::BufReader::new(std::fs::File::open(from)?);
    let mut zstd = ruzstd::decoding::StreamingDecoder::new(input).map_err(Error::other)?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(to)?);
    let mut header = [0; 12];
    zstd.read_exact(&mut header)?;
    if &header[..8] != b"NUMAPACK" {
        return Err(Error::other("not a packed model"));
    }
    let mut ranges = vec![0; u32::from_le_bytes(header[8..].try_into().unwrap()) as usize * 17];
    zstd.read_exact(&mut ranges)?;
    let (mut at, mut packed, mut plain) = (0, Vec::new(), Vec::new());
    for range in ranges.chunks(17) {
        let offset = u64::from_le_bytes(range[..8].try_into().unwrap());
        let length = u64::from_le_bytes(range[8..16].try_into().unwrap()) as usize;
        let (width, stored) = match range[16] {
            1 if length % 4 == 0 => (4, length),
            2 if length % 2 == 0 => (2, length),
            3 if length % 4 == 0 => (2, length / 2),
            kind => return Err(Error::other(format!("packed range of kind {kind}, {length} bytes"))),
        };
        let gap = offset.checked_sub(at).ok_or_else(|| Error::other("packed ranges out of order"))?;
        if std::io::copy(&mut (&mut zstd).take(gap), &mut out)? != gap {
            return Err(Error::other("packed model cut short"));
        }
        packed.resize(stored, 0);
        zstd.read_exact(&mut packed)?;
        plain.resize(length, 0);
        let count = stored / width;
        for index in 0..count {
            if range[16] == 3 {
                let half = half::f16::from_bits(u16::from_le_bytes([packed[index], packed[count + index]]));
                plain[index * 4..index * 4 + 4].copy_from_slice(&half.to_f32().to_le_bytes());
            } else {
                for byte in 0..width {
                    plain[index * width + byte] = packed[byte * count + index];
                }
            }
        }
        out.write_all(&plain)?;
        at = offset + length as u64;
    }
    std::io::copy(&mut zstd, &mut out)?;
    out.into_inner().map_err(|err| err.into_error())?.sync_all()
}

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

pub const NONCOMMERCIAL: &str = "non-commercial use only";

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
            if licence(description) == NONCOMMERCIAL {
                assert!(["ViTMatte-S", "SFace"].contains(&name), "{name}: {description}");
            } else {
                assert!(["MIT", "Apache-2.0", "MIT, Apache-2.0"].contains(&licence(description)), "{name}: {description}");
            }
        }
    }

    #[test]
    #[cfg(not(target_vendor = "apple"))]
    fn the_noncommercial_models_say_so_on_linux() {
        for wanted in ["ViTMatte-S", "SFace"] {
            let (_, description, _) = MODELS.iter().find(|(name, ..)| *name == wanted).unwrap();
            assert_eq!(licence(description), NONCOMMERCIAL, "{wanted}");
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
    fn a_packed_model_unpacks_to_its_file() {
        let bytes = |values: &[f32]| values.iter().flat_map(|value| value.to_le_bytes()).collect::<Vec<u8>>();
        let grouped = |bytes: &[u8], width: usize| {
            (0..width).flat_map(|byte| bytes.iter().skip(byte).step_by(width).copied()).collect::<Vec<u8>>()
        };
        let floats = bytes(&[1.0, -2.5, 1e-3, std::f32::consts::PI]);
        let halves: Vec<u8> = [1.0, 0.1].iter().flat_map(|value| half::f16::from_f32(*value).to_le_bytes()).collect();
        let weights = [0.1, -7.25, 3e-6, 1234.567];
        let as_half: Vec<u8> = weights.iter().flat_map(|value| half::f16::from_f32(*value).to_le_bytes()).collect();
        let widened: Vec<f32> = weights.iter().map(|value| half::f16::from_f32(*value).to_f32()).collect();
        let file = [&b"head"[..], &floats, b"mid", &halves, &bytes(&widened), b"tail"].concat();
        let mut container = b"NUMAPACK".to_vec();
        container.extend(3u32.to_le_bytes());
        for (offset, length, kind) in [(4u64, 16u64, 1u8), (23, 4, 2), (27, 16, 3)] {
            container.extend(offset.to_le_bytes());
            container.extend(length.to_le_bytes());
            container.push(kind);
        }
        container.extend([&b"head"[..], &grouped(&floats, 4), b"mid", &grouped(&halves, 2), &grouped(&as_half, 2), b"tail"].concat());

        let dir = std::env::temp_dir().join(format!("numa-unpack-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (packed, unpacked) = (dir.join("model.onnx.zst"), dir.join("model.onnx"));
        let zstd = ruzstd::encoding::compress_to_vec(&container[..], ruzstd::encoding::CompressionLevel::Fastest);
        std::fs::write(&packed, &zstd).unwrap();
        unpack(&packed, &unpacked).unwrap();
        assert_eq!(std::fs::read(&unpacked).unwrap(), file);

        let cut = ruzstd::encoding::compress_to_vec(&container[..container.len() - 30], ruzstd::encoding::CompressionLevel::Fastest);
        std::fs::write(&packed, &cut).unwrap();
        assert!(unpack(&packed, &unpacked).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore]
    fn unpacks_the_mirror_files() {
        let dir = std::path::PathBuf::from(std::env::var("PACKED_DIR").expect("PACKED_DIR"));
        for (file, name, size, sha256, unpacked) in PACKED {
            let from = dir.join(name);
            assert_eq!(std::fs::metadata(&from).unwrap().len(), *size, "{name}");
            let to = dir.join(format!("{file}.unpacked"));
            let started = std::time::Instant::now();
            unpack(&from, &to).unwrap();
            let took = started.elapsed();
            let digest = |path: &Path| {
                let out = std::process::Command::new("sha256sum").arg(path).output().unwrap();
                String::from_utf8(out.stdout).unwrap().split_whitespace().next().unwrap().to_string()
            };
            assert_eq!(digest(&from), *sha256, "{name}");
            assert_eq!(digest(&to), *unpacked, "{file}");
            println!("{name}: {size} B -> {} B in {took:.2?}", std::fs::metadata(&to).unwrap().len());
            std::fs::remove_file(&to).unwrap();
        }
    }

    #[test]
    #[cfg(not(target_vendor = "apple"))]
    fn refine_edge_is_vitmatte_on_linux() {
        assert!(MODELS.iter().any(|(name, _, files)| *name == "ViTMatte-S" && files[0].0 == "vitmatte_small.onnx"));
    }
}
