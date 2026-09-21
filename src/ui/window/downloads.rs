use super::*;

type ModelFile = (&'static str, &'static str, u64, &'static str);

const MODELS: [(&str, &str, &[ModelFile]); 12] = [
    ("EfficientViT-Seg B2", "Sky, greenery and the other found masks · Apache-2.0 · 61 MB", &[(
        "efficientvit_seg_b2_ade20k_1024.onnx",
        "https://github.com/simmmmm/Numa/releases/download/models/efficientvit_seg_b2_ade20k_1024.onnx",
        61_277_554, "39f11050777fe5562292ca2bfbac344515da28128d4330c6d8c514ca5d5e6efa",
    )]),
    ("SlimSAM", "Click to select · Apache-2.0 · 40 MB", &[
        ("sam_encoder.onnx", "https://huggingface.co/Xenova/slimsam-77-uniform/resolve/main/onnx/vision_encoder.onnx", 23_276_014, "9f8433273a6750b587779baa0cf5508111001bf7e7acfcf585d370139fd366d0"),
        ("sam_decoder.onnx", "https://huggingface.co/Xenova/slimsam-77-uniform/resolve/main/onnx/prompt_encoder_mask_decoder.onnx", 16_557_892, "f4514391764fbd56e08e119060d874ecd7d52994bfb1968af159e12d4943b5bb"),
    ]),
    ("IS-Net", "Subject edges and Refine edge · Apache-2.0 · 179 MB", &[(
        "isnet.onnx",
        "https://github.com/danielgatis/rembg/releases/download/v0.0.0/isnet-general-use.onnx",
        178_648_008, "60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a",
    )]),

    ("ViTMatte-S", "Hair and fur in Refine edge · Apache-2.0 · 104 MB", &[(
        "vitmatte_small.onnx",
        "https://huggingface.co/Xenova/vitmatte-small-composition-1k/resolve/main/onnx/model.onnx",
        103_885_865, "bf28d2e0be2c073286e88d60ad649d7123da2749a2d99133fd1098d5887e0225",
    )]),
    ("YuNet", "Finding faces · MIT · 0.2 MB", &[(
        "face_detection_yunet_2023mar.onnx",
        "https://media.githubusercontent.com/media/opencv/opencv_zoo/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx",
        232_589, "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4",
    )]),
    ("SFace", "Recognising people · Apache-2.0 · 39 MB", &[(
        "face_recognition_sface_2021dec.onnx",
        "https://media.githubusercontent.com/media/opencv/opencv_zoo/main/models/face_recognition_sface/face_recognition_sface_2021dec.onnx",
        38_696_353, "0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79",
    )]),
    ("PP-ResNet50", "Naming the animal in a subject mask · Apache-2.0 · 103 MB", &[(
        "image_classification_ppresnet50_2022jan.onnx",
        "https://media.githubusercontent.com/media/opencv/opencv_zoo/main/models/image_classification_ppresnet/image_classification_ppresnet50_2022jan.onnx",
        102_567_035, "ad5486b0de6c2171ea4d28c734c2fb7c5f64fcdbd97180a0ef515cf4b766a405",
    )]),

    ("LaMa", "Remove: what is under a spot, filled from around it · Apache-2.0 · 208 MB", &[(
        "lama_fp32.onnx",
        "https://huggingface.co/Carve/LaMa-ONNX/resolve/main/lama_fp32.onnx",
        208_044_816, "1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6",
    )]),

    ("YOLOX-s", "Remove people: finding the passers-by · Apache-2.0 · 36 MB", &[(
        "yolox_s.onnx",
        "https://huggingface.co/Heliosoph/yolox-onnx/resolve/main/yolox_s.onnx",
        35_858_002, "c5c2d13e59ae883e6af3b45daea64af4833a4951c92d116ec270d9ddbe998063",
    )]),

    ("Restormer", "AI sharpen: undoing a hand that moved · MIT · 107 MB", &[(
        "restormer_motion_deblurring.onnx",
        "https://github.com/simmmmm/Numa/releases/download/models/restormer_motion_deblurring.onnx",
        107_114_300, "cbaeb199a2a1f2b3d2008cef37cbe411ff9e388bb0700065ff8af25de1545001",
    )]),

    ("RealPLKSR", "Super Resolution: export at twice the size · MIT · 30 MB", &[(
        "realplksr_x2.onnx",
        "https://huggingface.co/darktable-org/upscale-realplksr-onnx/resolve/main/onnx/model_x2.onnx",
        29_627_920, "d7abb65092f3808d3aa255ffdab42b1d672883915d90adbdd321204168b9f293",
    )]),

    ("SCUNet", "AI denoise · Apache-2.0 · 77 MB", &[
        ("scunet_color_real_psnr.onnx", "https://huggingface.co/Heliosoph/scunet-onnx/resolve/main/scunet_color_real_psnr.onnx", 3_798_678, "231be201ab413dbc999d7951caa9844846b93a12a40a41e037d6b5888ed4e88c"),
        ("scunet_color_real_psnr.onnx.data", "https://huggingface.co/Heliosoph/scunet-onnx/resolve/main/scunet_color_real_psnr.onnx.data", 73_138_176, "98825ea1210b641c71e5f052f582c70c49fd44b35387ebe2c034268c17df3feb"),
    ]),
];

const MIRROR: &str = "https://github.com/simmmmm/Numa/releases/download/models";

const PROFILES_ARCHIVE: ModelFile = (
    "rawtherapee-dcpprofiles-5.13.tar.gz",
    "https://github.com/simmmmm/Numa/releases/download/models/rawtherapee-dcpprofiles-5.13.tar.gz",
    67_267_725, "482c0f664fea223028be5291a6ad1577e58d555e49f185f19f4ffacedb8f5c50",
);

const GPU_PLUGIN_WHEEL: [ModelFile; 2] = [(
    "onnxruntime_ep_webgpu-0.3.0-py3-none-manylinux_2_28_x86_64.whl",
    "https://files.pythonhosted.org/packages/97/9c/d37bc05c56c3d91d44585db7bebbf0f068ece5d01df5b3898449771d4bf2/onnxruntime_ep_webgpu-0.3.0-py3-none-manylinux_2_28_x86_64.whl",
    6_599_236, "865ce82d80319d7f259a4a65e66e32834f0f117db55ae4377868b4f28016e7bf",
), (

    "scunet_color_real_psnr_fp16.onnx",
    "https://github.com/simmmmm/Numa/releases/download/models/scunet_color_real_psnr_fp16.onnx",
    40_325_163, "83bd516b28c9d6a3fb733d33fc81c840407764d82d273d43b98903bd21e231cc",
)];

const GPU_ENABLED: &str = "gpu-acceleration";

const GPU_GUARD: &str = "gpu-attempt";

fn download_dir(file: &str) -> PathBuf {
    if file.ends_with(".tar.gz") { numa::core::paths::data_dir() } else { numa::core::paths::models_dir() }
}

fn model_on_disk((file, url, _, _): &ModelFile) -> Option<PathBuf> {
    if file.ends_with(".tar.gz") {
        let unpacked = dcp::downloaded_profiles_dir();
        return unpacked.is_dir().then_some(unpacked);
    }
    if file.ends_with(".whl") {
        return numa::infer::gpu_plugin(&numa::core::paths::models_dir());
    }
    let linked = url.rsplit('/').next().unwrap_or(file);
    numa::core::paths::model_file(&[file, linked])
}

fn unpack_wheel(wheel: &Path, dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(wheel).map_err(|err| err.to_string())?;
    let mut archive =
        zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|err| err.to_string())?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|err| err.to_string())?;

        let name = entry.enclosed_name().and_then(|path| Some(path.file_name()?.to_owned()));
        let keep = match name.as_deref().and_then(|name| name.to_str()) {
            Some(name) if name == numa::infer::GPU_PLUGIN => name.to_string(),
            Some("LICENSE") => "onnxruntime_providers_webgpu.LICENSE.txt".to_string(),
            Some("ThirdPartyNotices.txt") => {
                "onnxruntime_providers_webgpu.ThirdPartyNotices.txt".to_string()
            }
            _ => continue,
        };
        let mut out = std::fs::File::create(dir.join(keep)).map_err(|err| err.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|err| err.to_string())?;
    }

    match dir.join(numa::infer::GPU_PLUGIN).is_file() {
        true => Ok(()),
        false => Err(format!("no {} inside the wheel", numa::infer::GPU_PLUGIN)),
    }
}

