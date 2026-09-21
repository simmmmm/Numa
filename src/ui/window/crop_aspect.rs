use super::hold_aspect;

#[test]
fn a_held_crop_keeps_its_shape() {

    let held = hold_aspect([0.1, 0.1, 0.5, 0.3], 3, 1.0);
    assert!((held[2] - held[3]).abs() < 1e-5, "square: {held:?}");
    assert!((held[0] - 0.1).abs() < 1e-5 && (held[1] - 0.1).abs() < 1e-5, "anchor held");

    let held = hold_aspect([0.2, 0.2, 0.4, 0.4], 0, 2.0);
    assert!((held[0] + held[2] - 0.6).abs() < 1e-5, "right edge held: {held:?}");
    assert!((held[1] + held[3] - 0.6).abs() < 1e-5, "bottom edge held: {held:?}");
    assert!((held[2] / held[3] - 2.0).abs() < 1e-4, "twice as wide: {held:?}");

    let held = hold_aspect([0.0, 0.0, 0.9, 0.9], 3, 3.0);
    assert!(held[0] + held[2] <= 1.0 + 1e-5 && held[1] + held[3] <= 1.0 + 1e-5, "{held:?}");
    assert!((held[2] / held[3] - 3.0).abs() < 1e-4, "still three to one: {held:?}");
}
