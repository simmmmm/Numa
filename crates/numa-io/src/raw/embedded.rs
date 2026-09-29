use image::DynamicImage;
use rawler::formats::tiff::reader::TiffReader;
use rawler::formats::tiff::{GenericTiffReader, IFD};
use rawler::rawsource::RawSource;
use std::path::Path;

pub(super) fn at_least(path: &Path, edge: u32) -> Option<DynamicImage> {
    std::panic::catch_unwind(|| {
        let source = RawSource::new(path).ok()?;
        let jpeg = smallest(&source, edge)?;
        let image = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg).ok()?;
        let decoder = rawler::get_decoder(&source).ok()?;
        let orientation = decoder
            .raw_metadata(&source, &Default::default())
            .ok()
            .and_then(|meta| meta.exif.orientation)
            .map_or(rawler::Orientation::Normal, rawler::Orientation::from_u16);
        Some(super::orient_image(image, orientation))
    })
    .ok()
    .flatten()
}

fn smallest(source: &RawSource, edge: u32) -> Option<&[u8]> {
    let buf = source.buf();
    if buf.get(4..12) == Some(b"ftypcrx ") {
        return prvw(buf).filter(|jpeg| baseline_size(jpeg).is_some_and(|(width, height)| width.max(height) >= edge));
    }
    let mut found: Vec<((u32, u32), &[u8])> = in_tiff(source)
        .into_iter()
        .filter_map(|jpeg| Some((baseline_size(jpeg)?, jpeg)))
        .filter(|((width, height), _)| (*width).max(*height) >= edge)
        .collect();
    found.sort_by_key(|((width, height), _)| width * height);

    let ((width, height), _) = *found.last().filter(|_| found.len() >= 2)?;
    let aspect = |width: u32, height: u32| width as f32 / height as f32;
    found
        .into_iter()
        .find(|((w, h), _)| (aspect(*w, *h) / aspect(width, height) - 1.0).abs() < 0.01)
        .map(|(_, jpeg)| jpeg)
}

fn prvw(buf: &[u8]) -> Option<&[u8]> {
    let at = buf[..buf.len().min(1 << 20)].windows(4).position(|w| w == b"PRVW")?;
    let length = u32::from_be_bytes(buf.get(at + 16..at + 20)?.try_into().ok()?) as usize;
    buf.get(at + 20..(at + 20).checked_add(length)?)
}

fn in_tiff(source: &RawSource) -> Vec<&[u8]> {
    let Ok(tiff) = GenericTiffReader::new(&mut source.reader(), 0, 0, None, &[]) else { return Vec::new() };
    let value = |ifd: &IFD, tag: u16| ifd.entries().get(&tag).and_then(|entry| entry.value.get_usize(0).ok().flatten());
    tiff.find_ifds_with_filter(|_| true)
        .into_iter()
        .filter_map(|ifd| {
            let (start, length) = match (value(ifd, 0x201), value(ifd, 0x202)) {
                (Some(start), Some(length)) => (start, length),
                _ if value(ifd, 0xFE) == Some(1) && matches!(value(ifd, 0x103), Some(6 | 7)) => {
                    (value(ifd, 0x111)?, value(ifd, 0x117)?)
                }
                _ => return None,
            };
            source.subview((ifd.base as usize + start) as u64, length as u64).ok()
        })
        .collect()
}

fn baseline_size(jpeg: &[u8]) -> Option<(u32, u32)> {
    if jpeg.get(..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut at = 2;
    loop {
        let (marker, length) = (*jpeg.get(at + 1)?, u16::from_be_bytes(jpeg.get(at + 2..at + 4)?.try_into().ok()?) as usize);
        if jpeg[at] != 0xFF {
            return None;
        }
        if matches!(marker, 0xC0..=0xC2) {
            let height = u16::from_be_bytes(jpeg.get(at + 5..at + 7)?.try_into().ok()?) as u32;
            let width = u16::from_be_bytes(jpeg.get(at + 7..at + 9)?.try_into().ok()?) as u32;
            return (width > 0 && height > 0).then_some((width, height));
        }
        if marker == 0xC3 || marker == 0xDA {
            return None;
        }
        at += 2 + length;
    }
}
