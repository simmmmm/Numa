use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const HEAD: u64 = 256 << 10;

pub(super) fn maker_note_jpeg(path: &Path) -> Option<Vec<u8>> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = Vec::new();
    (&mut file).take(HEAD).read_to_end(&mut head).ok()?;

    if head.get(..2)? != b"II" {
        return None;
    }
    let u16_at = |at: usize| Some(u16::from_le_bytes(head.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(head.get(at..at + 4)?.try_into().ok()?));

    let find = |at: usize, tag: u16| {
        (0..u16_at(at)? as usize).map(|k| at + 2 + k * 12).find(|&entry| u16_at(entry) == Some(tag)).and_then(|entry| u32_at(entry + 8))
    };
    let exif = find(u32_at(4)? as usize, 0x8769)? as usize;
    let maker = find(exif, 0x927c)? as usize;

    if head.get(maker..maker + 8)? != b"OLYMPUS\0" {
        return None;
    }
    let settings = maker + find(maker + 12, 0x2020)? as usize;
    let start = maker as u64 + u64::from(find(settings, 0x0101)?);
    let length = u64::from(find(settings, 0x0102)?);

    let mut jpeg = Vec::new();
    file.seek(SeekFrom::Start(start)).ok()?;
    file.take(length).read_to_end(&mut jpeg).ok()?;
    (jpeg.len() as u64 == length && jpeg.starts_with(&[0xFF, 0xD8])).then_some(jpeg)
}
