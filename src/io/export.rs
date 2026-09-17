use std::path::{Path, PathBuf};

use image::RgbImage;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    Jpeg,
    Png,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Jpeg => "jpg",
            Format::Png => "png",
        }
    }

    pub fn is_lossy(self) -> bool {
        self == Format::Jpeg
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Size {
    Full,

    LongEdge(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportSettings {

    pub template: String,
    pub format: Format,
    pub quality: u8,
    pub size: Size,

    pub sharpen: bool,

    pub metadata: bool,

    pub folder: Option<PathBuf>,

    #[serde(default)]
    pub space: crate::core::space::ColourSpace,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            template: "{stem} - edited #{index}".to_string(),
            format: Format::Jpeg,
            quality: 92,
            size: Size::Full,
            sharpen: true,
            metadata: true,
            folder: None,
            space: crate::core::space::ColourSpace::Srgb,
        }
    }
}

impl ExportSettings {

    pub fn summary(&self) -> String {
        let size = match self.size {
            Size::Full => "full size".to_string(),
            Size::LongEdge(edge) => format!("{edge} px"),
        };
        match self.format {
            Format::Jpeg => format!("JPEG {}, {size}", self.quality),
            Format::Png => format!("PNG, {size}"),
        }
    }
}

pub fn render_name(template: &str, source: &Path, index: u32, format: Format) -> String {
    let stem = source.file_stem().and_then(|stem| stem.to_str()).unwrap_or("image");
    format!(
        "{}.{}",
        template
            .replace("{stem}", stem)
            .replace("{index}", &format!("{index:03}")),
        format.extension()
    )
}

pub fn next_path(dir: &Path, source: &Path, settings: &ExportSettings) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|err| format!("{}: {}", dir.display(), err))?;

    for index in 1..10_000 {
        let candidate = dir.join(render_name(&settings.template, source, index, settings.format));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(format!("no free file name in {}", dir.display()))
}

pub fn fit(image: RgbImage, settings: &ExportSettings) -> RgbImage {
    let Size::LongEdge(edge) = settings.size else { return image };
    let longest = image.width().max(image.height());
    if edge == 0 || longest <= edge {
        return image;
    }

    let scale = edge as f32 / longest as f32;
    let width = ((image.width() as f32 * scale).round() as u32).max(1);
    let height = ((image.height() as f32 * scale).round() as u32).max(1);
    let mut reduced =
        image::imageops::resize(&image, width, height, image::imageops::FilterType::Lanczos3);

    if settings.sharpen {
        sharpen_for_output(&mut reduced);
    }
    reduced
}

fn sharpen_for_output(image: &mut RgbImage) {
    const AMOUNT: f32 = 0.35;
    const RADIUS: f32 = 0.7;

    const THRESHOLD: f32 = 0.08;

    let (width, height) = (image.width() as usize, image.height() as usize);
    let to_linear = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let to_display = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        let v = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };

    let mut data: Vec<f32> = image.iter().map(|value| to_linear(*value)).collect();
    crate::render::detail::sharpen(&mut data, width, height, AMOUNT, RADIUS, THRESHOLD, 1.0);
    for (byte, value) in image.iter_mut().zip(data) {
        *byte = to_display(value);
    }
}

pub fn save(
    image: &RgbImage,
    path: &Path,
    settings: &ExportSettings,
    source: Option<&Path>,
) -> Result<(), String> {
    let fail = |err: String| format!("{}: {}", path.display(), err);

    let mut bytes: Vec<u8> = Vec::new();
    match settings.format {
        Format::Jpeg => JpegEncoder::new_with_quality(&mut bytes, settings.quality.clamp(1, 100))
            .encode_image(image)
            .map_err(|err| fail(err.to_string()))?,
        Format::Png => image
            .write_with_encoder(PngEncoder::new(&mut bytes))
            .map_err(|err| fail(err.to_string()))?,
    }

    if settings.metadata && settings.format == Format::Jpeg {
        if let Some(exif) = source.and_then(exif_block) {
            bytes = with_exif(bytes, &exif);
        }
    }

    std::fs::write(path, &bytes).map_err(|err| fail(err.to_string()))
}

