use std::path::Path;

use numa_cull as cull;

use crate::catalog::{Catalog, Filter, Flag, Photo};
use crate::raw;

pub type Measured = (cull::Frame, Option<u32>, Option<f32>, Vec<([f32; cull::people::LENGTH], Option<Vec<u8>>)>);

pub const FACE_EDGE: u32 = 640;

const EYE_EDGE: u32 = 4800;

pub fn measure_photo(path: &Path) -> Result<Measured, String> {

    raw::load_scaled(path, EYE_EDGE).map(|full| {
        let shrink = FACE_EDGE as f32 / full.width().max(full.height()) as f32;
        let image = match shrink < 1.0 {
            true => image::imageops::thumbnail(
                &full,
                ((full.width() as f32 * shrink).round() as u32).max(1),
                ((full.height() as f32 * shrink).round() as u32).max(1),
            ),
            false => full.clone(),
        };
        let mut frame = cull::measure::of(&image);

        frame.clock = crate::clocks::read(&image);

        if let Some((exposure, focal35)) = raw::shot(path) {
            frame.exposure = exposure;
            frame.focal35 = focal35;
        }

        if let Some((clipped, dark)) = raw::raw_levels(path) {
            frame.raw_clipped = Some(clipped);
            frame.raw_dark = Some(dark);
        }

        let found = cull::faces::detect(&image);
        let faces = found.as_ref().map(|faces| faces.len() as u32);
        let sharpest = found
            .as_ref()
            .and_then(|faces| cull::faces::sharpest_face(&image, faces));

        frame.eyes_closed = found
            .iter()
            .flatten()
            .max_by(|a, b| a.width.total_cmp(&b.width))
            .and_then(|face| cull::eyes::closed(&full, face, full.width() as f32 / image.width() as f32));

        let embeddings = found
            .iter()
            .flatten()
            .filter_map(|face| {
                Some((
                    cull::people::embed(&image, face)?,
                    cull::people::portrait_jpeg(&image, face),
                ))
            })
            .collect::<Vec<_>>();
        (frame, faces, sharpest, embeddings)
    })
}

pub fn summary(total: usize, failed: usize, grouped: Result<(usize, cull::learn::Outcome), String>) -> String {

    let faces = if cull::faces::is_installed() {
        String::new()
    } else {
        format!(" · {}", cull::faces::model_missing_message())
    };

    match (total, failed, grouped) {
        (0, _, Ok((bursts, learned))) => format!("Already analysed — {bursts} burst(s) · {learned}{faces}"),
        (_, 0, Ok((bursts, learned))) => {
            format!("Analysed {total} photo(s) — {bursts} burst(s) · {learned}{faces}")
        }
        (_, n, Ok((bursts, learned))) => format!(
            "Analysed {} of {total} — {bursts} burst(s), {n} unreadable · {learned}{faces}",
            total - n
        ),
        (_, _, Err(err)) => format!("Grouping failed: {err}"),
    }
}

pub fn twins<'a>(
    photos: impl IntoIterator<Item = (i64, &'a Path, Option<i64>)>,
    is_raw: impl Fn(&Path) -> bool,
) -> std::collections::HashMap<i64, i64> {
    let key = |path: &Path, taken: i64| (path.file_stem().map(|stem| stem.to_string_lossy().to_uppercase()), taken);
    let dated: Vec<(i64, &Path, i64)> =
        photos.into_iter().filter_map(|(id, path, taken)| Some((id, path, taken?))).collect();
    let raws: std::collections::HashMap<_, i64> =
        dated.iter().filter(|(_, path, _)| is_raw(path)).map(|(id, path, taken)| (key(path, *taken), *id)).collect();
    dated
        .iter()
        .filter(|(_, path, _)| !is_raw(path))
        .filter_map(|(id, path, taken)| Some((*id, *raws.get(&key(path, *taken))?)))
        .collect()
}