fn profiles_available() -> bool {
    let own = dcp::profiles_dir();
    dcp::search_paths().into_iter().filter(|dir| Some(dir) != own.as_ref()).any(|dir| {
        std::fs::read_dir(dir).is_ok_and(|entries| {
            entries.flatten().any(|entry| entry.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("dcp")))
        })
    })
}

fn megabytes(files: &[ModelFile]) -> u64 {
    (files.iter().map(|(_, _, bytes, _)| bytes).sum::<u64>() + 500_000) / 1_000_000
}

fn files_of(names: &[&str]) -> Vec<ModelFile> {
    MODELS
        .iter()
        .filter(|(name, _, _)| names.contains(name))
        .flat_map(|(_, _, files)| files.iter().copied())
        .collect()
}

pub(super) fn offer_additional_files(state: &App, window: &adw::ApplicationWindow) {
    let any = MODELS.iter().any(|(_, _, files)| files.iter().all(|file| model_on_disk(file).is_some()));
    if any || state.catalog.recall::<bool>(OFFERED_MODELS).unwrap_or(false) {
        return;
    }
    state.catalog.remember(OFFERED_MODELS, &true);

    let missing = missing_model_files();

    let dialog = adw::Dialog::new();
    dialog.set_content_width(560);
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_show_title(false);
    view.add_top_bar(&header);

    let column = gtk::Box::new(gtk::Orientation::Vertical, 18);
    column.set_margin_start(24);
    column.set_margin_end(24);
    column.set_margin_bottom(24);

    let icon = gtk::Image::from_icon_name("folder-download-symbolic");
    icon.set_pixel_size(64);
    icon.add_css_class("dim-label");
    column.append(&icon);
    let title = gtk::Label::new(Some("Additional files"));
    title.add_css_class("title-1");
    column.append(&title);
    let intro = gtk::Label::new(Some(
        "A few features need extra files that are not part of the download. \
         Everything else in Numa works without them.",
    ));
    intro.set_wrap(true);
    intro.set_justify(gtk::Justification::Center);
    column.append(&intro);

    let features = adw::PreferencesGroup::new();
    let mut rows = vec![
        ("weather-few-clouds-symbolic", "Masks that find things for you", "The sky, greenery, buildings, water, people", files_of(&["EfficientViT-Seg B2"])),
        ("input-mouse-symbolic", "Select anything with a click", "Including things no preset has a name for", files_of(&["SlimSAM"])),
        ("edit-cut-symbolic", "Clean edges", "Hair, fur and feathers in a subject mask", files_of(&["IS-Net", "ViTMatte-S"])),
        ("system-users-symbolic", "Faces and people", "Face retouching, and browsing by person", files_of(&["YuNet", "SFace"])),
        ("emoji-nature-symbolic", "Animals by name", "A subject mask that says Bird or Dog", files_of(&["PP-ResNet50"])),
        ("image-x-generic-symbolic", "AI denoise", "Clean high-ISO photographs, kept once worked out", files_of(&["SCUNet"])),
        ("edit-clear-symbolic", "Remove", "Take out a sign, a wire or a stranger, filled from around it", files_of(&["LaMa", "YOLOX-s"])),
        ("image-x-generic-symbolic", "AI sharpen", "Undo the blur of a hand that moved, kept once worked out", files_of(&["Restormer"])),
        ("zoom-in-symbolic", "Super Resolution", "Export at twice the size, with detail rather than blur", files_of(&["RealPLKSR"])),
    ];

    if missing.contains(&PROFILES_ARCHIVE) {
        rows.push(("camera-photo-symbolic", "Colour for your camera", "Camera profiles for 161 cameras, from RawTherapee", vec![PROFILES_ARCHIVE]));
    }
    for (icon, title, subtitle, files) in rows {
        let row = adw::ActionRow::new();
        row.set_title(title);
        row.set_subtitle(subtitle);
        row.add_prefix(&gtk::Image::from_icon_name(icon));
        let size = gtk::Label::new(Some(&format!("{} MB", megabytes(&files).max(1))));
        size.add_css_class("dim-label");
        size.add_css_class("numeric");
        row.add_suffix(&size);
        features.add(&row);
    }

    let models = adw::ExpanderRow::new();
    models.set_title("What is downloaded");
    models.set_subtitle("Machine-learning models that run on this computer, and camera profiles");
    models.add_prefix(&gtk::Image::from_icon_name("dialog-information-symbolic"));
    for (name, what, _) in MODELS {
        let row = adw::ActionRow::new();
        row.set_title(name);
        row.set_subtitle(what);
        models.add_row(&row);
    }
    let profiles = adw::ActionRow::new();
    profiles.set_title("RawTherapee camera profiles");
    profiles.set_subtitle("Colour for your camera · GPL-3.0 · 67 MB");
    models.add_row(&profiles);
    features.add(&models);
    column.append(&features);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    buttons.set_halign(gtk::Align::Center);
    let later = gtk::Button::with_label("Not Now");
    later.add_css_class("pill");
    let download = gtk::Button::with_label(&format!("Download {} MB", megabytes(&missing)));
    download.add_css_class("pill");
    download.add_css_class("suggested-action");
    buttons.append(&later);
    buttons.append(&download);
    column.append(&buttons);

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_propagate_natural_height(true);
    scroller.set_child(Some(&column));
    view.set_content(Some(&scroller));
    dialog.set_child(Some(&view));

    later.connect_clicked(glib::clone!(
        #[weak] dialog,
        move |_| {
            dialog.close();
        }
    ));
    download.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] dialog,
        move |_| {
            dialog.close();

            let cancel = Cancel::default();
            let (toast, text, bar) = progress_toast(&state, &cancel);
            let progress = move |got: u64, total: u64| {
                text.set_text(&format!(
                    "Downloading additional files — {} of {} MB",
                    got / 1_000_000,
                    total / 1_000_000
                ));
                bar.set_fraction(got as f64 / total.max(1) as f64);
            };
            let state = state.clone();
            download_model(missing.clone(), progress, cancel, move |result| {
                toast.dismiss();
                match result {
                    Ok(()) => state.toast("Additional files installed — restart Numa to use them"),
                    Err(err) if err == "stopped" => {
                        state.toast("Download stopped — continue any time from Preferences")
                    }
                    Err(err) => {
                        log::warn!("downloading the additional files: {err}");
                        state.toast("Some files could not be downloaded — Preferences shows which")
                    }
                }
            });
        }
    ));

    glib::idle_add_local_once(glib::clone!(
        #[weak] window,
        move || dialog.present(Some(&window))
    ));
}

