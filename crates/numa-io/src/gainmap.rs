use image::codecs::jpeg::JpegEncoder;
use image::{GrayImage, ImageBuffer, ImageEncoder, Luma};

pub type Gains = ImageBuffer<Luma<f32>, Vec<f32>>;

pub fn gain_map(gains: &Gains) -> Result<(Vec<u8>, f32), String> {
    let most = gains.pixels().map(|gain| gain.0[0]).fold(1.0f32, f32::max).log2().max(0.01);
    let coded = GrayImage::from_fn(gains.width(), gains.height(), |x, y| {
        let stops = gains.get_pixel(x, y).0[0].max(1.0).log2();
        Luma([(stops / most * 255.0).round().clamp(0.0, 255.0) as u8])
    });
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 85)
        .write_image(coded.as_raw(), coded.width(), coded.height(), image::ExtendedColorType::L8)
        .map_err(|err| err.to_string())?;
    let description = format!(
        r#"<rdf:Description rdf:about="" xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/" hdrgm:Version="1.0" hdrgm:GainMapMin="0" hdrgm:GainMapMax="{most:.4}" hdrgm:Gamma="1" hdrgm:OffsetSDR="0" hdrgm:OffsetHDR="0" hdrgm:HDRCapacityMin="0" hdrgm:HDRCapacityMax="{most:.4}" hdrgm:BaseRenditionIsHDR="False"/>"#
    );
    Ok((with_segment(jpeg, &crate::xmp::segment(&description)), most))
}

pub fn primary_description(gain_map_length: usize) -> (String, String) {
    let attributes = r#" xmlns:Container="http://ns.google.com/photos/1.0/container/" xmlns:Item="http://ns.google.com/photos/1.0/container/item/" xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/" hdrgm:Version="1.0""#;
    let elements = format!(
        r#"<Container:Directory><rdf:Seq><rdf:li rdf:parseType="Resource"><Container:Item Item:Semantic="Primary" Item:Mime="image/jpeg"/></rdf:li><rdf:li rdf:parseType="Resource"><Container:Item Item:Semantic="GainMap" Item:Mime="image/jpeg" Item:Length="{gain_map_length}"/></rdf:li></rdf:Seq></Container:Directory>"#
    );
    (attributes.to_string(), elements)
}

pub fn attach(primary: Vec<u8>, gain_map: &[u8]) -> Vec<u8> {
    const SEGMENT: usize = 90;

    let at = after_app_segments(&primary);
    let header = at + 8;
    let total = primary.len() + SEGMENT;
    let mut mpf = Vec::with_capacity(SEGMENT);
    mpf.extend_from_slice(&[0xFF, 0xE2]);
    mpf.extend_from_slice(&((SEGMENT - 2) as u16).to_be_bytes());
    mpf.extend_from_slice(b"MPF\0");
    mpf.extend_from_slice(b"II*\0");
    mpf.extend_from_slice(&8u32.to_le_bytes());
    mpf.extend_from_slice(&3u16.to_le_bytes());

    for (tag, kind, count, value) in [(0xB000u16, 7u16, 4u32, u32::from_le_bytes(*b"0100")), (0xB001, 4, 1, 2), (0xB002, 7, 32, 50)] {
        mpf.extend_from_slice(&tag.to_le_bytes());
        mpf.extend_from_slice(&kind.to_le_bytes());
        mpf.extend_from_slice(&count.to_le_bytes());
        mpf.extend_from_slice(&value.to_le_bytes());
    }
    mpf.extend_from_slice(&0u32.to_le_bytes());

    for (attribute, size, offset) in [(0x0003_0000u32, total, 0usize), (0, gain_map.len(), total - header)] {
        mpf.extend_from_slice(&attribute.to_le_bytes());
        mpf.extend_from_slice(&(size as u32).to_le_bytes());
        mpf.extend_from_slice(&(offset as u32).to_le_bytes());
        mpf.extend_from_slice(&[0; 4]);
    }
    debug_assert_eq!(mpf.len(), SEGMENT);

    let mut out = Vec::with_capacity(total + gain_map.len());
    out.extend_from_slice(&primary[..at]);
    out.extend_from_slice(&mpf);
    out.extend_from_slice(&primary[at..]);
    out.extend_from_slice(gain_map);
    out
}

pub fn with_segment(jpeg: Vec<u8>, segment: &[u8]) -> Vec<u8> {
    let at = after_app_segments(&jpeg);
    [&jpeg[..at], segment, &jpeg[at..]].concat()
}

fn after_app_segments(jpeg: &[u8]) -> usize {
    let mut at = 2;
    while at + 4 <= jpeg.len() && jpeg[at] == 0xFF && (0xE0..=0xEF).contains(&jpeg[at + 1]) {
        at += 2 + u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]) as usize;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_index_finds_the_gain_map() {
        let gains = Gains::from_fn(40, 30, |x, _| Luma([1.0 + x as f32 / 10.0]));
        let (map, most) = gain_map(&gains).unwrap();
        assert!((most - 4.9f32.log2()).abs() < 1e-3);
        let primary = with_segment(vec![0xFF, 0xD8, 0xFF, 0xDB, 0, 2, 0xFF, 0xD9], &crate::xmp::segment("<rdf:Description/>"));
        let file = attach(primary.clone(), &map);

        let mpf = file.windows(4).position(|window| window == b"MPF\0").unwrap();
        let header = mpf + 4;
        let entry = |index: usize, field: usize| {
            let at = header + 50 + index * 16 + field * 4;
            u32::from_le_bytes(file[at..at + 4].try_into().unwrap()) as usize
        };
        assert_eq!(entry(0, 1), file.len() - map.len(), "the primary's size");
        assert_eq!(&file[header + entry(1, 2)..], &map[..], "the gain map where the index says");
        assert_eq!(entry(1, 1), map.len());
        assert!(String::from_utf8_lossy(&map).contains("hdrgm:GainMapMax=\"2.2928\""));
    }
}
