use super::{hold_aspect, turned_rect};

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

#[test]
fn a_turned_crop_turns_with_the_photograph() {
    let rect = [0.0, 0.0, 0.5, 1.0];
    assert_eq!(turned_rect(rect), [0.0, 0.0, 1.0, 0.5]);
    let rect = [0.1, 0.2, 0.3, 0.4];
    let turned = turned_rect(rect);
    assert!((turned[0] - 0.4).abs() < 1e-6 && (turned[1] - 0.1).abs() < 1e-6, "{turned:?}");

    let (before, after) = (rect[2] * 3000.0 / (rect[3] * 2000.0), turned[2] * 2000.0 / (turned[3] * 3000.0));
    assert!((before * after - 1.0).abs() < 1e-5, "the ratio turns: {before} then {after}");
    let back = (0..4).fold(rect, |rect, _| turned_rect(rect));
    assert!(back.iter().zip(rect).all(|(a, b)| (a - b).abs() < 1e-6), "{back:?}");
}
