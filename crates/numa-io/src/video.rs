use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const EXTENSIONS: &[&str] = &["mp4", "m4v", "mov", "3gp", "webm", "mkv", "avi", "mts"];

fn extension(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    if name.starts_with("._") {
        return None;
    }
    Some(path.extension()?.to_str()?.to_ascii_lowercase())
}

pub fn is_video(path: &Path) -> bool {
    extension(path).is_some_and(|ext| EXTENSIONS.contains(&ext.as_str()))
}

pub fn content_type(path: &Path) -> &'static str {
    match extension(path).as_deref() {
        Some("mp4" | "m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("3gp") => "video/3gpp",
        Some("webm") => "video/webm",
        Some("mkv") => "video/x-matroska",
        Some("avi") => "video/x-msvideo",
        Some("mts") => "video/mp2t",
        _ => "application/octet-stream",
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Info {
    pub seconds: Option<f32>,

    pub created: Option<i64>,

    pub size: Option<(u32, u32)>,
}

const FROM_1904: i64 = 2_082_844_800;

const MOOV_MAX: u64 = 64 << 20;

const TOP_BOXES: usize = 4096;

type Kept = Mutex<HashMap<(PathBuf, i64), Option<Info>>>;
static KEPT: std::sync::OnceLock<Kept> = std::sync::OnceLock::new();

pub fn probe(path: &Path) -> Option<Info> {
    let mtime = std::fs::metadata(path).and_then(|meta| meta.modified()).ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
    let kept = KEPT.get_or_init(Default::default);
    let key = (path.to_path_buf(), mtime);
    if let Some(known) = kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&key) {
        return *known;
    }

    let read = std::panic::catch_unwind(|| std::fs::File::open(path).ok().and_then(|mut file| moov(&mut file)).and_then(|moov| info(&moov))).ok().flatten();
    kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(key, read);
    read
}

fn moov(file: &mut (impl Read + Seek)) -> Option<Vec<u8>> {
    let end = file.seek(SeekFrom::End(0)).ok()?;
    let mut at = 0u64;
    for _ in 0..TOP_BOXES {
        if at.checked_add(8).is_none_or(|next| next > end) {
            break;
        }
        file.seek(SeekFrom::Start(at)).ok()?;
        let mut head = [0u8; 8];
        file.read_exact(&mut head).ok()?;
        let (mut size, kind) = (u32::from_be_bytes(head[..4].try_into().ok()?) as u64, &head[4..]);
        let mut header = 8;
        if size == 1 {
            let mut long = [0u8; 8];
            file.read_exact(&mut long).ok()?;
            (size, header) = (u64::from_be_bytes(long), 16);
        } else if size == 0 {
            size = end - at;
        }
        if size < header {
            return None;
        }
        if kind == b"moov" {
            let len = size - header;
            if len > MOOV_MAX || at.checked_add(size).is_none_or(|next| next > end) {
                return None;
            }
            let mut content = vec![0u8; len as usize];
            file.read_exact(&mut content).ok()?;
            return Some(content);
        }
        at = at.checked_add(size)?;
    }
    None
}

fn boxes(data: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut found = Vec::new();
    let mut at = 0usize;
    while at + 8 <= data.len() {
        let size = u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as usize;
        let size = if size == 0 { data.len() - at } else { size };
        if size < 8 || at + size > data.len() {
            break;
        }
        found.push((&data[at + 4..at + 8], &data[at + 8..at + size]));
        at += size;
    }
    found
}

fn be32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn be64(data: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(data.get(at..at + 8)?.try_into().ok()?))
}

fn info(moov: &[u8]) -> Option<Info> {
    let mut info = Info::default();
    for (kind, content) in boxes(moov) {
        match kind {
            b"mvhd" => {
                let wide = *content.first()? == 1;
                let (created, scale, length) = match wide {
                    true => (be64(content, 4)?, be32(content, 20)?, be64(content, 24)?),
                    false => (be32(content, 4)? as u64, be32(content, 12)?, be32(content, 16)? as u64),
                };
                info.created = (created as i64 > FROM_1904).then(|| created as i64 - FROM_1904);

                info.seconds = (scale > 0 && length != u32::MAX as u64 && length != u64::MAX).then(|| length as f32 / scale as f32);
            }
            b"trak" if info.size.is_none() => info.size = boxes(content).into_iter().find(|(kind, _)| *kind == b"tkhd").and_then(|(_, tkhd)| shape(tkhd)),
            _ => {}
        }
    }
    (info != Info::default()).then_some(info)
}

fn shape(tkhd: &[u8]) -> Option<(u32, u32)> {
    let (matrix, size) = match *tkhd.first()? {
        1 => (48, 88),
        _ => (40, 76),
    };
    let (width, height) = (be32(tkhd, size)? >> 16, be32(tkhd, size + 4)? >> 16);
    if width == 0 || height == 0 {
        return None;
    }
    let (a, b) = (be32(tkhd, matrix)? as i32, be32(tkhd, matrix + 4)? as i32);
    Some(if a == 0 && b != 0 { (height, width) } else { (width, height) })
}

