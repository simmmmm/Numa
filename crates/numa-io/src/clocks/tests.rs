use super::*;

fn noise(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn frame(id: i64, camera: &str, own: i64, seed: u64) -> Moment {
    Moment { id, camera: camera.into(), taken: own, own, hash: noise(seed), shape: noise(seed + 1000) }
}

fn wedding(shared: &[i64]) -> Vec<Moment> {
    let mut moments: Vec<Moment> = (0..40).map(|i| frame(i, "FUJIFILM|X-T5|1", 1000 + i * 60, i as u64)).collect();
    for (n, i) in shared.iter().enumerate() {
        let mut theirs = frame(100 + n as i64, "SONY|ILCE-7M4|", 1000 + i * 60 + 192, *i as u64);

        theirs.hash ^= 0b1011;
        moments.push(theirs);
    }
    moments.extend((0..30).map(|i| frame(200 + i, "SONY|ILCE-7M4|", 1030 + i * 70, 500 + i as u64)));
    moments
}

#[test]
fn three_shared_moments_line_the_second_body_up() {
    let offers = shared_moments(&wedding(&[5, 6, 20, 31]));
    assert_eq!(offers.len(), 1);
    let offer = &offers[0];
    assert_eq!((offer.camera.as_str(), offer.reference.as_deref()), ("SONY|ILCE-7M4|", Some("FUJIFILM|X-T5|1")));
    assert_eq!(offer.seconds, -192, "its clock is ahead, so it is taken back");
    assert_eq!(offer.agree, 4);
    assert!(offer.pair.0 < 40 && offer.pair.1 >= 100, "the X-T5's frame, then the A7 IV's");
}

#[test]
fn two_are_not_enough_and_one_body_has_nothing_to_agree_with() {
    assert!(shared_moments(&wedding(&[5, 6])).is_empty());
    let alone: Vec<Moment> = wedding(&[]).into_iter().filter(|moment| moment.camera.starts_with("FUJIFILM")).collect();
    assert!(shared_moments(&alone).is_empty());
}

#[test]
fn matches_that_disagree_offer_nothing() {
    let mut moments = wedding(&[]);
    for (n, (i, late)) in [(5, 300), (12, -400), (25, 900)].into_iter().enumerate() {
        moments.push(frame(300 + n as i64, "SONY|ILCE-7M4|", 1000 + i * 60 + late, i as u64));
    }
    assert!(shared_moments(&moments).is_empty());
}

fn two_shooters(same_moments: bool, ahead: i64) -> Vec<Moment> {
    let mut moments = Vec::new();
    let mut id = 0;
    let mut push = |camera: &str, own: i64| {
        id += 1;
        moments.push(Moment { id, camera: camera.into(), taken: own, own, hash: noise(id as u64), shape: noise(id as u64 + 7) });
    };

    let draw = |what: u64, n: u64| noise(1_000_000 + what * 10_000 + n);
    for event in 0..200u64 {
        let at = 50_000 + (draw(0, event) % 10_800) as i64;
        for frame in 0..1 + draw(1, event) % 5 {
            push("A", at + frame as i64);
        }
        if draw(2, event) % 10 < 6 {
            let theirs = if same_moments { at } else { 50_000 + (draw(3, event) % 10_800) as i64 };
            for frame in 0..1 + draw(4, event) % 3 {
                push("B", theirs + (draw(5, event) % 2) as i64 + frame as i64 + ahead);
            }
        }
    }
    for extra in 0..100u64 {
        push("A", 50_000 + (draw(6, extra) % 10_800) as i64);
        push("B", 50_000 + (draw(7, extra) % 10_800) as i64 + ahead);
    }
    moments
}

#[test]
fn the_rhythm_lines_up_shooters_who_share_no_framing() {
    let offers = shared_moments(&two_shooters(true, 192));
    assert_eq!(offers.len(), 1);
    assert!((offers[0].seconds + 192).abs() <= 1, "{:?}", offers[0]);
    assert!(shared_moments(&two_shooters(false, 192)).is_empty());

    let moments = two_shooters(true, 0);
    let ours: Vec<&Moment> = moments.iter().filter(|moment| moment.camera == "A").collect();
    let theirs: Vec<&Moment> = moments.iter().filter(|moment| moment.camera == "B").collect();
    assert!(!rhythm::rhythm(&ours, &theirs).unwrap().clear());
}

#[test]
fn a_clock_photo_says_its_body_s_offset() {
    let slates = vec![(7, "Apple|iPhone 16|".to_string(), 5000, 5047), (9, "Apple|iPhone 16|".to_string(), 6000, 6046)];
    let offers = from_slates(&slates);
    assert_eq!(offers.len(), 1);
    assert_eq!((offers[0].seconds, offers[0].reference.clone(), offers[0].agree), (47, None, 2));
}

#[test]
fn the_clock_s_code_reads_back() {
    let millis = 1_790_000_000_999;
    assert_eq!(shown(&payload(millis)), Some(1_790_000_000), "floored, as a camera floors");
    let (width, modules) = code(&payload(millis)).unwrap();
    assert_eq!(width, 21, "version 1: the largest modules");

    let (scale, quiet) = (8u32, 4u32);
    let mut image = image::RgbImage::from_pixel(640, 427, image::Rgb([200, 200, 200]));
    let side = (width as u32 + 2 * quiet) * scale;
    for y in 0..side {
        for x in 0..side {
            let (mx, my) = ((x / scale) as i64 - quiet as i64, (y / scale) as i64 - quiet as i64);
            let dark = (0..width as i64).contains(&mx)
                && (0..width as i64).contains(&my)
                && modules[my as usize * width + mx as usize];
            image.put_pixel(100 + x, 50 + y, image::Rgb(if dark { [20, 20, 20] } else { [250, 250, 250] }));
        }
    }
    assert_eq!(read(&image), Some(1_790_000_000));
    assert_eq!(read(&image::RgbImage::from_pixel(64, 64, image::Rgb([128, 128, 128]))), None);
}

#[test]
fn a_clock_off_reads_in_words() {
    assert_eq!(runs(-192), "runs 3 min 12 s ahead");
    assert_eq!(runs(47), "runs 47 s behind");
    assert_eq!(span(3900), "1 h 5 min");
    assert_eq!(span(0), "0 s");
    assert_eq!(when(19_190 * 86_400 + 44_072), "2022-07-17 12:14:32");
    let every = vec!["FUJIFILM|X-T5|2D001165".to_string(), "FUJIFILM|X-T5|2D009999".to_string(), "Apple|iPhone 16|".to_string()];
    assert_eq!(camera_name("Apple|iPhone 16|", &every), "iPhone 16");
    assert_eq!(camera_name("FUJIFILM|X-T5|2D001165", &every), "X-T5 ··1165", "two of one model");
}

#[test]
#[ignore]
fn one_body_dealt_out_as_two() {
    let Some(file) = std::env::var_os("NUMA_MOMENTS") else {
        println!("NUMA_MOMENTS is not set");
        return;
    };
    let read = |file: &std::ffi::OsStr| -> Vec<(i64, i64, u64, u64)> {
        std::fs::read_to_string(file)
            .unwrap()
            .lines()
            .map(|line| line.split('\t').collect::<Vec<_>>())
            .filter(|row| row[4].parse::<f32>().unwrap() >= numa_cull::BLANK)
            .map(|row| (row[0].parse().unwrap(), row[1].parse().unwrap(), row[2].parse::<i64>().unwrap() as u64, row[3].parse::<i64>().unwrap() as u64))
            .collect()
    };
    let frame = |(id, taken, hash, shape): (i64, i64, u64, u64), camera: &str, shift: i64, alike: bool| Moment {
        id,
        camera: camera.into(),
        taken: taken + shift,
        own: taken + shift,
        hash: if alike { hash } else { noise(id as u64) },
        shape: if alike { shape } else { noise(id as u64 + 7) },
    };
    let report = |what: &str, moments: &[Moment]| {
        let ours: Vec<&Moment> = moments.iter().filter(|moment| moment.camera == "A").collect();
        let theirs: Vec<&Moment> = moments.iter().filter(|moment| moment.camera == "B").collect();
        let started = std::time::Instant::now();
        let offers = shared_moments(moments);
        let found = rhythm::rhythm(&ours, &theirs);
        println!("{what}: {offers:?} in {:.1?}; rhythm {found:?}", started.elapsed());
    };
    let rows = read(&file);
    for (every, shift) in [(2, 0), (2, 192), (2, -600), (2, 1100), (4, 192)] {
        let moments: Vec<Moment> = rows
            .iter()
            .enumerate()
            .map(|(n, row)| match n % every == 1 {
                true => frame(*row, "B", shift, false),
                false => frame(*row, "A", 0, true),
            })
            .collect();
        report(&format!("{} frames, one in {every} to B, B {shift:+} s, nothing alike", moments.len()), &moments);
    }
    if let Some(other) = std::env::var_os("NUMA_OTHER") {
        let others = read(&other);
        let first = |rows: &[(i64, i64, u64, u64)]| rows.iter().map(|row| row.1.div_euclid(86_400)).min().unwrap();
        let days = (first(&rows) - first(&others)) * 86_400;
        let mut moments: Vec<Moment> = rows.iter().map(|row| frame(*row, "A", 0, true)).collect();
        moments.extend(others.iter().map(|row| frame((row.0 + 1_000_000, row.1, row.2, row.3), "B", days, false)));
        report("two unrelated shoots on the same days", &moments);
    }
}

#[test]
#[ignore]
fn how_small_a_code_reads() {
    let (width, modules) = code(&payload(1_790_000_000_123)).unwrap();
    for scale in 2..=10u32 {
        let mut image = image::RgbImage::from_pixel(640, 427, image::Rgb([40, 40, 44]));
        let side = (width as u32 + 8) * scale;
        for y in 0..side.min(427) {
            for x in 0..side.min(640) {
                let (mx, my) = ((x / scale) as i64 - 4, (y / scale) as i64 - 4);
                let dark = (0..width as i64).contains(&mx) && (0..width as i64).contains(&my) && modules[my as usize * width + mx as usize];
                image.put_pixel(20 + x.min(619), 20 + y.min(406), image::Rgb(if dark { [20, 20, 20] } else { [235, 235, 235] }));
            }
        }
        let blurred = image::imageops::blur(&image, 1.0);
        println!("{scale} px a module ({side} px of 640): {:?}", read(&blurred));
    }
    if let Some(photo) = std::env::var_os("NUMA_PHOTO") {
        let image = image::open(photo).unwrap().thumbnail(640, 640).to_rgb8();
        let started = std::time::Instant::now();
        for _ in 0..20 {
            assert_eq!(read(&image), None);
        }
        println!("a photograph with no code: {:.2?} a read", started.elapsed() / 20);
    }
}