pub fn regroup_bursts(catalog: &Catalog, library_id: i64) -> Result<(usize, cull::learn::Outcome), String> {
    let analysed = catalog.analysed(library_id)?;
    if analysed.is_empty() {
        return Ok((0, cull::learn::Outcome::TooFew { rated: 0 }));
    }

    let marks: std::collections::HashMap<i64, Photo> =
        catalog.photos(library_id, &Filter::default())?.into_iter().map(|photo| (photo.id, photo)).collect();

    let measured: std::collections::HashSet<i64> = analysed.iter().map(|(id, _, _)| *id).collect();
    let twin_of: std::collections::HashMap<i64, i64> =
        twins(marks.values().map(|photo| (photo.id, photo.path.as_path(), photo.taken)), raw::is_raw)
            .into_iter()
            .filter(|(_, raw)| measured.contains(raw))
            .collect();
    let (twinned, analysed): (Vec<_>, Vec<_>) = analysed.into_iter().partition(|(id, _, _)| twin_of.contains_key(id));

    let slates: std::collections::HashSet<i64> = catalog.slates(library_id)?.into_iter().map(|(id, ..)| id).collect();
    let analysed: Vec<_> = analysed.into_iter().filter(|(id, _, _)| !slates.contains(id)).collect();

    let frames: Vec<cull::Frame> = analysed.iter().map(|(_, frame, _)| *frame).collect();
    let hashes: Vec<u64> = frames.iter().map(|frame| frame.hash).collect();

    let taken: Vec<i64> = analysed
        .iter()
        .map(|(id, _, _)| marks.get(id).map_or(0, |photo| photo.taken.unwrap_or(photo.mtime)))
        .collect();

    let bodies = catalog.bodies(library_id)?;
    let body: Vec<&str> = analysed.iter().map(|(id, _, _)| bodies.get(id).map_or("", String::as_str)).collect();
    let groups = cull::bursts_by_body(&hashes, &taken, &body, cull::BURST_TOLERANCE);
    let faces: Vec<Option<f32>> = analysed.iter().map(|(_, _, face)| *face).collect();
    let best = cull::best_of_each(&frames, &faces, &groups);

    let best: std::collections::HashSet<usize> = best.into_iter().collect();

    let scale = cull::Scale::of(analysed.iter().map(|(_, frame, face)| face.unwrap_or(frame.sharpness)));
    let samples: Vec<cull::learn::Sample> = analysed
        .iter()
        .enumerate()
        .map(|(index, (id, frame, face))| {
            let photo = marks.get(id);
            cull::learn::Sample::new(
                scale,
                frame,
                photo.and_then(|photo| photo.faces),
                *face,
                best.contains(&index),
                groups[index],
                photo.map_or(0, |photo| photo.rating),
                photo.is_some_and(|photo| photo.flag == Flag::Rejected),
            )
        })
        .collect();
    let (suggested, learned) = cull::learn::score(&samples);
    let mut rows: Vec<(i64, Option<usize>, bool, f32)> = analysed
        .iter()
        .enumerate()
        .map(|(index, (id, _, _))| (*id, Some(groups[index]), best.contains(&index), suggested[index]))
        .collect();
    let by_raw: std::collections::HashMap<i64, f32> = rows.iter().map(|(id, _, _, suggested)| (*id, *suggested)).collect();
    rows.extend(twinned.iter().filter_map(|(id, _, _)| Some((*id, None, false, *by_raw.get(twin_of.get(id)?)?))));

    catalog.save_bursts(&rows)?;

    let found = cull::echoes(&frames, &groups, &taken);
    let echoes: Vec<(i64, Option<i64>)> = analysed
        .iter()
        .zip(&found)
        .map(|((id, _, _), echo)| (*id, echo.map(|index| analysed[index].0)))
        .chain(twinned.iter().map(|(id, _, _)| (*id, None)))
        .collect();
    catalog.save_echoes(&echoes)?;
    Ok((best.len(), learned))
}