fn download_model(
    files: Vec<ModelFile>,
    progress: impl Fn(u64, u64) + 'static,
    cancel: Cancel,
    done: impl FnOnce(Result<(), String>) + 'static,
) {
    let total: u64 = files.iter().map(|(_, _, bytes, _)| bytes).sum();
    let running: Rc<RefCell<Option<gio::Subprocess>>> = Rc::default();
    let ticker = {
        let (files, running, cancel) = (files.clone(), running.clone(), cancel.clone());
        glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
            if cancel.stopped() {
                if let Some(process) = running.borrow().as_ref() {
                    process.force_exit();
                }
            }
            let got: u64 = files
                .iter()
                .map(|entry @ (file, _, bytes, _)| {
                    if model_on_disk(entry).is_some() {
                        return *bytes;
                    }
                    std::fs::metadata(download_dir(file).join(format!("{file}.part"))).map_or(0, |meta| meta.len())
                })
                .sum();
            progress(got.min(total), total);
            glib::ControlFlow::Continue
        })
    };

    glib::spawn_future_local(async move {
        let result = async {
            let mut failed = Vec::new();
            for entry @ (file, url, _, sha256) in &files {
                if model_on_disk(entry).is_some() {
                    continue;
                }
                let dir = download_dir(file);
                std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
                let part = dir.join(format!("{file}.part"));

                let mirror = format!("{MIRROR}/{file}");
                let mut error = String::new();
                let mut fetched = false;
                let urls = if mirror == *url { vec![*url] } else { vec![mirror.as_str(), *url] };
                for url in urls {
                    if cancel.stopped() {
                        return Err("stopped".to_string());
                    }
                    let finished = run(
                        &running,
                        &["curl".as_ref(), "--fail".as_ref(), "--location".as_ref(), "--retry".as_ref(), "3".as_ref(),
                          "--continue-at".as_ref(), "-".as_ref(), "--output".as_ref(), part.as_os_str(), url.as_ref()],
                    )
                    .await;
                    let finished = match finished {
                        Ok(()) => checked(&part, sha256).await,
                        failed => failed,
                    };
                    if cancel.stopped() {
                        return Err("stopped".to_string());
                    }
                    match finished {
                        Ok(()) => {
                            fetched = true;
                            break;
                        }

                        Err(err) => {
                            let _ = std::fs::remove_file(&part);
                            error = err;
                        }
                    }
                }

                if !fetched {
                    failed.push(format!("{file}: {error}"));
                    continue;
                }
                if file.ends_with(".tar.gz") {
                    let unpacked = run(&running, &["tar".as_ref(), "-xzf".as_ref(), part.as_os_str(), "-C".as_ref(), dir.as_os_str()]).await;
                    let _ = std::fs::remove_file(&part);
                    if let Err(err) = unpacked {
                        failed.push(format!("{file}: {err}"));
                    }
                } else if file.ends_with(".whl") {

                    let unpacked = unpack_wheel(&part, &dir);
                    let _ = std::fs::remove_file(&part);
                    if let Err(err) = unpacked {
                        failed.push(format!("{file}: {err}"));
                    }
                } else {
                    std::fs::rename(&part, dir.join(file)).map_err(|err| err.to_string())?;
                }
            }
            if failed.is_empty() { Ok(()) } else { Err(failed.join("; ")) }
        }
        .await;
        ticker.remove();
        done(result);
    });
}

