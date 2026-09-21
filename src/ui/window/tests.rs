use super::*;

#[test]
fn a_menu_entry_starts_the_appimage_it_was_written_for() {
    let entry = menu_entry_for(
        std::path::Path::new("/home/a b/Apps/Numa $1.AppImage"),
        std::path::Path::new("/home/a b/.local/share/icons/hicolor/256x256/apps/com.tijmen.Numa.png"),
    );
    assert!(entry.contains("Exec=\"/home/a b/Apps/Numa \\$1.AppImage\" %F\n"), "{entry}");
    assert!(entry.contains("TryExec=/home/a b/Apps/Numa $1.AppImage\n"));
    assert!(entry.contains("Icon=/home/a b/.local/share/icons/hicolor/256x256/apps/com.tijmen.Numa.png\n"));
    assert!(entry.contains("X-Numa-AppImage=true"));
    assert!(!entry.contains("Exec=numa"));
}

#[test]
fn a_restored_snapshot_is_the_snapshot() {
    use numa::core::grading::Grading;
    use numa::core::mixer::Mixer;
    use numa::core::space::ColourSpace;

    let mut edited = Document::new("a.RAF".into());
    edited.set_basic(Basic::with(|b| { b.tone.exposure = 1.25; b.tone.contrast = -18.0; b.tone.whites = 40.0; b.detail.defringe = 30.0; b.detail.moire = 15.0 }));
    edited.white_balance = Some(WhiteBalance { temperature: 7100.0, tint: 6.0 });
    edited.colour_profile = Some("Adobe Standard".into());
    edited.working_space = ColourSpace::AdobeRgb;
    edited.ai_denoise = 60.0;
    edited.set_crop([0.1, 0.2, 0.6, 0.5], 2.5);
    edited.set_rotation(90.0);
    edited.set_perspective(Perspective { vertical: -22.0, horizontal: 8.0, aspect: 4.0 });
    edited.set_curve(Curve::new([[0.0, 0.0], [0.4, 0.62], [1.0, 1.0]]));
    edited.set_mixer(Mixer::default());
    edited.set_point_colours(PointColours {
        points: vec![PointColour { hue: 40.0, ..Default::default() }],
        highlight: None,
    });
    edited.set_grading(Grading::default());
    edited.set_beautify(Beautify { spots: 40.0, skin: 25.0, ..Default::default() });
    edited.set_masks(vec![Mask::new(Shape::radial())]);

    let snapshot = EditState::of(&edited);

    let mut other = Document::new("a.RAF".into());
    other.set_basic(Basic::with(|b| { b.tone.exposure = -2.0; b.presence.saturation = 55.0 }));
    other.working_space = ColourSpace::ProPhoto;
    other.set_crop([0.0, 0.0, 0.3, 0.3], -7.0);

    snapshot.restore(&mut other);
    assert_eq!(
        EditState::of(&other),
        snapshot,
        "a field of the snapshot is not being put back"
    );

    assert_eq!(other.basic().tone.exposure, 1.25);
    assert_eq!(other.basic().tone.contrast, -18.0);
    assert_eq!(other.working_space, ColourSpace::AdobeRgb);
}

#[test]
fn the_curve_editor_stays_inside_its_box() {
    const PAD: usize = 24;
    let (width, height) = (240.0f64, 212.0f64);
    let (full_width, full_height) =
        (width as usize + PAD * 2, height as usize + PAD * 2);

    let mut surface = gtk::cairo::ImageSurface::create(
        gtk::cairo::Format::ARgb32,
        full_width as i32,
        full_height as i32,
    )
    .unwrap();

    {
        let context = gtk::cairo::Context::new(&surface).unwrap();
        context.translate(PAD as f64, PAD as f64);

        let curve = Curve::new([
            [0.0, 0.0],
            [0.13, 0.08],
            [0.5, 1.0],
            [0.72, 0.0],
            [1.0, 1.0],
        ]);
        let curves = [curve.clone(), curve.clone(), curve.clone(), curve];
        draw_curve(&context, width, height, Some(&curves), 1, None);
    }

    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let mut outside = Vec::new();
    for y in 0..full_height {
        for x in 0..full_width {
            let inside = x >= PAD
                && y >= PAD
                && x < PAD + width as usize
                && y < PAD + height as usize;
            if inside {
                continue;
            }

            if data[y * stride + x * 4 + 3] != 0 {
                outside.push((x as i32 - PAD as i32, y as i32 - PAD as i32));
            }
        }
    }

    assert!(
        outside.is_empty(),
        "{} pixels drawn outside the box, first at {:?}",
        outside.len(),
        &outside[..outside.len().min(4)]
    );
}

