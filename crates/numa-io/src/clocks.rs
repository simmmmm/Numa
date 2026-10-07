use std::collections::{HashMap, HashSet};

use numa_cull as cull;

use crate::catalog::{Catalog, Clock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moment {
    pub id: i64,

    pub camera: String,

    pub taken: i64,
    pub own: i64,

    pub hash: u64,
    pub shape: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {

    pub camera: String,

    pub reference: Option<String>,

    pub seconds: i64,

    pub pair: (i64, i64),

    pub agree: usize,
}

pub const WINDOW: i64 = 20 * 60;

pub const AGREE: i64 = 5;

pub const MATCHES: usize = 3;

pub const NEAR: u32 = cull::ECHO;

const PREFIX: &str = "NUMA:";

pub fn shared_moments(moments: &[Moment]) -> Vec<Offer> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for moment in moments {
        *counts.entry(moment.camera.as_str()).or_default() += 1;
    }
    let Some(reference) = counts.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))).map(|(camera, _)| *camera) else {
        return Vec::new();
    };
    let mut ours: Vec<&Moment> = moments.iter().filter(|moment| moment.camera == reference).collect();
    ours.sort_by_key(|moment| moment.taken);

    let mut others: Vec<&str> = counts.keys().copied().filter(|camera| *camera != reference).collect();
    others.sort();
    let mut offers = Vec::new();
    for camera in others {

        let mut found: Vec<(i64, u32, i64, i64)> = Vec::new();
        for theirs in moments.iter().filter(|moment| moment.camera == camera) {
            let from = ours.partition_point(|moment| moment.taken < theirs.taken - WINDOW);
            let best = ours[from..]
                .iter()
                .take_while(|moment| moment.taken <= theirs.taken + WINDOW)
                .map(|moment| (cull::distance(moment.hash, theirs.hash) + cull::distance(moment.shape, theirs.shape), *moment))
                .min_by_key(|(apart, _)| *apart);
            if let Some((apart, moment)) = best.filter(|(apart, _)| *apart <= NEAR) {
                found.push((moment.taken - theirs.own, apart, moment.id, theirs.id));
            }
        }
        let theirs: Vec<&Moment> = moments.iter().filter(|moment| moment.camera == camera).collect();
        let by_rhythm = || {
            let rhythm = rhythm::rhythm(&ours, &theirs).filter(rhythm::Rhythm::clear)?;
            let seconds = theirs[0].taken - theirs[0].own + rhythm.lag;
            let agrees = found.is_empty() || found.iter().any(|(offset, ..)| (offset - seconds).abs() <= AGREE);
            agrees.then_some(Offer { camera: String::new(), reference: None, seconds, pair: rhythm.pair, agree: rhythm.coincide })
        };
        let by_hash = agreeing(&mut found.clone());
        if let Some(offer) = by_hash.or_else(by_rhythm) {
            offers.push(Offer { camera: camera.to_string(), reference: Some(reference.to_string()), ..offer });
        }
    }
    offers
}

fn agreeing(found: &mut [(i64, u32, i64, i64)]) -> Option<Offer> {
    found.sort();
    let (mut start, mut best) = (0, (0, 0));
    for end in 0..found.len() {
        while found[end].0 - found[start].0 > AGREE {
            start += 1;
        }
        if end + 1 - start > best.1 - best.0 {
            best = (start, end + 1);
        }
    }
    let run = &found[best.0..best.1];
    if run.len() < MATCHES {
        return None;
    }
    let closest = run.iter().min_by_key(|(_, apart, _, _)| *apart)?;
    Some(Offer {
        camera: String::new(),
        reference: None,
        seconds: run[run.len() / 2].0,
        pair: (closest.2, closest.3),
        agree: run.len(),
    })
}

