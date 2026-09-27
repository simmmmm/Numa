use image::{ImageEncoder, Rgb, RgbImage};
use numa_core::document::Document;
use numa_core::space::ColourSpace;

fn chart() -> RgbImage {
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut noise = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 56) as u8
    };
    RgbImage::from_fn(256, 96, |x, y| {
        let code = x as u8;
        match y / 16 {
            0 => Rgb([code, code, code]),
            1 => Rgb([code, 0, 0]),
            2 => Rgb([0, code, 255 - code]),
            3 => Rgb([255, code, 0]),
            4 => Rgb([code / 2 + 100, code / 3 + 60, 40]),
            _ => Rgb([noise(), noise(), noise()]),
        }
    })
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("numa-finished-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn compare(rendered: &RgbImage, file: &RgbImage) -> (u8, f64) {
    assert_eq!(rendered.dimensions(), file.dimensions());
    let mut worst = 0u8;
    let mut squares = 0.0f64;
    for (a, b) in rendered.as_raw().iter().zip(file.as_raw()) {
        worst = worst.max(a.abs_diff(*b));
        squares += (*a as f64 - *b as f64).powi(2);
    }
    let mse = squares / rendered.as_raw().len() as f64;
    (worst, 10.0 * (255.0f64 * 255.0 / mse.max(1e-12)).log10())
}

fn develop(path: &std::path::Path) -> RgbImage {
    let linear = numa_io::raw::decode_linear_any(path).unwrap();
    numa_render::develop(&Document::new(path.display().to_string()), linear, &Default::default())
}

#[test]
fn an_unedited_picture_renders_as_itself() {
    let chart = chart();
    let png = scratch("chart.png");
    chart.save(&png).unwrap();
    let jpeg = scratch("chart.jpg");
    image::codecs::jpeg::JpegEncoder::new_with_quality(std::fs::File::create(&jpeg).unwrap(), 95)
        .write_image(chart.as_raw(), 256, 96, image::ExtendedColorType::Rgb8)
        .unwrap();

    for path in [png, jpeg] {
        let file = image::open(&path).unwrap().into_rgb8();
        let (worst, psnr) = compare(&develop(&path), &file);
        eprintln!("{}: worst {worst} codes, {psnr:.1} dB", path.display());
        assert!(worst <= 2 && psnr >= 45.0, "{}: worst {worst} codes, {psnr:.1} dB", path.display());
    }
}

#[test]
fn exposure_on_a_finished_picture_is_a_stop_of_light() {
    let path = scratch("grey.png");
    RgbImage::from_pixel(8, 8, Rgb([180, 180, 180])).save(&path).unwrap();
    let mut document = Document::new(path.display().to_string());
    document.set_basic(numa_core::document::Basic::with(|b| b.tone.exposure = -1.0));
    let rendered = numa_render::develop(&document, numa_io::raw::decode_linear_any(&path).unwrap(), &Default::default());
    let expected = ColourSpace::Srgb.encode(ColourSpace::Srgb.decode(180.0 / 255.0) / 2.0) * 255.0;
    let got = rendered.get_pixel(4, 4)[0] as f32;
    assert!((got - expected).abs() <= 1.0, "a stop down from 180: {got}, sRGB says {expected:.1}");
}

#[test]
fn a_display_p3_picture_is_read_as_display_p3() {
    let colour = [200u8, 120, 80];
    let path = scratch("p3.png");
    let mut encoder = image::codecs::png::PngEncoder::new(std::fs::File::create(&path).unwrap());
    encoder.set_icc_profile(numa_io::icc::profile(ColourSpace::DisplayP3)).unwrap();
    encoder.write_image(&colour.repeat(64), 8, 8, image::ExtendedColorType::Rgb8).unwrap();

    let p3 = colour.map(|code| ColourSpace::DisplayP3.decode(code as f32 / 255.0));
    let matrix = ColourSpace::DisplayP3.convert_to(ColourSpace::Srgb).unwrap();
    let expected: [f32; 3] = std::array::from_fn(|row| {
        let linear = matrix[row][0] * p3[0] + matrix[row][1] * p3[1] + matrix[row][2] * p3[2];
        ColourSpace::Srgb.encode(linear) * 255.0
    });
    let rendered = develop(&path);
    let got = rendered.get_pixel(4, 4).0;
    for channel in 0..3 {
        assert!(
            (got[channel] as f32 - expected[channel]).abs() <= 1.0,
            "P3 {colour:?} in sRGB is {expected:?}, rendered {got:?}"
        );
    }
}

#[test]
fn auto_leaves_a_finished_pictures_ends_where_they_were() {
    let path = scratch("auto.png");
    chart().save(&path).unwrap();
    let basic = numa_render::auto::tone(&numa_io::raw::decode_linear_any(&path).unwrap(), None).basic;
    assert!(
        basic.tone.exposure >= 0.0 && basic.tone.whites >= 0.0 && basic.tone.blacks <= 0.0,
        "{:?}",
        basic.tone
    );
}
