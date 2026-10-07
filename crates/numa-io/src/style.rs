use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::raw;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Style {

    Neutral,

    Other(String),

    Unknown,
}

pub fn of(path: &Path) -> Style {
    let extension = path.extension().map(|ext| ext.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let style = match extension.as_str() {
        "raf" => return fujifilm(path),
        "arw" | "sr2" | "srf" => sony(path),
        "cr2" => canon(path, false),
        "cr3" => canon(path, true),
        "nef" | "nrw" => nikon(path),
        _ => None,
    };
    style.unwrap_or(Style::Unknown)
}

pub fn setting(make: &str) -> Option<&'static str> {
    match make.to_lowercase().split_whitespace().next()? {
        "fujifilm" => Some("Provia, with Color at 0"),
        "sony" => Some("Creative Style or Creative Look Standard, with saturation at 0"),
        "canon" => Some("Picture Style Standard (not Auto), with saturation and colour tone at 0"),
        "nikon" => Some("Picture Control Standard, with saturation at 0"),
        _ => None,
    }
}

fn fujifilm(path: &Path) -> Style {
    match (raw::film_mode(path), raw::colour_setting(path)) {
        (Some("Provia"), Some(0)) => Style::Neutral,
        (Some("Provia"), _) => Style::Other("Provia with Color not at 0".into()),
        (Some(film), _) => Style::Other(film.into()),
        (None, _) => Style::Unknown,
    }
}

fn sony(path: &Path) -> Option<Style> {
    let (mut tiff, exif) = exif(path)?;
    let note = tiff.u32(&exif.get(&0x927c)?.value);

    let skip = if tiff.bytes(note, 4)? == b"SONY" { 12 } else { 0 };
    let dir = tiff.ifd(note + skip)?;
    let style = text(&tiff.value(dir.get(&0xb020)?)?);

    let saturation = match (dir.get(&0x2005), exif.get(&0xa409)) {
        (Some(set), _) => tiff.u32(&set.value) as i32,
        (None, Some(exif)) => tiff.u16(&exif.value) as i32,
        (None, None) => 0,
    };
    Some(match style.as_str() {
        "" => return None,
        "Standard" if saturation == 0 => Style::Neutral,
        "Standard" => Style::Other("Standard with saturation not at 0".into()),
        _ => Style::Other(style),
    })
}

fn canon(path: &Path, cr3: bool) -> Option<Style> {
    let (mut tiff, dir) = if cr3 {
        let mut file = File::open(path).ok()?;
        let note = cmt3(&mut file)?;
        let (mut tiff, first) = Tiff::open(file, note)?;
        let dir = tiff.ifd(first)?;
        (tiff, dir)
    } else {
        let (mut tiff, exif) = exif(path)?;
        let note = tiff.u32(&exif.get(&0x927c)?.value);
        let dir = tiff.ifd(note)?;
        (tiff, dir)
    };
    let mut shorts = |tag: u16| -> Option<Vec<i16>> {
        let bytes = tiff.value(dir.get(&tag)?)?;
        Some(bytes.chunks_exact(2).map(|pair| tiff.u16(pair) as i16).collect())
    };

    let code = *shorts(0x00a0)?.get(10)? as u16;
    let settings = shorts(0x0001).unwrap_or_default();
    let adjusted = |at: usize| settings.get(at).is_some_and(|&value| value != 0 && value != 0x7fff);
    let name = match code {
        0x00 => "None",
        0x01 | 0x81 => "Standard",
        0x02 | 0x82 => "Portrait",
        0x03 => "High Saturation",
        0x04 => "Adobe RGB",
        0x05 => "Low Saturation",
        0x06 => "CM Set 1",
        0x07 => "CM Set 2",
        0x21 => "User Def. 1",
        0x22 => "User Def. 2",
        0x23 => "User Def. 3",
        0x41 => "PC 1",
        0x42 => "PC 2",
        0x43 => "PC 3",
        0x83 => "Landscape",
        0x84 => "Neutral",
        0x85 => "Faithful",
        0x86 => "Monochrome",
        0x87 => "Auto",
        0x88 => "Fine Detail",
        0xff | 0xffff => return None,
        other => return Some(Style::Other(format!("Picture Style {other:#x}"))),
    };
    Some(match name {
        "Standard" if !adjusted(14) && !adjusted(42) => Style::Neutral,
        "Standard" => Style::Other("Standard with saturation or colour tone not at 0".into()),
        _ => Style::Other(name.into()),
    })
}

fn cmt3(file: &mut File) -> Option<u64> {
    const CANON: [u8; 16] = [0x85, 0xc0, 0xb6, 0x87, 0x82, 0x0f, 0x11, 0xe0, 0x81, 0x11, 0xf4, 0xce, 0x46, 0x2b, 0x6a, 0x48];
    let mut at = 0;

    for _ in 0..32 {
        let mut header = [0; 24];
        file.seek(SeekFrom::Start(at)).ok()?;
        file.read_exact(&mut header).ok()?;
        let size = u32::from_be_bytes(header[..4].try_into().ok()?) as u64;
        match &header[4..8] {
            b"moov" => at += 8,
            b"uuid" if header[8..] == CANON => at += 24,
            b"CMT3" => return Some(at + 8),
            _ if size < 8 => return None,
            _ => at += size,
        }
    }
    None
}