pub fn from_slates(slates: &[(i64, String, i64, i64)]) -> Vec<Offer> {
    let mut by_camera: HashMap<&str, Vec<(i64, i64)>> = HashMap::new();
    for (id, camera, own, shown) in slates {
        by_camera.entry(camera.as_str()).or_default().push((shown - own, *id));
    }
    let mut offers: Vec<Offer> = by_camera
        .into_iter()
        .map(|(camera, mut found)| {
            found.sort();
            let (seconds, id) = found[found.len() / 2];
            Offer { camera: camera.to_string(), reference: None, seconds, pair: (id, id), agree: found.len() }
        })
        .collect();
    offers.sort_by(|a, b| a.camera.cmp(&b.camera));
    offers
}

pub fn offers(catalog: &Catalog, library_id: i64) -> Result<Vec<Offer>, String> {
    let clocks: HashMap<String, Clock> = catalog
        .cameras(library_id)?
        .into_iter()
        .filter_map(|camera| Some((camera.key, camera.clock?)))
        .collect();
    let declined = catalog.declined_clocks(library_id);
    let mut offers = from_slates(&catalog.slates(library_id)?);
    let slated: HashSet<String> = offers.iter().map(|offer| offer.camera.clone()).collect();
    offers.extend(shared_moments(&catalog.moments(library_id)?).into_iter().filter(|offer| !slated.contains(&offer.camera)));
    let near = |a: i64, b: i64| (a - b).abs() <= AGREE;
    offers.retain(|offer| {
        let clock = clocks.get(&offer.camera);
        let shift = clock.filter(|clock| clock.applied).map_or(0, |clock| clock.seconds);
        !near(offer.seconds, shift)
            && !clock.is_some_and(|clock| near(clock.seconds, offer.seconds))
            && !declined.get(&offer.camera).is_some_and(|seconds| near(*seconds, offer.seconds))
    });
    Ok(offers)
}

pub fn payload(millis: i64) -> String {
    format!("{PREFIX}{millis}")
}

pub fn shown(text: &str) -> Option<i64> {
    text.strip_prefix(PREFIX)?.parse::<i64>().ok().map(|millis| millis.div_euclid(1000))
}

pub fn code(text: &str) -> Option<(usize, Vec<bool>)> {
    let code = qrcode::QrCode::with_error_correction_level(text, qrcode::EcLevel::M).ok()?;
    Some((code.width(), code.to_colors().into_iter().map(|colour| colour == qrcode::Color::Dark).collect()))
}

pub fn read(image: &image::RgbImage) -> Option<i64> {
    let grey = image::imageops::grayscale(image);
    let (width, height) = (grey.width() as usize, grey.height() as usize);
    let mut prepared =
        rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| grey.get_pixel(x as u32, y as u32).0[0]);
    prepared.detect_grids().into_iter().find_map(|grid| shown(&grid.decode().ok()?.1))
}

pub fn camera_name(key: &str, every: &[String]) -> String {
    let mut parts = key.split('|');
    let (make, model, serial) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let name = if model.is_empty() { make.to_string() } else { model.to_string() };
    let twins = every.iter().filter(|other| other.rsplit_once('|').map(|(body, _)| body) == key.rsplit_once('|').map(|(body, _)| body)).count();
    match twins > 1 && !serial.is_empty() {
        true => format!("{name} ··{}", &serial[serial.len().saturating_sub(4)..]),
        false => name,
    }
}

pub fn when(seconds: i64) -> String {
    let rest = seconds.rem_euclid(86_400);
    format!("{} {:02}:{:02}:{:02}", crate::import::day(seconds, 0), rest / 3600, rest / 60 % 60, rest % 60)
}

pub fn span(seconds: i64) -> String {
    let all = seconds.unsigned_abs();
    let (hours, minutes, rest) = (all / 3600, all / 60 % 60, all % 60);
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{hours} h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes} min"));
    }
    if rest > 0 || parts.is_empty() {
        parts.push(format!("{rest} s"));
    }
    parts.join(" ")
}

pub fn off(seconds: i64) -> String {
    match seconds {
        0 => "on time".to_string(),
        s if s < 0 => format!("{} ahead", span(s)),
        s => format!("{} behind", span(s)),
    }
}

pub fn runs(seconds: i64) -> String {
    match seconds {
        0 => "is on time".to_string(),
        s => format!("runs {}", off(s)),
    }
}

pub mod rhythm;

#[cfg(test)]
mod tests;
