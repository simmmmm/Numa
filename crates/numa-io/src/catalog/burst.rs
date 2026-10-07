use super::Photo;

pub fn sharper_in_burst(photos: &[Photo], id: i64) -> Option<(&Photo, usize, usize)> {
    let members = burst_of(photos, id);
    let at = members.iter().position(|photo| photo.best_of_burst && photo.id != id)?;
    Some((members[at], at + 1, members.len()))
}

pub fn burst_of(photos: &[Photo], id: i64) -> Vec<&Photo> {
    let Some(burst) = photos.iter().find(|photo| photo.id == id).and_then(|photo| photo.burst) else { return Vec::new() };
    let mut members: Vec<&Photo> = photos.iter().filter(|photo| photo.burst == Some(burst)).collect();
    members.sort_by_key(|photo| (photo.taken.unwrap_or(photo.mtime), photo.id));
    members
}