async fn run(running: &Rc<RefCell<Option<gio::Subprocess>>>, argv: &[&std::ffi::OsStr]) -> Result<(), String> {
    let process = gio::Subprocess::newv(argv, gio::SubprocessFlags::STDERR_PIPE)
        .map_err(|_| format!("{} is not installed", argv[0].to_string_lossy()))?;
    running.replace(Some(process.clone()));
    let finished = process.wait_check_future().await;
    running.replace(None);
    finished.map_err(|err| err.message().to_string())
}

async fn checked(path: &Path, sha256: &str) -> Result<(), String> {
    let process = gio::Subprocess::newv(&["sha256sum".as_ref(), path.as_os_str()], gio::SubprocessFlags::STDOUT_PIPE)
        .map_err(|_| "sha256sum is not installed".to_string())?;
    let (out, _) = process.communicate_utf8_future(None).await.map_err(|err| err.message().to_string())?;
    match out.as_deref().and_then(|out| out.split_whitespace().next()) {
        Some(got) if got == sha256 => Ok(()),
        got => Err(format!("checksum mismatch ({})", got.unwrap_or("none"))),
    }
}

fn missing_model_files() -> Vec<ModelFile> {
    let mut missing: Vec<ModelFile> = MODELS
        .iter()
        .flat_map(|(_, _, files)| files.iter().copied())
        .filter(|file| model_on_disk(file).is_none())
        .collect();
    if !profiles_available() {
        missing.push(PROFILES_ARCHIVE);
    }
    missing
}