pub fn exif_block(source: &Path) -> Option<Vec<u8>> {
    use std::io::Read;

    const SCAN: u64 = 4 * 1024 * 1024;

    const LIMIT: usize = 65_535 + 2;

    let file = std::fs::File::open(source).ok()?;
    let mut head = Vec::new();
    file.take(SCAN).read_to_end(&mut head).ok()?;

    for start in 0..head.len().saturating_sub(16) {

        if head[start..start + 4] != [0xFF, 0xD8, 0xFF, 0xE1] {
            continue;
        }
        if head.get(start + 6..start + 12)? != b"Exif\0\0" {
            continue;
        }

        if !matches!(head.get(start + 12..start + 14), Some(b"II") | Some(b"MM")) {
            continue;
        }

        let at = start + 2;
        let length = u16::from_be_bytes([head[at + 2], head[at + 3]]) as usize;
        let segment = head.get(at..at + 2 + length)?;
        if segment.len() > LIMIT {
            return None;
        }

        let mut exif = segment.to_vec();
        tidy_exif(&mut exif);
        return Some(exif);
    }

    exif_from_tiff(source)
}

fn exif_from_tiff(source: &Path) -> Option<Vec<u8>> {
    use rawler::formats::tiff::writer::TiffWriter;
    use rawler::formats::tiff::{Value, IFD};
    use std::io::Cursor;

    const EXIF_IFD: u16 = 0x8769;
    const GPS_IFD: u16 = 0x8825;
    const ORIENTATION: u16 = 0x0112;

    const MAKER_NOTE: u16 = 0x927C;

    const INTEROP_IFD: u16 = 0xA005;

    const KEEP: [u16; 11] = [
        0x010E,
        0x010F,
        0x0110,
        0x0112,
        0x011A,
        0x011B,
        0x0128,
        0x0131,
        0x0132,
        0x013B,
        0x8298,
    ];

    let file = std::fs::File::open(source).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let root = IFD::new_root_with_correction(&mut reader, 0, 0, 0, 10, &[EXIF_IFD, GPS_IFD]).ok()?;
    if root.get_entry_recursive(0x010Fu16).is_none() {

        return None;
    }

    let mut buffer = Cursor::new(Vec::new());
    let mut tiff = TiffWriter::new(&mut buffer).ok()?;

    let mut pointers: Vec<(u16, u32)> = Vec::new();
    for tag in [EXIF_IFD, GPS_IFD] {
        let Some(source) = root.get_sub_ifd(tag) else { continue };
        let mut sub = tiff.new_directory();
        for (entry, value) in source.value_iter() {
            if matches!(*entry, MAKER_NOTE | INTEROP_IFD) {
                continue;
            }
            sub.add_untyped_tag(*entry, value.clone());
        }
        if sub.is_empty() {
            continue;
        }
        let offset = sub.build(&mut tiff).ok()?;
        pointers.push((tag, offset));
    }

    let mut ifd0 = tiff.new_directory();
    for (entry, value) in root.value_iter() {
        if KEEP.contains(entry) {
            ifd0.add_untyped_tag(*entry, value.clone());
        }
    }

    ifd0.add_untyped_tag(ORIENTATION, Value::Short(vec![1]));
    for (tag, offset) in pointers {
        ifd0.add_untyped_tag(tag, Value::Long(vec![offset]));
    }
    tiff.build(ifd0).ok()?;

    let tiff = buffer.into_inner();
    let length = 2 + 6 + tiff.len();
    let size = u16::try_from(length).ok()?;

    let mut block = Vec::with_capacity(length + 2);
    block.extend_from_slice(&[0xFF, 0xE1]);
    block.extend_from_slice(&size.to_be_bytes());
    block.extend_from_slice(b"Exif\0\0");
    block.extend_from_slice(&tiff);
    Some(block)
}