#[test]
fn a_history_is_resumed_not_restarted() {
    let untouched = Document::new("x".into());
    let mut cropped = untouched.clone();
    cropped.set_crop([0.1, 0.1, 0.8, 0.8], 0.0);
    let mut brighter = cropped.clone();
    brighter.set_basic(Basic::with(|b| b.tone.exposure = 1.0));
    let (untouched, cropped, brighter) = (EditState::of(&untouched), EditState::of(&cropped), EditState::of(&brighter));

    let mut fresh = History::resumed(None, cropped.clone());
    assert_eq!(fresh.steps(), ["Original", "Crop"]);
    assert_eq!(fresh.undo(), Some(untouched.clone()), "the crop can be taken back");

    let saved = Some((vec![untouched.clone(), cropped.clone(), brighter.clone()], 1));
    let resumed = History::resumed(saved.clone(), cropped.clone());
    assert_eq!((resumed.states.len(), resumed.position), (3, 1));

    let pasted = History::resumed(Some((vec![untouched.clone(), cropped.clone()], 1)), brighter.clone());
    assert_eq!(pasted.steps(), ["Original", "Crop", "Exposure"]);

    assert_eq!(History::resumed(None, untouched.clone()).steps(), ["Original"]);

    let mut long = History::new(untouched);
    for step in 0..HISTORY_KEPT + 20 {
        let mut document = Document::new("x".into());
        document.set_basic(Basic::with(|b| b.tone.exposure = step as f32 / 100.0 + 0.01));
        long.push(EditState::of(&document));
    }
    assert_eq!((long.states.len(), long.position), (HISTORY_KEPT, HISTORY_KEPT - 1));
}

#[test]
fn a_snapshot_put_back_is_one_named_step() {
    let mut saved = Document::new("x".into());
    saved.set_crop([0.1, 0.1, 0.8, 0.8], 0.0);
    saved.set_basic(Basic::with(|b| b.tone.exposure = 0.7));
    let json = serde_json::to_string(&saved).unwrap();

    let mut document = Document::new("x".into());
    document.set_basic(Basic::with(|b| b.tone.contrast = 30.0));
    let before = EditState::of(&document);
    let mut history = History::resumed(None, before.clone());

    let snapshot: Document = serde_json::from_str(&json).unwrap();
    history.push_named(EditState::of(&snapshot), "Before the sky");
    history.states[history.position].restore(&mut document);
    assert_eq!(EditState::of(&document), EditState::of(&saved));
    assert_eq!(history.steps(), ["Original", "Contrast", "Before the sky"]);

    history.push_named(EditState::of(&snapshot), "Before the sky");
    assert_eq!(history.states.len(), 3);
    assert_eq!(history.undo(), Some(before));

    history.push(EditState::untouched());
    assert_eq!(history.steps(), ["Original", "Contrast", "Contrast"]);
}

#[test]
fn choosing_a_camera_profile_is_something_to_step_back_out_of() {
    let mut document = Document::new("x".into());
    let before = EditState::of(&document);

    document.colour_profile = Some("Adobe Standard".to_string());
    let after = EditState::of(&document);
    assert_ne!(before, after, "the snapshot did not notice the profile");

    let mut history = History::new(before.clone());
    history.push(after.clone());
    assert_eq!(history.undo(), Some(before));
    assert_eq!(history.redo(), Some(after));
}