pub fn stamp(path: &Path) -> crate::exif::Stamp {
    crate::exif::Stamp { taken: probe(path).and_then(|info| info.created), camera: String::new() }
}

pub fn holds<'a>(files: impl IntoIterator<Item = (i64, &'a Path)>, is_still: impl Fn(&Path) -> bool) -> HashMap<i64, i64> {
    let key = |path: &Path| (path.parent().map(Path::to_path_buf), path.file_stem().map(|stem| stem.to_string_lossy().to_uppercase()));
    let mut stills: HashMap<_, i64> = HashMap::new();
    let mut videos = Vec::new();
    for (id, path) in files {
        match is_video(path) {
            true => videos.push((id, key(path))),
            false if is_still(path) => {
                let first = stills.entry(key(path)).or_insert(id);
                *first = (*first).min(id);
            }
            false => {}
        }
    }
    videos.into_iter().filter_map(|(video, key)| Some((*stills.get(&key)?, video))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(kind: &[u8], content: &[u8]) -> Vec<u8> {
        let mut out = ((content.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(content);
        out
    }

    fn mvhd(created: u32, scale: u32, length: u32) -> Vec<u8> {
        let mut content = vec![0u8; 100];
        content[4..8].copy_from_slice(&created.to_be_bytes());
        content[12..16].copy_from_slice(&scale.to_be_bytes());
        content[16..20].copy_from_slice(&length.to_be_bytes());
        boxed(b"mvhd", &content)
    }

    fn tkhd(width: u32, height: u32, turned: bool) -> Vec<u8> {
        let mut content = vec![0u8; 84];
        let (a, b) = if turned { (0u32, 0x0001_0000u32) } else { (0x0001_0000, 0) };
        content[40..44].copy_from_slice(&a.to_be_bytes());
        content[44..48].copy_from_slice(&b.to_be_bytes());
        content[76..80].copy_from_slice(&(width << 16).to_be_bytes());
        content[80..84].copy_from_slice(&(height << 16).to_be_bytes());
        boxed(b"tkhd", &content)
    }

    fn file(moov: &[u8], after_mdat: bool) -> Vec<u8> {
        let mut out = boxed(b"ftyp", b"isom\0\0\0\0isom");
        if after_mdat {
            out.extend(boxed(b"mdat", &[7u8; 4096]));
        }
        out.extend(boxed(b"moov", moov));
        out
    }

    #[test]
    fn names_are_video_except_apples_sidecars() {
        assert!(is_video(Path::new("a/IMG_1.MOV")) && is_video(Path::new("clip.Mp4")) && is_video(Path::new("c.webm")));
        assert!(!is_video(Path::new("a.jpg")) && !is_video(Path::new("._IMG_1.MOV")) && !is_video(Path::new("clip")));
        assert_eq!(content_type(Path::new("a.mov")), "video/quicktime");
    }

    #[test]
    fn a_files_own_length_date_and_shape_are_read_wherever_its_index_is() {
        let created = (FROM_1904 + 1_700_000_000) as u32;
        let index = [mvhd(created, 600, 6000), boxed(b"trak", &tkhd(0, 0, false)), boxed(b"trak", &tkhd(1920, 1080, true))].concat();
        for after in [false, true] {
            let read = info(&moov(&mut std::io::Cursor::new(file(&index, after))).unwrap()).unwrap();
            assert_eq!(read.seconds, Some(10.0));
            assert_eq!(read.created, Some(1_700_000_000));

            assert_eq!(read.size, Some((1080, 1920)));
        }
    }

    #[test]
    fn a_damaged_or_foreign_file_reads_as_nothing() {
        assert!(moov(&mut std::io::Cursor::new(b"not a video at all".to_vec())).is_none());
        assert!(moov(&mut std::io::Cursor::new(Vec::new())).is_none());
        let mut cut = file(&mvhd(0, 1, 1), true);
        cut.truncate(cut.len() - 10);
        assert!(moov(&mut std::io::Cursor::new(cut)).is_none());

        assert!(info(&[0, 0, 0, 4, b'm', b'v', b'h', b'd']).is_none());
        assert!(info(&[0, 0, 255, 255, b'm', b'v', b'h', b'd', 0]).is_none());

        let unset = info(&mvhd(0, 600, u32::MAX)).unwrap_or_default();
        assert_eq!((unset.created, unset.seconds), (None, None));
    }

    #[test]
    fn a_video_beside_a_still_of_the_same_name_is_its_hold() {
        let files = [
            (1, "/p/IMG_0412.HEIC"),
            (2, "/p/IMG_0412.MOV"),
            (3, "/p/IMG_0413.HEIC"),
            (4, "/p/IMG_0414.MOV"),
            (5, "/q/IMG_0413.mov"),
            (6, "/p/img_0413.mov"),
        ];
        let held = holds(files.iter().map(|(id, path)| (*id, Path::new(path))), |path| !is_video(path));
        assert_eq!(held.get(&1), Some(&2));

        assert_eq!(held.get(&3), Some(&6));
        assert_eq!(held.len(), 2);
        assert!(!held.values().any(|video| *video == 4 || *video == 5));
    }
}
