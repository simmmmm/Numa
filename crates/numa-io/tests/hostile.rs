use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("numa-hostile-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn boxed(kind: &[u8], content: &[u8]) -> Vec<u8> {
    let mut out = ((content.len() + 8) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(content);
    out
}

fn quick<T>(what: &str, work: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let out = work();
    let took = started.elapsed();
    assert!(took < Duration::from_secs(10), "{what} took {took:?}");
    out
}

#[test]
fn a_hostile_mp4_is_nothing() {
    let dir = dir("mp4");
    let mut files: Vec<(&str, Vec<u8>)> = Vec::new();

    let mut huge = vec![0, 0, 0, 1];
    huge.extend_from_slice(b"free");
    huge.extend_from_slice(&u64::MAX.to_be_bytes());
    files.push(("huge-size", huge));

    let mut wrap = vec![0, 0, 0, 1];
    wrap.extend_from_slice(b"free");
    wrap.extend_from_slice(&(u64::MAX - 3).to_be_bytes());
    wrap.extend_from_slice(&[0; 64]);
    files.push(("wrap", wrap));

    let mut big = (u32::MAX).to_be_bytes().to_vec();
    big.extend_from_slice(b"moov");
    files.push(("big-moov", big));

    files.push(("short-mvhd", boxed(b"moov", &boxed(b"mvhd", &[1, 0, 0, 0, 0, 0]))));

    files.push(("zero-scale", boxed(b"moov", &boxed(b"mvhd", &[0u8; 100]))));

    files.push(("short-tkhd", boxed(b"moov", &boxed(b"trak", &boxed(b"tkhd", &[0u8; 20])))));

    let mut many = Vec::new();
    for _ in 0..2_000_000 {
        many.extend_from_slice(&[0, 0, 0, 8]);
        many.extend_from_slice(b"free");
    }
    files.push(("many-boxes", many));

    let mut late = Vec::new();
    for _ in 0..5_000 {
        late.extend_from_slice(&[0, 0, 0, 8]);
        late.extend_from_slice(b"free");
    }
    let mut mvhd = vec![0u8; 100];
    mvhd[4..8].copy_from_slice(&3_000_000_000u32.to_be_bytes());
    let index = boxed(b"moov", &boxed(b"mvhd", &mvhd));
    late.extend_from_slice(&index);
    let (path, early) = (dir.join("late.mp4"), dir.join("early.mp4"));
    std::fs::write(&path, &late).unwrap();
    std::fs::write(&early, &index).unwrap();
    assert!(numa_io::video::probe(&early).is_some_and(|film| film.created.is_some()), "the index is read");
    assert_eq!(numa_io::video::probe(&path), None, "but not after thousands of boxes");

    let mut inner = Vec::new();
    for _ in 0..1_000_000 {
        inner.extend_from_slice(&[0, 0, 0, 8]);
        inner.extend_from_slice(b"trak");
    }
    files.push(("many-traks", boxed(b"moov", &inner)));
    for (name, bytes) in files {
        let path = dir.join(format!("{name}.mp4"));
        std::fs::write(&path, bytes).unwrap();
        quick(name, || {
            let _ = numa_io::video::probe(&path);
            let _ = numa_io::video::stamp(&path);
        });
    }
}

fn jpeg_with_size(width: u16, height: u16) -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xC0, 0, 17, 8];
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    out.extend_from_slice(&[0xFF, 0xDA, 0, 12, 3, 1, 0, 2, 0x11, 3, 0x11, 0, 63, 0]);
    out.extend_from_slice(&[0; 64]);
    out.extend_from_slice(&[0xFF, 0xD9]);
    out
}

fn png_with_size(width: u32, height: u32) -> Vec<u8> {
    let mut out = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13];
    out.extend_from_slice(b"IHDR");
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
    out
}

fn tiff(count: u32, little: bool, next_self: bool) -> Vec<u8> {
    let (head, w16, w32): (&[u8], fn(u16) -> [u8; 2], fn(u32) -> [u8; 4]) = match little {
        true => (b"II*\0", u16::to_le_bytes, u32::to_le_bytes),
        false => (b"MM\0*", u16::to_be_bytes, u32::to_be_bytes),
    };
    let mut out = head.to_vec();
    out.extend_from_slice(&w32(8));
    out.extend_from_slice(&w16(count.min(65535) as u16));

    for _ in 0..3 {
        out.extend_from_slice(&w16(0x9003));
        out.extend_from_slice(&w16(2));
        out.extend_from_slice(&w32(u32::MAX));
        out.extend_from_slice(&w32(u32::MAX - 4));
    }
    out.extend_from_slice(&w32(if next_self { 8 } else { 0 }));
    out
}

fn exif_jpeg(tiff: &[u8]) -> Vec<u8> {
    let mut app = b"Exif\0\0".to_vec();
    app.extend_from_slice(tiff);
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
    out.extend_from_slice(&((app.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&app);
    out.extend_from_slice(&jpeg_with_size(8, 8)[2..]);
    out
}

fn use_everything(path: &Path) {
    let say = |what: &str| eprintln!("  {} {what}", path.file_name().unwrap().to_string_lossy());
    say("stamp");
    let _ = numa_io::exif::stamp(path);
    say("taken");
    let _ = numa_io::exif::taken(path);
    say("rating");
    let _ = numa_io::exif::rating(path);
    say("all");
    let _ = numa_io::exif::all(path);
    say("position");
    let _ = numa_io::exif::position(path);
    say("photograph_and_file");
    let _ = numa_io::exif::photograph_and_file(path);
    say("capture");
    let _ = numa_io::capture::read(path);
    say("load_thumbnail");
    let _ = numa_io::raw::load_thumbnail(path, 320);
}

#[test]
fn hostile_stills_are_nothing() {
    let dir = dir("stills");
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("jpeg-65535.jpg".into(), jpeg_with_size(65535, 65535)),
        ("jpeg-zero.jpg".into(), jpeg_with_size(0, 0)),
        ("png-huge.png".into(), png_with_size(u32::MAX, u32::MAX)),
        ("png-wide.png".into(), png_with_size(1 << 30, 1)),
        ("empty.jpg".into(), Vec::new()),
        ("soi.jpg".into(), vec![0xFF, 0xD8]),
        ("garbage.tif".into(), vec![0x49, 0x49, 0x2A, 0x00, 0xFF, 0xFF, 0xFF, 0xFF]),
    ];
    for (little, count, looped) in [(true, 65535, true), (false, 65535, true), (true, 3, true), (false, 1, false)] {
        files.push((format!("loop-{little}-{count}-{looped}.tif"), tiff(count, little, looped)));
        files.push((format!("loop-{little}-{count}-{looped}.jpg"), exif_jpeg(&tiff(count, little, looped))));
        files.push((format!("loop-{little}-{count}-{looped}.dng"), tiff(count, little, looped)));
    }

    let mut app = vec![0xFF, 0xD8];
    for _ in 0..10_000 {
        app.extend_from_slice(&[0xFF, 0xE1, 0, 8, b'E', b'x', b'i', b'f', 0, 0]);
    }
    files.push(("many-app1.jpg".into(), app));
    for (name, bytes) in files {
        let path = dir.join(&name);
        std::fs::write(&path, bytes).unwrap();
        quick(&name, || {
            let panicked = std::panic::catch_unwind(|| use_everything(&path)).is_err();
            assert!(!panicked, "{name}: a panic came out of a reader");
        });
    }
}