fn tidy_exif(exif: &mut [u8]) {

    const TIFF: usize = 10;
    let Some(header) = exif.get(TIFF..TIFF + 8) else { return };
    let big = match &header[..2] {
        b"MM" => true,
        b"II" => false,
        _ => return,
    };
    let read_u16 = |bytes: &[u8]| {
        if big {
            u16::from_be_bytes([bytes[0], bytes[1]])
        } else {
            u16::from_le_bytes([bytes[0], bytes[1]])
        }
    };
    let read_u32 = |bytes: &[u8]| {
        let raw = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if big { u32::from_be_bytes(raw) } else { u32::from_le_bytes(raw) }
    };

    let ifd0 = TIFF + read_u32(&header[4..8]) as usize;
    let Some(count) = exif.get(ifd0..ifd0 + 2).map(read_u16) else { return };
    let entries = ifd0 + 2;

    for index in 0..count as usize {
        let entry = entries + index * 12;
        let Some(bytes) = exif.get(entry..entry + 12) else { return };

        if read_u16(&bytes[..2]) == 0x0112 {
            let one = if big { [0, 1] } else { [1, 0] };
            exif[entry + 8..entry + 10].copy_from_slice(&one);
        }
    }

    let next = entries + count as usize * 12;
    if next + 4 <= exif.len() {
        exif[next..next + 4].copy_from_slice(&[0, 0, 0, 0]);
    }
}

