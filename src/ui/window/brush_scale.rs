use super::{brush_size, brush_travel, BRUSH_LARGEST, BRUSH_SMALLEST, DEFAULT_BRUSH};

#[test]
fn the_small_end_of_the_brush_has_the_travel() {
    assert!((brush_size(0.0) - BRUSH_SMALLEST).abs() < 1e-6);
    assert!((brush_size(1.0) - BRUSH_LARGEST).abs() < 1e-6);

    assert!((brush_size(brush_travel(DEFAULT_BRUSH)) - DEFAULT_BRUSH).abs() < 1e-5);

    assert!(brush_size(0.5) < 0.07, "{}", brush_size(0.5));

    assert!(brush_size(0.1) < 0.005, "{}", brush_size(0.1));
}