fn nikon(path: &Path) -> Option<Style> {
    let (mut tiff, exif) = exif(path)?;
    let note = tiff.u32(&exif.get(&0x927c)?.value);

    if tiff.bytes(note, 7)? != b"Nikon\0\x02" {
        return None;
    }
    let base = tiff.base + note as u64 + 10;
    let (mut tiff, first) = Tiff::open(tiff.file, base)?;
    let dir = tiff.ifd(first)?;
    picture_control(&tiff.value(dir.get(&0x0023).or(dir.get(&0x00bd))?)?)
}

fn picture_control(data: &[u8]) -> Option<Style> {
    let (name, quick, saturation) = match data.get(..2)? {
        b"01" => (4, 49, 53),
        b"02" => (4, 49, 59),
        b"03" => (8, 55, 67),
        _ => return None,
    };
    let name = text(data.get(name..name + 20)?);

    let quick_at_zero = matches!(data.get(quick), Some(0x80 | 0xff));
    Some(match name.as_str() {
        "" => return None,
        "STANDARD" if quick_at_zero && data.get(saturation) == Some(&0x80) => Style::Neutral,
        "STANDARD" => Style::Other("Standard with saturation not at 0".into()),
        _ => Style::Other(title_case(&name)),
    })
}

fn title_case(name: &str) -> String {
    name.split_inclusive(|c: char| !c.is_ascii_alphabetic())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) if word.contains(['A', 'E', 'I', 'O', 'U', 'Y']) => first.to_string() + &chars.as_str().to_ascii_lowercase(),
                _ => word.to_string(),
            }
        })
        .collect()
}

fn text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

fn exif(path: &Path) -> Option<(Tiff, HashMap<u16, Entry>)> {
    let (mut tiff, first) = Tiff::open(File::open(path).ok()?, 0)?;
    let ifd0 = tiff.ifd(first)?;
    let exif = tiff.ifd(tiff.u32(&ifd0.get(&0x8769)?.value))?;
    Some((tiff, exif))
}

struct Tiff {
    file: File,

    base: u64,
    little: bool,
}

struct Entry {
    kind: u16,
    count: u32,
    value: [u8; 4],
}

impl Tiff {

    fn open(file: File, base: u64) -> Option<(Tiff, u32)> {
        let mut tiff = Tiff { file, base, little: true };
        let header = tiff.bytes(0, 8)?;
        tiff.little = match &header[..4] {
            b"II*\0" => true,
            b"MM\0*" => false,
            _ => return None,
        };
        let first = tiff.u32(&header[4..]);
        Some((tiff, first))
    }

    fn bytes(&mut self, at: u32, len: usize) -> Option<Vec<u8>> {
        let mut out = vec![0; len];
        self.file.seek(SeekFrom::Start(self.base + at as u64)).ok()?;
        self.file.read_exact(&mut out).ok()?;
        Some(out)
    }

    fn u16(&self, bytes: &[u8]) -> u16 {
        let bytes = [bytes[0], bytes[1]];
        if self.little { u16::from_le_bytes(bytes) } else { u16::from_be_bytes(bytes) }
    }

    fn u32(&self, bytes: &[u8]) -> u32 {
        let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if self.little { u32::from_le_bytes(bytes) } else { u32::from_be_bytes(bytes) }
    }

    fn ifd(&mut self, at: u32) -> Option<HashMap<u16, Entry>> {
        let count = self.bytes(at, 2)?;
        let count = self.u16(&count) as usize;

        if count == 0 || count > 1000 {
            return None;
        }
        let entries = self.bytes(at + 2, count * 12)?;
        Some(
            entries
                .chunks_exact(12)
                .map(|entry| {
                    let value = [entry[8], entry[9], entry[10], entry[11]];
                    (self.u16(entry), Entry { kind: self.u16(&entry[2..]), count: self.u32(&entry[4..]), value })
                })
                .collect(),
        )
    }