fn with_exif(jpeg: Vec<u8>, exif: &[u8]) -> Vec<u8> {
    if jpeg.len() < 2 || jpeg[..2] != [0xFF, 0xD8] {
        return jpeg;
    }
    let mut out = Vec::with_capacity(jpeg.len() + exif.len());
    out.extend_from_slice(&jpeg[..2]);
    out.extend_from_slice(exif);
    out.extend_from_slice(&jpeg[2..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_templated_and_never_collide() {
        let dir = std::env::temp_dir().join("numa-export-test");
        let _ = std::fs::remove_dir_all(&dir);
        let source = Path::new("/photos/DSCF5591.RAF");
        let settings = ExportSettings::default();

        assert_eq!(
            render_name(&settings.template, source, 7, Format::Jpeg),
            "DSCF5591 - edited #007.jpg"
        );

        assert_eq!(
            render_name(&settings.template, source, 7, Format::Png),
            "DSCF5591 - edited #007.png"
        );

        let first = next_path(&dir, source, &settings).unwrap();
        assert_eq!(first.file_name().unwrap(), "DSCF5591 - edited #001.jpg");

        let image = RgbImage::from_pixel(4, 3, image::Rgb([10, 20, 30]));
        save(&image, &first, &settings, None).unwrap();

        let second = next_path(&dir, source, &settings).unwrap();
        assert_eq!(second.file_name().unwrap(), "DSCF5591 - edited #002.jpg");
        assert!(first.exists());

        let decoded = image::open(&first).unwrap();
        assert_eq!(decoded.width(), 4);
        assert_eq!(decoded.height(), 3);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fitting_shrinks_and_never_enlarges() {
        let at = |edge: u32| ExportSettings {
            size: Size::LongEdge(edge),
            sharpen: false,
            ..Default::default()
        };
        let wide = RgbImage::from_pixel(400, 200, image::Rgb([1, 2, 3]));

        let small = fit(wide.clone(), &at(100));
        assert_eq!((small.width(), small.height()), (100, 50));

        let same = fit(wide.clone(), &at(4000));
        assert_eq!((same.width(), same.height()), (400, 200));
        assert_eq!(fit(wide, &ExportSettings::default()).width(), 400);

        let tall = RgbImage::from_pixel(200, 400, image::Rgb([1, 2, 3]));
        let small = fit(tall, &at(100));
        assert_eq!((small.width(), small.height()), (50, 100));
    }

    #[test]
    fn a_reduced_frame_comes_out_sharper_than_the_reduction_left_it() {

        let full = RgbImage::from_fn(400, 80, |x, _| {
            if x < 200 { image::Rgb([20, 20, 20]) } else { image::Rgb([220, 220, 220]) }
        });

        let soft = fit(full.clone(), &ExportSettings {
            size: Size::LongEdge(100),
            sharpen: false,
            ..Default::default()
        });
        let crisp = fit(full.clone(), &ExportSettings {
            size: Size::LongEdge(100),
            sharpen: true,
            ..Default::default()
        });

        let step = |image: &RgbImage| {
            image.get_pixel(51, 10)[0] as i32 - image.get_pixel(48, 10)[0] as i32
        };
        assert!(
            step(&crisp) > step(&soft),
            "sharpening did nothing: {} against {}",
            step(&crisp),
            step(&soft)
        );

        let flat = RgbImage::from_pixel(400, 80, image::Rgb([128, 128, 128]));
        let out = fit(flat, &ExportSettings {
            size: Size::LongEdge(100),
            sharpen: true,
            ..Default::default()
        });
        assert!(out.pixels().all(|p| (p[0] as i32 - 128).abs() <= 1), "flat grey was not left alone");

        let untouched = fit(full.clone(), &ExportSettings::default());
        assert_eq!(untouched, full);
    }

    #[test]
    fn a_png_is_written_as_a_png() {
        let dir = std::env::temp_dir().join("numa-export-png");
        let _ = std::fs::remove_dir_all(&dir);
        let settings = ExportSettings { format: Format::Png, ..Default::default() };
        let path = next_path(&dir, Path::new("/photos/DSCF1.RAF"), &settings).unwrap();
        assert_eq!(path.extension().unwrap(), "png");

        let image = RgbImage::from_pixel(5, 4, image::Rgb([200, 100, 50]));
        save(&image, &path, &settings, None).unwrap();

        let decoded = image::open(&path).unwrap().to_rgb8();
        assert_eq!(decoded.dimensions(), (5, 4));

        assert_eq!(decoded.get_pixel(2, 2), &image::Rgb([200, 100, 50]));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn tiny_exif() -> Vec<u8> {
        let mut exif: Vec<u8> = vec![0xFF, 0xE1, 0x00, 0x16];
        exif.extend_from_slice(b"Exif\0\0");
        exif.extend_from_slice(b"MM");
        exif.extend_from_slice(&[0x00, 0x2A]);
        exif.extend_from_slice(&8u32.to_be_bytes());
        exif.extend_from_slice(&0u16.to_be_bytes());
        exif.extend_from_slice(&0u32.to_be_bytes());
        exif
    }

    #[test]
    fn the_exif_block_is_found_wherever_it_sits() {
        let dir = std::env::temp_dir().join("numa-exif-scan");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut file: Vec<u8> = vec![0x42; 300];
        file.extend_from_slice(&[0xFF, 0xD8]);
        file.extend_from_slice(&tiny_exif());
        file.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02]);
        let path = dir.join("camera.raw");
        std::fs::write(&path, &file).unwrap();

        let found = exif_block(&path).expect("the block was not found");
        assert_eq!(&found[..2], &[0xFF, 0xE1]);
        assert_eq!(&found[4..10], b"Exif\0\0");
        assert_eq!(found.len(), tiny_exif().len());

        let mut liar: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x16];
        liar.extend_from_slice(b"Exif\0\0");
        liar.extend_from_slice(b"ZZ");
        liar.extend_from_slice(&[0u8; 32]);
        let path = dir.join("not-really.raw");
        std::fs::write(&path, &liar).unwrap();
        assert!(exif_block(&path).is_none(), "a bad TIFF header was accepted");

        let path = dir.join("bare.raw");
        std::fs::write(&path, vec![0u8; 5000]).unwrap();
        assert!(exif_block(&path).is_none());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_block_is_built_from_a_tiff_that_has_no_app1() {
        use rawler::formats::tiff::writer::TiffWriter;
        use rawler::formats::tiff::{Value, IFD};
        use std::io::Cursor;

        let dir = std::env::temp_dir().join("numa-exif-build");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut buffer = Cursor::new(Vec::new());
        let mut tiff = TiffWriter::new(&mut buffer).unwrap();
        let mut exif = tiff.new_directory();
        exif.add_untyped_tag(0x829Du16, Value::Rational(vec![rawler::formats::tiff::Rational::new(14, 10)]));
        exif.add_untyped_tag(0x8827u16, Value::Short(vec![200]));
        exif.add_untyped_tag(0x927Cu16, Value::Byte(vec![7; 64]));
        let exif_offset = exif.build(&mut tiff).unwrap();

        let mut root = tiff.new_directory();
        root.add_untyped_tag(0x010Fu16, Value::Ascii(rawler::formats::tiff::TiffAscii::new("PENTAX")));
        root.add_untyped_tag(0x0110u16, Value::Ascii(rawler::formats::tiff::TiffAscii::new("K-1")));

        root.add_untyped_tag(0x0112u16, Value::Short(vec![8]));

        root.add_untyped_tag(0x0111u16, Value::Long(vec![900_000]));
        root.add_untyped_tag(0x8769u16, Value::Long(vec![exif_offset]));
        tiff.build(root).unwrap();

        let path = dir.join("camera.tif");
        std::fs::write(&path, buffer.into_inner()).unwrap();

        let block = exif_block(&path).expect("nothing was built");
        assert_eq!(&block[..2], &[0xFF, 0xE1]);
        assert_eq!(&block[4..10], b"Exif\0\0");

        let mut reader = Cursor::new(block[10..].to_vec());
        let read = IFD::new_root_with_correction(&mut reader, 0, 0, 0, 10, &[0x8769]).unwrap();
        let text = |tag: u16| {
            read.get_entry_recursive(tag).map(|entry| format!("{:?}", entry.value))
        };
        assert!(text(0x010F).unwrap().contains("PENTAX"), "the make did not survive");
        assert!(text(0x0110).unwrap().contains("K-1"));
        assert!(text(0x8827).unwrap().contains("200"), "the Exif directory did not survive");
        assert!(text(0x829D).is_some(), "the aperture did not survive");

        assert!(text(0x0112).unwrap().contains('1'), "the orientation was not reset");
        assert!(!text(0x0112).unwrap().contains('8'));
        assert!(text(0x927C).is_none(), "the MakerNote travelled and its offsets did not");
        assert!(text(0x0111).is_none(), "a pointer to image data travelled");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn exif_is_spliced_in_after_the_start_marker() {
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xDB, 0x00, 0x02];
        let exif = vec![0xFF, 0xE1, 0x00, 0x08, b'E', b'x', b'i', b'f', 0, 0];
        let out = with_exif(jpeg.clone(), &exif);

        assert_eq!(&out[..2], &[0xFF, 0xD8], "the start marker has to stay first");
        assert_eq!(&out[2..12], &exif[..]);
        assert_eq!(&out[12..], &jpeg[2..]);

        assert_eq!(with_exif(vec![1, 2, 3], &exif), vec![1, 2, 3]);
    }

    #[test]
    fn the_copied_orientation_is_reset_and_the_thumbnail_cut_loose() {

        let mut exif: Vec<u8> = vec![0xFF, 0xE1, 0x00, 0x00];
        exif.extend_from_slice(b"Exif\0\0");
        exif.extend_from_slice(b"MM");
        exif.extend_from_slice(&[0x00, 0x2A]);
        exif.extend_from_slice(&8u32.to_be_bytes());
        exif.extend_from_slice(&1u16.to_be_bytes());
        exif.extend_from_slice(&0x0112u16.to_be_bytes());
        exif.extend_from_slice(&3u16.to_be_bytes());
        exif.extend_from_slice(&1u32.to_be_bytes());
        exif.extend_from_slice(&[0x00, 0x06, 0x00, 0x00]);
        exif.extend_from_slice(&64u32.to_be_bytes());

        let orientation = 10 + 8 + 2 + 8;
        let next = orientation + 4;
        assert_eq!(exif[orientation..orientation + 2], [0x00, 0x06]);

        tidy_exif(&mut exif);
        assert_eq!(exif[orientation..orientation + 2], [0x00, 0x01], "still rotated");
        assert_eq!(exif[next..next + 4], [0, 0, 0, 0], "the thumbnail is still pointed at");
    }

}