fn download_row(
    dialog: &adw::PreferencesDialog,
    name: &'static str,
    what: &'static str,
    files: &'static [ModelFile],
) -> (adw::ActionRow, Option<gtk::Button>) {
    let row = adw::ActionRow::new();
    row.set_title(name);
    row.set_subtitle(what);

    let state = gtk::Stack::new();
    state.set_valign(gtk::Align::Center);
    let installed = gtk::Label::new(Some("Installed"));
    let get = gtk::Button::with_label("Download");
    let bar = gtk::ProgressBar::new();
    bar.set_size_request(120, -1);
    bar.set_valign(gtk::Align::Center);
    state.add_named(&installed, Some("installed"));
    state.add_named(&get, Some("get"));
    state.add_named(&bar, Some("busy"));

    let present = files.iter().all(|file| model_on_disk(file).is_some());
    state.set_visible_child_name(if present { "installed" } else { "get" });

    get.connect_clicked(glib::clone!(
        #[weak] dialog,
        #[weak] state,
        #[weak] bar,
        #[weak] row,
        move |_| {
            state.set_visible_child_name("busy");
            let bar = bar.clone();
            let progress = move |got: u64, total: u64| bar.set_fraction(got as f64 / total.max(1) as f64);
            download_model(files.to_vec(), progress, Cancel::default(), move |result| match result {
                Ok(()) => {
                    state.set_visible_child_name("installed");
                    dialog.add_toast(adw::Toast::new(&format!("{name} installed — restart Numa to use it")));
                }
                Err(err) => {
                    log::warn!("downloading {name} failed: {err}");
                    state.set_visible_child_name("get");
                    row.set_subtitle(&format!("Download failed: {err}"));

                    if let Some((_, url, _, _)) = files.iter().find(|file| model_on_disk(file).is_none()) {
                        let link = gtk::Button::from_icon_name("web-browser-symbolic");
                        link.set_tooltip_text(Some("Open the download link in a browser"));
                        link.set_valign(gtk::Align::Center);
                        link.add_css_class("flat");
                        let url = url.to_string();
                        link.connect_clicked(move |button| {
                            let window = button.root().and_downcast::<gtk::Window>();
                            gtk::UriLauncher::new(&url).launch(window.as_ref(), None::<&gio::Cancellable>, |_| {});
                        });
                        row.add_suffix(&link);
                    }
                }
            });
        }
    ));
    row.add_suffix(&state);
    (row, (!present).then_some(get))
}

