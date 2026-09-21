use avif_serialize::constants::{ColorPrimaries, MatrixCoefficients as Matrix, TransferCharacteristics};
use numa_core::space::ColourSpace;
use rav1e::color::{ChromaSampling, ColorDescription, PixelRange};
use rav1e::prelude::*;

pub fn space_for(asked: ColourSpace) -> ColourSpace {
    match asked {
        ColourSpace::Srgb => ColourSpace::Srgb,
        _ => ColourSpace::DisplayP3,
    }
}

pub fn encode(pixels: &[u16], width: u32, height: u32, space: ColourSpace, quality: u8, exif: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let (width, height) = (width as usize, height as usize);
    let quantizer = quantizer(quality as f32);
    let primaries = match space_for(space) {
        ColourSpace::DisplayP3 => (rav1e::color::ColorPrimaries::SMPTE432, ColorPrimaries::DisplayP3),
        _ => (rav1e::color::ColorPrimaries::BT709, ColorPrimaries::Bt709),
    };
    let config = Config::new()
        .with_encoder_config(EncoderConfig {
            width,
            height,
            bit_depth: 10,
            chroma_sampling: ChromaSampling::Cs444,
            pixel_range: PixelRange::Full,
            color_description: Some(ColorDescription {
                color_primaries: primaries.0,
                transfer_characteristics: rav1e::color::TransferCharacteristics::SRGB,
                matrix_coefficients: rav1e::color::MatrixCoefficients::BT709,
            }),
            still_picture: true,
            quantizer: quantizer as usize,
            min_quantizer: quantizer,
            tune: Tune::Psychovisual,

            tiles: rayon::current_num_threads().min(width * height / (128 * 128)).max(1),
            speed_settings: SpeedSettings::from_preset(6),
            ..Default::default()
        })
        .with_threads(rayon::current_num_threads());
    let mut context: Context<u16> = config.new_context().map_err(|err| err.to_string())?;

    let mut frame = context.new_frame();
    let [y, u, v] = &mut frame.planes;
    let (kr, kb) = (0.2126f32, 0.0722f32);
    for (row, ((y, u), v)) in y.mut_slice(Default::default()).rows_iter_mut()
        .zip(u.mut_slice(Default::default()).rows_iter_mut())
        .zip(v.mut_slice(Default::default()).rows_iter_mut())
        .take(height)
        .enumerate()
    {
        let line = &pixels[row * width * 3..(row + 1) * width * 3];
        for (x, rgb) in line.chunks_exact(3).enumerate() {
            let [r, g, b] = [rgb[0], rgb[1], rgb[2]].map(|value| value as f32 / 65535.0);
            let luma = kr * r + (1.0 - kr - kb) * g + kb * b;
            let ten = |value: f32| (value * 1023.0).round().clamp(0.0, 1023.0) as u16;
            y[x] = ten(luma);
            u[x] = ten((b - luma) / (2.0 * (1.0 - kb)) + 0.5);
            v[x] = ten((r - luma) / (2.0 * (1.0 - kr)) + 0.5);
        }
    }
    context.send_frame(frame).map_err(|err| err.to_string())?;
    context.flush();
    let mut av1 = Vec::new();
    loop {
        match context.receive_packet() {
            Ok(mut packet) => av1.append(&mut packet.data),
            Err(EncoderStatus::Encoded) => continue,
            Err(EncoderStatus::LimitReached) => break,
            Err(err) => return Err(err.to_string()),
        }
    }

    let mut container = avif_serialize::Aviffy::new();
    container
        .set_color_primaries(primaries.1)
        .set_transfer_characteristics(TransferCharacteristics::Srgb)
        .set_matrix_coefficients(Matrix::Bt709)
        .set_full_color_range(true);
    if let Some(exif) = exif {
        container.set_exif(exif.to_vec());
    }
    Ok(container.to_vec(&av1, None, width as u32, height as u32, 10))
}

fn quantizer(quality: f32) -> u8 {
    let q = quality.clamp(1.0, 100.0) / 100.0;
    let x = if q >= 0.82 { (1.0 - q) * 2.6 } else if q > 0.25 { q.mul_add(-0.5, 1.0 - 0.125) } else { 1.0 - q };
    (x * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_is_an_avif_in_its_own_primaries() {
        let (width, height) = (64u32, 48u32);
        let pixels: Vec<u16> = (0..width * height * 3).map(|i| ((i * 97) % 65536) as u16).collect();
        let file = encode(&pixels, width, height, ColourSpace::DisplayP3, 80, None).unwrap();
        assert_eq!(&file[4..12], b"ftypavif");
        let colr = file.windows(8).position(|window| window == b"colrnclx").expect("a colour box");

        assert_eq!(&file[colr + 8..colr + 14], &[0, 12, 0, 13, 0, 1]);
        let srgb = encode(&pixels, width, height, ColourSpace::AdobeRgb, 80, None).unwrap();
        let colr = srgb.windows(8).position(|window| window == b"colrnclx").unwrap();
        assert_eq!(&srgb[colr + 8..colr + 10], &[0, 12], "Adobe RGB has no code; it is written as P3");
    }
}
