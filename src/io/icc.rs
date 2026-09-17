use crate::core::space::ColourSpace;

const D50: [f32; 3] = [0.9642, 1.0, 0.8249];

pub fn profile(space: ColourSpace) -> Vec<u8> {
    let matrix = space.to_xyz();
    let column = |c: usize| [matrix[0][c], matrix[1][c], matrix[2][c]];

    let mut tags: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"desc", description(&format!("Numa {}", space.name()))),
        (*b"cprt", text("No copyright, use freely")),
        (*b"wtpt", xyz(D50)),
        (*b"rXYZ", xyz(column(0))),
        (*b"gXYZ", xyz(column(1))),
        (*b"bXYZ", xyz(column(2))),
    ];
    let trc = curve(space);
    for name in [*b"rTRC", *b"gTRC", *b"bTRC"] {
        tags.push((name, trc.clone()));
    }

    let table_size = 4 + 12 * tags.len();
    let mut offset = 128 + table_size;
    let mut table = (tags.len() as u32).to_be_bytes().to_vec();
    let mut body = Vec::new();
    for (name, data) in &tags {
        while (offset + body.len()) % 4 != 0 {
            body.push(0);
        }
        let at = offset + body.len();
        table.extend_from_slice(name);
        table.extend_from_slice(&(at as u32).to_be_bytes());
        table.extend_from_slice(&(data.len() as u32).to_be_bytes());
        body.extend_from_slice(data);
    }
    while body.len() % 4 != 0 {
        body.push(0);
    }
    offset = 128 + table.len() + body.len();

    let mut header = Vec::with_capacity(128);
    header.extend_from_slice(&(offset as u32).to_be_bytes());
    header.extend_from_slice(&[0; 4]);
    header.extend_from_slice(&[0x02, 0x10, 0, 0]);
    header.extend_from_slice(b"mntr");
    header.extend_from_slice(b"RGB ");
    header.extend_from_slice(b"XYZ ");
    header.extend_from_slice(&[0; 12]);
    header.extend_from_slice(b"acsp");
    header.extend_from_slice(&[0; 4]);
    header.extend_from_slice(&[0; 4]);
    header.extend_from_slice(&[0; 4]);
    header.extend_from_slice(&[0; 4]);
    header.extend_from_slice(&[0; 8]);
    header.extend_from_slice(&[0; 4]);
    for value in D50 {
        header.extend_from_slice(&fixed(value));
    }
    header.extend_from_slice(&[0; 4]);
    header.resize(128, 0);

    let mut out = header;
    out.extend_from_slice(&table);
    out.extend_from_slice(&body);
    out
}

fn fixed(value: f32) -> [u8; 4] {
    ((value as f64 * 65536.0).round() as i32).to_be_bytes()
}

fn xyz(value: [f32; 3]) -> Vec<u8> {
    let mut out = b"XYZ \0\0\0\0".to_vec();
    for v in value {
        out.extend_from_slice(&fixed(v));
    }
    out
}

fn curve(space: ColourSpace) -> Vec<u8> {
    let mut out = b"curv\0\0\0\0".to_vec();
    match space {
        ColourSpace::AdobeRgb | ColourSpace::ProPhoto => {
            let gamma = if space == ColourSpace::AdobeRgb { 563.0 / 256.0 } else { 1.8 };
            out.extend_from_slice(&1u32.to_be_bytes());
            out.extend_from_slice(&(((gamma * 256.0) as f32).round() as u16).to_be_bytes());
        }
        _ => {
            const SAMPLES: u32 = 1024;
            out.extend_from_slice(&SAMPLES.to_be_bytes());
            for i in 0..SAMPLES {
                let linear = space.decode(i as f32 / (SAMPLES - 1) as f32);
                out.extend_from_slice(&((linear * 65535.0).round() as u16).to_be_bytes());
            }
        }
    }
    out
}

fn text(value: &str) -> Vec<u8> {
    let mut out = b"text\0\0\0\0".to_vec();
    out.extend_from_slice(value.as_bytes());
    out.push(0);
    out
}

fn description(value: &str) -> Vec<u8> {
    let mut out = b"desc\0\0\0\0".to_vec();
    out.extend_from_slice(&(value.len() as u32 + 1).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
    out.push(0);
    out.extend_from_slice(&[0; 8]);
    out.extend_from_slice(&[0; 3]);
    out.extend_from_slice(&[0; 67]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_is_well_formed() {
        for space in [ColourSpace::Srgb, ColourSpace::DisplayP3, ColourSpace::AdobeRgb, ColourSpace::ProPhoto] {
            let bytes = profile(space);
            assert_eq!(u32::from_be_bytes(bytes[0..4].try_into().unwrap()) as usize, bytes.len());
            assert_eq!(&bytes[36..40], b"acsp");
            assert_eq!(bytes.len() % 4, 0);
            let count = u32::from_be_bytes(bytes[128..132].try_into().unwrap()) as usize;
            assert_eq!(count, 9);
            for tag in 0..count {
                let at = 132 + tag * 12;
                let offset = u32::from_be_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
                let size = u32::from_be_bytes(bytes[at + 8..at + 12].try_into().unwrap()) as usize;
                assert!(offset + size <= bytes.len() && offset % 4 == 0, "{space:?} tag {tag}");
            }
        }
    }
}
