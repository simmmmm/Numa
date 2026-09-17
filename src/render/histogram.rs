use image::RgbImage;

pub const BINS: usize = 256;

const CLIP_FRACTION: f32 = 0.001;

#[derive(Debug, Clone)]
pub struct Histogram {

    pub channels: [[u32; BINS]; 3],

    pub shadow_clipped: u32,
    pub highlight_clipped: u32,
    pub total: u32,
}

impl Histogram {
    pub fn is_shadow_clipped(&self) -> bool {
        self.total > 0 && self.shadow_clipped as f32 > self.total as f32 * CLIP_FRACTION
    }

    pub fn is_highlight_clipped(&self) -> bool {
        self.total > 0 && self.highlight_clipped as f32 > self.total as f32 * CLIP_FRACTION
    }

    pub fn scale(&self) -> u32 {
        let mut occupied: Vec<u32> = self
            .channels
            .iter()
            .flatten()
            .copied()
            .filter(|count| *count > 0)
            .collect();

        if occupied.is_empty() {
            return 1;
        }

        occupied.sort_unstable();
        let index = (occupied.len() as f32 * 0.99) as usize;
        occupied[index.min(occupied.len() - 1)].max(1)
    }
}

pub fn of(image: &RgbImage) -> Histogram {
    const STEP: usize = 4;

    let mut histogram = Histogram {
        channels: [[0; BINS]; 3],
        shadow_clipped: 0,
        highlight_clipped: 0,
        total: 0,
    };

    for pixel in image.pixels().step_by(STEP) {
        for channel in 0..3 {
            histogram.channels[channel][pixel[channel] as usize] += 1;
        }

        if pixel.0.iter().any(|value| *value == 0) {
            histogram.shadow_clipped += 1;
        }
        if pixel.0.iter().any(|value| *value == 255) {
            histogram.highlight_clipped += 1;
        }
        histogram.total += 1;
    }

    histogram
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClippingOverlay {
    pub shadows: bool,
    pub highlights: bool,
}

impl ClippingOverlay {
    pub fn is_off(&self) -> bool {
        !self.shadows && !self.highlights
    }
}

pub fn mark_clipping(image: &mut RgbImage, overlay: ClippingOverlay) {
    if overlay.is_off() {
        return;
    }

    for pixel in image.pixels_mut() {
        if overlay.highlights && pixel.0.iter().any(|value| *value == 255) {
            pixel.0 = [255, 40, 40];
        } else if overlay.shadows && pixel.0.iter().any(|value| *value == 0) {
            pixel.0 = [40, 90, 255];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image_of(pixels: &[[u8; 3]]) -> RgbImage {
        RgbImage::from_fn(pixels.len() as u32, 1, |x, _| image::Rgb(pixels[x as usize]))
    }

    fn repeated(pixel: [u8; 3], count: usize) -> Vec<[u8; 3]> {
        vec![pixel; count * 4]
    }

    #[test]
    fn counts_each_channel_separately() {
        let histogram = of(&image_of(&repeated([10, 200, 10], 8)));

        assert_eq!(histogram.total, 8);
        assert_eq!(histogram.channels[0][10], 8);
        assert_eq!(histogram.channels[1][200], 8);
        assert_eq!(histogram.channels[2][10], 8);
        assert_eq!(histogram.channels[1][10], 0, "green is not red");
    }

    #[test]
    fn clipping_needs_more_than_a_stray_pixel() {

        let mut pixels = repeated([128, 128, 128], 2000);
        pixels[0] = [255, 255, 255];
        let histogram = of(&image_of(&pixels));
        assert!(!histogram.is_highlight_clipped(), "one pixel must not raise the alarm");

        let mut pixels = repeated([128, 128, 128], 1000);
        for pixel in pixels.iter_mut().take(400) {
            *pixel = [255, 255, 255];
        }
        let histogram = of(&image_of(&pixels));
        assert!(histogram.is_highlight_clipped());
        assert!(!histogram.is_shadow_clipped());
    }

    #[test]
    fn a_single_channel_clipping_counts() {

        let histogram = of(&image_of(&repeated([255, 100, 100], 100)));
        assert!(histogram.is_highlight_clipped());

        let histogram = of(&image_of(&repeated([0, 100, 100], 100)));
        assert!(histogram.is_shadow_clipped());
    }

    #[test]
    fn the_scale_survives_a_spike() {

        let mut pixels = Vec::new();
        for value in 0..=200u8 {
            pixels.extend(repeated([value, value, value], 4));
        }
        pixels.extend(repeated([255, 255, 255], 5000));

        let histogram = of(&image_of(&pixels));
        let peak = histogram.channels[0].iter().copied().max().unwrap();

        assert!(
            histogram.scale() < peak / 4,
            "scale {} is dominated by the spike {peak}",
            histogram.scale()
        );
        assert!(histogram.scale() > 0);
    }

    #[test]
    fn an_empty_histogram_has_a_usable_scale() {

        let empty = Histogram {
            channels: [[0; BINS]; 3],
            shadow_clipped: 0,
            highlight_clipped: 0,
            total: 0,
        };
        assert_eq!(empty.scale(), 1);
        assert!(!empty.is_shadow_clipped() && !empty.is_highlight_clipped());
    }

    #[test]
    fn the_overlay_marks_only_what_clips() {
        let mut image = image_of(&[[255, 10, 10], [0, 10, 10], [128, 128, 128]]);
        mark_clipping(
            &mut image,
            ClippingOverlay { shadows: true, highlights: true },
        );

        assert_eq!(image.get_pixel(0, 0).0, [255, 40, 40], "blown");
        assert_eq!(image.get_pixel(1, 0).0, [40, 90, 255], "crushed");
        assert_eq!(image.get_pixel(2, 0).0, [128, 128, 128], "untouched");

        let mut image = image_of(&[[255, 255, 255], [0, 0, 0]]);
        let before = image.clone();
        mark_clipping(&mut image, ClippingOverlay::default());
        assert_eq!(image, before);
    }
}