#[test]
fn the_marks_line_up_with_the_sliders_they_describe() {
    let as_shot = WhiteBalance { temperature: 5500.0, tint: 0.0 };
    let rest = at_rest_of(Basic::default(), as_shot, as_shot);
    assert!(rest.iter().all(|at| *at), "an untouched photograph has a mark on it");

    let moved = at_rest_of(
        Basic::with(|b| { b.tone.exposure = 0.4; b.presence.hdr = 20.0 }),
        as_shot,
        as_shot,
    );
    let marked: Vec<usize> =
        moved.iter().enumerate().filter(|(_, at)| !**at).map(|(index, _)| index).collect();
    assert_eq!(marked, vec![2, 8], "the wrong sliders would have been coloured");

    let warmed = WhiteBalance { temperature: 7000.0, tint: 0.0 };
    assert!(!at_rest_of(Basic::default(), warmed, as_shot)[0]);
    assert!(at_rest_of(Basic::default(), as_shot, as_shot)[0]);
}

#[test]
fn a_mask_reads_its_temperature_as_a_shift() {
    assert_eq!(Readout::OffsetKelvin.format(0.0), "0 K");
    assert_eq!(Readout::OffsetKelvin.format(-800.0), "\u{2212}800 K");

    assert_eq!(Readout::OffsetKelvin.format(1500.0), "+1\u{2009}500 K");

    assert_eq!(Readout::Kelvin.format(5300.0), "5\u{2009}300 K");
}

#[test]
fn dragging_a_mask_moves_what_was_grabbed() {
    let linear = Shape::Linear { from: [0.2, 0.8], to: [0.2, 0.2] };

    assert!(nearest_mask_handle(&linear, 0.21, 0.79) == Handle::Start);
    assert!(nearest_mask_handle(&linear, 0.2, 0.21) == Handle::End);
    assert!(nearest_mask_handle(&linear, 0.9, 0.5) == Handle::Whole);

    let moved = move_mask_handle(linear.clone(), Handle::End, [0.3, 0.1], [0.5, 0.3]);
    assert_eq!(moved, Shape::Linear { from: [0.2, 0.8], to: [0.5, 0.3] });

    let Shape::Linear { from, to } = move_mask_handle(linear, Handle::Whole, [0.1, -0.2], [0.0, 0.0])
    else {
        panic!("a linear gradient stopped being one");
    };
    assert!((from[0] - 0.3).abs() < 1e-6 && (to[0] - 0.3).abs() < 1e-6);
    assert!((from[1] - 0.6).abs() < 1e-6 && (to[1] - 0.0).abs() < 1e-6);
}

#[test]
fn a_radial_resizes_from_its_edge_and_never_inverts() {
    let radial = Shape::Radial { centre: [0.5, 0.5], radius: [0.2, 0.3], feather: 0.4 };

    assert!(nearest_mask_handle(&radial, 0.5, 0.5) == Handle::Centre);
    assert!(nearest_mask_handle(&radial, 0.7, 0.5) == Handle::EdgeX);
    assert!(nearest_mask_handle(&radial, 0.5, 0.8) == Handle::EdgeY);

    let Shape::Radial { radius, feather, .. } =
        move_mask_handle(radial.clone(), Handle::EdgeX, [0.0, 0.0], [0.85, 0.5])
    else {
        panic!("a radial stopped being one");
    };
    assert!((radius[0] - 0.35).abs() < 1e-6);
    assert_eq!(radius[1], 0.3, "the other axis moved");
    assert_eq!(feather, 0.4);

    let Shape::Radial { radius, .. } =
        move_mask_handle(radial, Handle::EdgeX, [0.0, 0.0], [0.1, 0.5])
    else {
        panic!("a radial stopped being one");
    };
    assert!(radius[0] > 0.0, "the radius went through zero: {radius:?}");
}

#[test]
fn the_corners_of_the_graph_are_the_corners_of_the_plot() {
    let (width, height) = (240.0, 212.0);
    let (left, top, plot_width, plot_height) = plot_rect(width, height);

    assert!(left > 0.0 && top > 0.0, "the plot must be inset");
    assert!((left + plot_width) < width && (top + plot_height) < height);

    assert!(left >= HANDLE && top >= HANDLE);

    let (_, _, tiny_width, tiny_height) = plot_rect(4.0, 4.0);
    assert!(tiny_width > 0.0 && tiny_height > 0.0);
}