pub(super) fn models_group(
    state: &App,
    dialog: &adw::PreferencesDialog,
    folder_row: &dyn Fn(&str, PathBuf) -> adw::ActionRow,
) -> adw::PreferencesGroup {
    let models_dir = numa::core::paths::models_dir();
    let models = adw::PreferencesGroup::new();
    models.set_title("Models");

    let total: u64 = MODELS.iter().flat_map(|(_, _, files)| files.iter()).map(|file| file.2).sum();
    models.set_description(Some(&format!(
        "Masks, faces, subject edges and the AI tools use machine-learning models that run \
         on this computer. They are not part of the download: together they are {}, \
         each comes from its own project under its own licence, and not everyone \
         wants every feature. Download them here, or fetch the files yourself and put \
         them in the models folder. Everything else works without them.",
        glib::format_size(total)
    )));
    let everything = gtk::Button::with_label("Download all");
    everything.set_valign(gtk::Align::Center);
    models.set_header_suffix(Some(&everything));
    models.add(&folder_row("Models folder", models_dir));
    let mut downloads: Vec<gtk::Button> = Vec::new();
    for (name, what, files) in MODELS {
        let (row, get) = download_row(&dialog, name, what, files);
        downloads.extend(get);
        models.add(&row);
    }
    everything.set_sensitive(!downloads.is_empty());

    everything.connect_clicked(move |button| {
        button.set_sensitive(false);
        for get in &downloads {
            get.emit_clicked();
        }
    });
    for row in gpu_rows(state, dialog) {
        models.add(&row);
    }
    models
}

fn gpu_rows(state: &App, dialog: &adw::PreferencesDialog) -> Vec<gtk::Widget> {
    let (row, _) = download_row(
        dialog,
        "GPU acceleration",
        "Found masks, click-to-select and AI denoise 3–6× faster · MIT · 47 MB",
        &GPU_PLUGIN_WHEEL,
    );
    let switch = adw::SwitchRow::new();
    switch.set_title("Use GPU acceleration");
    switch.set_subtitle(
        "Off, or where the graphics card will not run a model, the processor does it \
         instead — slower, and the same answer. Takes effect the next time Numa starts.",
    );
    switch.set_sensitive(model_on_disk(&GPU_PLUGIN_WHEEL[0]).is_some());
    switch.set_active(state.catalog.setting(GPU_ENABLED).as_deref() != Some("no"));
    switch.connect_active_notify(glib::clone!(
        #[strong] state,
        move |switch| {
            let _ = state
                .catalog
                .set_setting(GPU_ENABLED, if switch.is_active() { "yes" } else { "no" });
        }
    ));
    vec![row.upcast(), switch.upcast()]
}

