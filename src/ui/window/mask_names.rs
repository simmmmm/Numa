use numa::core::mask::{Mask, Shape};

fn radial(name: Option<&str>) -> Mask {
    let mut mask = Mask::new(Shape::radial());
    mask.name = name.map(str::to_string);
    mask
}

#[test]
fn numbered_only_when_shared() {
    let one = [radial(Some("Bird"))];
    assert_eq!(super::mask_label(&one, 0), "Bird");

    let two = [radial(Some("Bird")), radial(Some("Sky"))];
    assert_eq!(super::mask_label(&two, 0), "Bird");
    assert_eq!(super::mask_label(&two, 1), "Sky");

    let three = [radial(Some("Bird")), radial(Some("Sky")), radial(Some("Bird"))];
    assert_eq!(super::mask_label(&three, 0), "Bird 1");
    assert_eq!(super::mask_label(&three, 1), "Sky");
    assert_eq!(super::mask_label(&three, 2), "Bird 2");

    let plain = [radial(None), radial(None)];
    assert_eq!(super::mask_label(&plain, 0), "Radial 1");
    assert_eq!(super::mask_label(&plain, 1), "Radial 2");
}