    fn value(&mut self, entry: &Entry) -> Option<Vec<u8>> {
        let width = match entry.kind {
            3 | 8 => 2,
            4 | 9 | 11 | 13 => 4,
            5 | 10 | 12 => 8,
            _ => 1,
        };
        let len = (entry.count as usize).checked_mul(width)?;
        if len <= 4 {
            return Some(entry.value[..len].to_vec());
        }

        if len > 1 << 16 {
            return None;
        }
        self.bytes(self.u32(&entry.value), len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nikon_picture_control_versions() {

        let control = |version: &[u8; 4], name: &str, quick: u8, saturation: u8| {
            let mut data = vec![0x80; 70];
            data[..4].copy_from_slice(version);
            let (at, quick_at, saturation_at) = match &version[..2] {
                b"01" => (4, 49, 53),
                b"02" => (4, 49, 59),
                _ => (8, 55, 67),
            };
            data[at..at + 20].fill(0);
            data[at..at + name.len()].copy_from_slice(name.as_bytes());
            data[quick_at] = quick;
            data[saturation_at] = saturation;
            picture_control(&data)
        };
        assert_eq!(control(b"0100", "STANDARD", 0x80, 0x80), Some(Style::Neutral));
        assert_eq!(control(b"0200", "STANDARD", 0x80, 0x80), Some(Style::Neutral));
        assert_eq!(control(b"0310", "STANDARD", 0xff, 0x80), Some(Style::Neutral));
        assert!(matches!(control(b"0300", "STANDARD", 0xff, 0x8c), Some(Style::Other(_))));
        assert!(matches!(control(b"0100", "STANDARD", 0x81, 0x80), Some(Style::Other(_))));
        assert_eq!(control(b"0300", "VIVID-KR", 0xff, 0x80), Some(Style::Other("Vivid-KR".into())));
        assert_eq!(control(b"0200", "AUTO", 0x80, 0x80), Some(Style::Other("Auto".into())));
        assert_eq!(control(b"0900", "STANDARD", 0x80, 0x80), None);
    }

    #[test]
    #[ignore]
    fn against_exiftool() {
        use std::path::PathBuf;
        let dir = std::env::var("NUMA_STYLE_DIR").expect("NUMA_STYLE_DIR: a folder of raws");
        let mut args = vec!["-r", "-q", "-q", "-m", "-T"];
        for ext in ["arw", "sr2", "srf", "cr2", "cr3", "nef", "nrw"] {
            args.extend(["-ext", ext]);
        }
        args.extend([
            "-Directory",
            "-FileName",
            "-CreativeStyle",
            "-Saturation",
            "-PictureStyle",
            "-Canon:Saturation",
            "-Canon:ColorTone",
            "-SaturationStandard",
            "-ColorToneStandard",
            "-PictureControlName",
            "-Nikon:Saturation",
            "-PictureControlQuickAdjust",
            &dir,
        ]);
        let out = std::process::Command::new("exiftool").args(&args).output().expect("exiftool");
        let zero = |v: &str| matches!(v, "-" | "0" | "+0" | "0.00" | "Normal" | "None");
        let mut rows: Vec<(PathBuf, &str, Style)> = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let c: Vec<&str> = line.split('\t').collect();
            if c.len() < 12 {
                continue;
            }
            let path = Path::new(c[0]).join(c[1]);
            let ext = path.extension().unwrap().to_string_lossy().to_ascii_lowercase();
            let (make, truth) = match ext.as_str() {
                "arw" | "sr2" | "srf" => ("Sony", match c[2] {
                    "-" => Style::Unknown,
                    "Standard" if zero(c[3]) => Style::Neutral,
                    style => Style::Other(style.into()),
                }),
                "cr2" | "cr3" => ("Canon", match c[4] {
                    "-" | "n/a" => Style::Unknown,
                    style if style.starts_with("Unknown (0xffff") => Style::Unknown,
                    "Standard" if c[5..9].iter().all(|v| zero(v)) => Style::Neutral,
                    style => Style::Other(style.into()),
                }),
                _ => ("Nikon", match c[9] {
                    "-" => Style::Unknown,
                    "Standard" if zero(c[10]) && (zero(c[11]) || c[11] == "n/a") => Style::Neutral,
                    style => Style::Other(style.into()),
                }),
            };
            rows.push((path, make, truth));
        }

        let rchar = || {
            let io = std::fs::read_to_string("/proc/self/io").unwrap_or_default();
            io.lines().find_map(|l| l.strip_prefix("rchar: ")?.parse::<u64>().ok()).unwrap_or(0)
        };
        let (bytes, start) = (rchar(), std::time::Instant::now());
        let ours: Vec<Style> = rows.iter().map(|(path, ..)| of(path)).collect();
        let (bytes, took) = (rchar() - bytes, start.elapsed());
        let files = rows.len().max(1) as f64;
        println!("{} files: {:.0} bytes and {:.0} µs a file (warm cache)", rows.len(), bytes as f64 / files, took.as_micros() as f64 / files);

        let kind = |s: &Style| match s {
            Style::Neutral => 0,
            Style::Other(_) => 1,
            Style::Unknown => 2,
        };
        let squash = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        for make in ["Sony", "Canon", "Nikon"] {
            let mut confusion = [[0; 3]; 3];
            let (mut agree, mut total, mut names) = (0, 0, 0);
            for ((path, m, truth), ours) in rows.iter().zip(&ours) {
                if *m != make {
                    continue;
                }
                total += 1;
                confusion[kind(truth)][kind(ours)] += 1;
                if kind(truth) != kind(ours) {
                    println!("  wrong {}: exiftool {truth:?}, Numa {ours:?}", path.display());
                } else if let (Style::Other(a), Style::Other(b)) = (truth, ours) {
                    agree += 1;
                    if squash(a) != squash(b) && !b.starts_with(a.as_str()) {
                        names += 1;
                        println!("  name  {}: exiftool {a:?}, Numa {b:?}", path.display());
                    }
                } else {
                    agree += 1;
                }
            }
            println!("{make}: {agree} of {total} agree; rows exiftool N/O/U, columns Numa: {confusion:?}; {names} names differ");
        }
    }
}
