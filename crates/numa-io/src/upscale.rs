use numa_infer::Model;
use numa_render::Frame;

use crate::export::Channel;

pub const MODEL: &str = "realplksr_x2.onnx";

const TILE: usize = 512;

const OVERLAP: usize = 32;

pub fn is_installed() -> bool {
    numa_core::paths::model_file(&[MODEL]).is_some()
}

pub fn double<T: Channel>(frame: &Frame<T>, mut progress: impl FnMut(usize, usize) -> bool) -> Result<Option<Frame<T>>, String>
where
    image::Rgb<T>: image::Pixel<Subpixel = T>,
{
    let path = numa_core::paths::model_file(&[MODEL]).ok_or("the Super Resolution model is not installed")?;
    let model = Model::load(&path).ok_or("the Super Resolution model could not be loaded")?;
    let (width, height) = (frame.width() as usize, frame.height() as usize);
    let (across, down) = (origins(width), origins(height));
    let total = across.len() * down.len();
    let plane = TILE * TILE;
    let out_edge = 2 * TILE;

    let mut sum = vec![0.0f32; 4 * width * height * 3];
    let mut weight = vec![0.0f32; 4 * width * height];
    let raw = frame.as_raw();
    for (done, (top, left)) in down.iter().flat_map(|&top| across.iter().map(move |&left| (top, left))).enumerate() {
        if !progress(done, total) {
            return Ok(None);
        }

        let mut input = vec![0.0f32; 3 * plane];
        for ty in 0..TILE {
            let y = (top + ty).min(height - 1);
            for tx in 0..TILE {
                let at = (y * width + (left + tx).min(width - 1)) * 3;
                for channel in 0..3 {
                    input[channel * plane + ty * TILE + tx] = raw[at + channel].unit();
                }
            }
        }
        let tensor = ndarray::Array4::from_shape_vec((1, 3, TILE, TILE), input).map_err(|err| err.to_string())?.into_dyn();
        let outputs = model.run(vec![tensor.into()]).map_err(|err| err.to_string())?;
        let output: Vec<f32> = outputs.into_iter().next().ok_or("the model gave no output")?.iter().copied().collect();
        if output.len() != 3 * out_edge * out_edge {
            return Err(format!("the model answered with {} values for a {TILE}-pixel tile", output.len()));
        }
        let out_plane = out_edge * out_edge;
        for oy in 0..out_edge.min(2 * (height - top)) {
            for ox in 0..out_edge.min(2 * (width - left)) {
                let share = ramp(ox) * ramp(oy);
                let at = (2 * top + oy) * 2 * width + 2 * left + ox;
                weight[at] += share;
                for channel in 0..3 {
                    sum[at * 3 + channel] += share * output[channel * out_plane + oy * out_edge + ox];
                }
            }
        }
    }
    progress(total, total);

    let pixels = sum.chunks_exact(3).zip(&weight).flat_map(|(pixel, weight)| pixel.iter().map(move |value| T::from_unit(value / weight))).collect();
    Ok(Frame::from_raw(2 * width as u32, 2 * height as u32, pixels))
}

fn origins(length: usize) -> Vec<usize> {
    if length <= TILE {
        return vec![0];
    }
    let mut starts: Vec<usize> = (0..).map(|index| index * (TILE - OVERLAP)).take_while(|start| start + TILE < length).collect();
    starts.push(length - TILE);
    starts
}

fn ramp(position: usize) -> f32 {
    let edge = 2 * TILE;
    let from_edge = position.min(edge - 1 - position) as f32 + 0.5;
    (from_edge / (2 * OVERLAP) as f32).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_cover_every_edge() {
        assert_eq!(origins(300), vec![0]);
        let starts = origins(1500);
        assert_eq!(*starts.last().unwrap(), 1500 - TILE);
        assert!(starts.windows(2).all(|pair| pair[1] - pair[0] <= TILE - OVERLAP));
    }

    #[test]
    fn a_frame_comes_back_twice_the_size() {
        if !is_installed() {
            println!("the Super Resolution model is not installed here");
            return;
        }
        let frame = Frame::<u8>::from_pixel(600, 520, image::Rgb([120, 120, 120]));
        let doubled = double(&frame, |_, _| true).unwrap().unwrap();
        assert_eq!(doubled.dimensions(), (1200, 1040));
        let centre = doubled.get_pixel(600, 520).0;
        assert!(centre.iter().all(|value| value.abs_diff(120) <= 3), "{centre:?}");
    }
}
