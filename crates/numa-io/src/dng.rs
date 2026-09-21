use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, RgbImage};
use rawler::decoders::{Orientation, RawDecodeParams, WellKnownIFD};
use rawler::dng::writer::DngWriter;
use rawler::dng::{CropMode, DngCompression, DngPhotometricConversion, DNG_VERSION_V1_4};
use rawler::rawsource::RawSource;

pub fn write(source: &Path, path: &Path, preview: &RgbImage, xmp: &str) -> Result<(), String> {
    let fail = |err: String| format!("{}: {}", path.display(), err);
    if !crate::raw::is_raw(source) {
        return Err(fail("a DNG is made from a raw file".to_string()));
    }
    let rawfile = RawSource::new(source).map_err(|err| fail(err.to_string()))?;
    let decoder = rawler::get_decoder(&rawfile).map_err(say(path))?;
    let params = RawDecodeParams { image_index: 0 };
    let rawimage = decoder.raw_image(&rawfile, &params, false).map_err(say(path))?;

    if rawimage.camera.find_hint("fuji_rotation") || rawimage.camera.find_hint("fuji_rotation_alt") {
        return Err(fail("this camera's sensor layout cannot go into a DNG".to_string()));
    }
    let metadata = decoder.raw_metadata(&rawfile, &params).map_err(say(path))?;

    let mut out = Cursor::new(Vec::new());
    let mut dng = DngWriter::new(&mut out, DNG_VERSION_V1_4).map_err(say(path))?;
    let mut raw = dng.subframe(0);
    raw.raw_image(&rawimage, CropMode::Best, DngCompression::Lossless, DngPhotometricConversion::Original, 1)
        .map_err(say(path))?;
    if let Some(tags) = decoder.ifd(WellKnownIFD::VirtualDngRawTags).map_err(say(path))? {
        raw.ifd_mut().copy(tags.value_iter());
    }
    raw.finalize().map_err(say(path))?;

    let preview = DynamicImage::ImageRgb8(as_the_sensor_lay(preview, rawimage.orientation));
    let mut frame = dng.subframe(1);
    frame.preview(&preview, 0.85).map_err(say(path))?;
    frame.finalize().map_err(say(path))?;
    dng.thumbnail(&preview).map_err(say(path))?;

    dng.load_base_tags(&rawimage).map_err(say(path))?;
    dng.load_metadata(&metadata).map_err(say(path))?;
    dng.root_ifd_mut().add_tag(rawler::tags::ExifTag::Orientation, rawimage.orientation.to_u16());
    if let Some(tags) = decoder.ifd(WellKnownIFD::VirtualDngRootTags).map_err(say(path))? {
        dng.root_ifd_mut().copy(tags.value_iter());
    }
    dng.xpacket(xmp.as_bytes()).map_err(say(path))?;
    dng.root_ifd_mut().add_tag(rawler::tags::TiffCommonTag::Software, "Numa");
    dng.close().map_err(say(path))?;

    std::fs::write(path, out.into_inner()).map_err(|err| fail(err.to_string()))
}

fn say<E: std::fmt::Display>(path: &Path) -> impl Fn(E) -> String + '_ {
    move |err| format!("{}: {err}", path.display())
}

fn as_the_sensor_lay(image: &RgbImage, orientation: Orientation) -> RgbImage {
    use image::imageops::{flip_horizontal, flip_vertical, rotate180, rotate270, rotate90};
    match orientation {
        Orientation::HorizontalFlip => flip_horizontal(image),
        Orientation::Rotate180 => rotate180(image),
        Orientation::VerticalFlip => flip_vertical(image),

        Orientation::Transpose => rotate270(&flip_horizontal(image)),
        Orientation::Rotate90 => rotate270(image),
        Orientation::Transverse => rotate90(&flip_horizontal(image)),
        Orientation::Rotate270 => rotate90(image),
        Orientation::Normal | Orientation::Unknown => image.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preview_is_turned_back_to_the_sensor() {
        let upright = RgbImage::from_fn(3, 2, |x, y| image::Rgb([x as u8, y as u8, 0]));
        let laid = as_the_sensor_lay(&upright, Orientation::Rotate90);
        assert_eq!(laid.dimensions(), (2, 3));
        assert_eq!(image::imageops::rotate90(&laid), upright);
        assert_eq!(image::imageops::rotate270(&as_the_sensor_lay(&upright, Orientation::Rotate270)), upright);
        let transposed = as_the_sensor_lay(&upright, Orientation::Transpose);
        assert_eq!(transposed.get_pixel(1, 2), upright.get_pixel(2, 1), "a mirror across the diagonal");
    }
}