thread_local! {

    static SAFE_MODE: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn at_startup(state: &App, window: &adw::ApplicationWindow) {
    if SAFE_MODE.get() {
        state.toast("GPU acceleration is off after a crash — Preferences turns it back on");
    }
    offer_additional_files(state, window);
}

pub(super) fn start_gpu(catalog: &Catalog) {
    let models = numa::core::paths::models_dir();
    if numa::infer::gpu_plugin(&models).is_none() {
        return;
    }
    let guard = numa::core::paths::data_dir().join(GPU_GUARD);
    if guard.exists() {
        let _ = std::fs::remove_file(&guard);
        let _ = catalog.set_setting(GPU_ENABLED, "no");
        SAFE_MODE.set(true);
        return;
    }
    if catalog.setting(GPU_ENABLED).as_deref() == Some("no") {
        return;
    }
    numa::infer::enable_gpu(&models, &guard);
}

pub(super) fn profiles_group(
    dialog: &adw::PreferencesDialog,
    folder_row: &dyn Fn(&str, PathBuf) -> adw::ActionRow,
) -> adw::PreferencesGroup {
    let profiles = adw::PreferencesGroup::new();
    profiles.set_title("Camera profiles");
    profiles.set_description(Some(
        "A camera profile (.dcp) refines the colour of one camera model. Without one, \
         colour comes from the matrix inside the raw file, which is what every raw \
         developer falls back to. RawTherapee's profiles may be shipped and are included \
         where the package allows; Adobe's may not, so profiles from Adobe DNG Converter \
         have to be added here by you. Numa matches a profile on the camera model \
         written inside it, so the file name does not matter.",
    ));
    let (download, _) = download_row(
        &dialog,
        "RawTherapee camera profiles",
        "161 cameras from Canon, Fujifilm, Nikon, Sony and more · GPL-3.0 · 67 MB",
        std::slice::from_ref(&PROFILES_ARCHIVE),
    );
    profiles.add(&download);
    let own = dcp::profiles_dir();
    for dir in dcp::search_paths() {
        let count = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("dcp")))
                    .count()
            })
            .unwrap_or(0);
        let yours = Some(&dir) == own.as_ref();

        if !yours && count == 0 {
            continue;
        }
        let title = if yours { "Your profiles" } else { "Found profiles" };
        let row = folder_row(title, dir);
        let label = gtk::Label::new(Some(&match count {
            1 => "1 profile".to_string(),
            count => format!("{count} profiles"),
        }));
        label.add_css_class("dim-label");
        row.add_suffix(&label);
        profiles.add(&row);
    }
    profiles
}

#[cfg(test)]
mod tests {
    use super::{checked, unpack_wheel};
    use std::io::Write;

    #[test]
    fn a_download_is_the_file_its_digest_names() {
        let path = std::env::temp_dir().join("numa-sha256-test");
        std::fs::write(&path, "abc").unwrap();
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let context = gtk::glib::MainContext::default();
        assert_eq!(context.block_on(checked(&path, abc)), Ok(()));
        std::fs::write(&path, "abd").unwrap();
        assert!(context.block_on(checked(&path, abc)).is_err());
        let _ = std::fs::remove_file(&path);
    }

    fn wheel(dir: &std::path::Path, with_provider: bool) -> std::path::PathBuf {
        let path = dir.join("test.whl");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        let mut entries = vec![
            ("onnxruntime_ep_webgpu/__init__.py", "import os\n"),
            ("onnxruntime_ep_webgpu-0.3.0.dist-info/licenses/LICENSE", "MIT\n"),
            ("onnxruntime_ep_webgpu-0.3.0.dist-info/licenses/ThirdPartyNotices.txt", "notices\n"),
        ];
        if with_provider {
            entries.push(("onnxruntime_ep_webgpu/libonnxruntime_providers_webgpu.so", "ELF"));
        }
        for (name, body) in entries {
            zip.start_file(name, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn a_wheel_gives_up_the_provider_and_its_notices() {
        let dir = std::env::temp_dir().join("numa-wheel-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = wheel(&dir, true);
        unpack_wheel(&path, &dir).unwrap();
        for name in [
            numa::infer::GPU_PLUGIN,
            "onnxruntime_providers_webgpu.LICENSE.txt",
            "onnxruntime_providers_webgpu.ThirdPartyNotices.txt",
        ] {
            assert!(dir.join(name).is_file(), "{name} should have been unpacked");
        }
        assert!(!dir.join("__init__.py").exists(), "the Python is not ours to install");

        let empty = dir.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let path = wheel(&empty, false);
        assert!(unpack_wheel(&path, &empty).is_err(), "a wheel with no provider is a failure");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
