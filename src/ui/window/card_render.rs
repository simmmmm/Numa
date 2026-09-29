use super::*;

pub(super) enum Picture {

    Pixels(image::RgbImage),

    Rgba { width: u32, height: u32, bytes: Vec<u8>, stride: usize },

    #[cfg(feature = "gpu")]
    Card(numa::gpu::display::Shown),
}

impl Picture {
    pub(super) fn dimensions(&self) -> (u32, u32) {
        match self {
            Picture::Pixels(image) => image.dimensions(),
            Picture::Rgba { width, height, .. } => (*width, *height),
            #[cfg(feature = "gpu")]
            Picture::Card(shown) => (shown.width, shown.height),
        }
    }

    pub(super) fn texture(self, state: &App) -> (Option<gtk::gdk::Texture>, Option<image::RgbImage>) {
        match self {
            Picture::Pixels(mut image) => {
                let overlay = overlay_of(state);
                if overlay.is_off() {
                    return (Some(texture_from(&image)), Some(image));
                }
                render::histogram::mark_clipping(&mut image, overlay);
                (Some(texture_from(&image)), None)
            }
            Picture::Rgba { width, height, bytes, stride } => (
                Some(gtk::gdk::MemoryTexture::new(width as i32, height as i32, gtk::gdk::MemoryFormat::R8g8b8a8, &glib::Bytes::from_owned(bytes), stride).upcast()),
                None,
            ),
            #[cfg(feature = "gpu")]
            Picture::Card(shown) => (dmabuf_texture(state, shown), None),
        }
    }
}

fn overlay_of(state: &App) -> render::histogram::ClippingOverlay {
    render::histogram::ClippingOverlay { shadows: state.info.shadow_clip.is_active(), highlights: state.info.highlight_clip.is_active() }
}

pub(super) struct CardAsk {
    proxy: Arc<LinearImage>,
    inputs: render::RenderInputs,
    overlay: u32,
    dmabuf: bool,
}

pub(super) fn card_may_render() -> bool {
    #[cfg(feature = "gpu")]
    {
        numa::gpu::render_enabled() && numa::gpu::describe(raw::card_frugal()).is_some()
    }
    #[cfg(not(feature = "gpu"))]
    {
        false
    }
}

pub(super) fn ask(state: &App, photo: &OpenPhoto) -> Option<CardAsk> {
    #[cfg(feature = "gpu")]
    {
        if !card_may_render() {
            return None;
        }
        let overlay = overlay_of(state);
        Some(CardAsk {
            proxy: photo.proxy.clone(),
            inputs: photo.inputs.clone(),
            overlay: u32::from(overlay.shadows) | u32::from(overlay.highlights) << 1,
            dmabuf: dmabuf_shown(state),
        })
    }
    #[cfg(not(feature = "gpu"))]
    {
        let _ = (state, photo);
        None
    }
}

pub(super) fn render_on_card(ask: &CardAsk, document: &Document, proxy_scale: f32, scope: render::scope::Kind) -> Option<(Picture, render::histogram::Histogram)> {
    #[cfg(feature = "gpu")]
    {
        let plan = match render::card::plan(document, &ask.proxy, &ask.inputs, proxy_scale) {
            Ok(plan) => plan,
            Err(stage) => {
                say_once(stage);
                return None;
            }
        };

        if numa::gpu::discrete(raw::card_frugal()) && !plan.heavy() {
            return None;
        }

        let _card = numa::infer::try_card()?;

        let scoped = scope != render::scope::Kind::Histogram;
        let output = match ask.dmabuf && !scoped {
            true => numa::gpu::Output::Dmabuf,
            false => numa::gpu::Output::ReadBack,
        };
        let overlay = if scoped { 0 } else { ask.overlay };
        let rendered = match numa::gpu::render(&ask.proxy, &plan, overlay, raw::card_frugal(), output) {
            Ok(rendered) => rendered,
            Err(err) => {
                log::info!("render on the card: {err}; on the processor");
                return None;
            }
        };
        ON_CARD.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (width, height) = (rendered.width, rendered.height);
        let bins = &rendered.histogram;
        let histogram = render::histogram::Histogram {
            channels: std::array::from_fn(|channel| std::array::from_fn(|bin| bins[channel * 256 + bin])),
            shadow_clipped: bins[768],
            highlight_clipped: bins[769],
            total: (width * height).div_ceil(4),
        };
        let picture = match rendered.pixels {
            numa::gpu::Pixels::Rgba { bytes, stride } if scoped => {
                let rgb = bytes.chunks_exact(stride).flat_map(|row| row[..width as usize * 4].chunks_exact(4).flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])).collect();
                Picture::Pixels(image::RgbImage::from_raw(width, height, rgb)?)
            }
            numa::gpu::Pixels::Rgba { bytes, stride } => Picture::Rgba { width, height, bytes, stride },
            numa::gpu::Pixels::Dmabuf(shown) => Picture::Card(shown),
        };
        Some((picture, histogram))
    }
    #[cfg(not(feature = "gpu"))]
    {
        let _ = (ask, document, proxy_scale, scope);
        None
    }
}

static ON_CARD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(super) fn renders_on_card() -> u64 {
    ON_CARD.load(std::sync::atomic::Ordering::Relaxed)
}

pub(super) fn let_go() {
    #[cfg(feature = "gpu")]
    numa::gpu::forget_proxies();
}

#[cfg(feature = "gpu")]
fn say_once(stage: &'static str) {
    static SAID: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut said = SAID.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if timing() && !said.contains(&stage) {
        log::info!("render on the processor: the card does not have {stage} yet");
        said.push(stage);
    }
}

thread_local! {

    static DMABUF: Cell<Option<bool>> = const { Cell::new(None) };
}

fn dmabuf_shown(state: &App) -> bool {
    #[cfg(feature = "gpu")]
    {
        if std::env::var("NUMA_GPU_DMABUF").as_deref() != Ok("1") || DMABUF.get() == Some(false) {
            return false;
        }
        let drawn_by_card = state
            .canvas
            .native()
            .and_then(|native| native.renderer())
            .is_some_and(|renderer| !renderer.type_().name().contains("Cairo"));
        use numa::gpu::display::{FOURCC, MODIFIER};
        drawn_by_card && state.canvas.display().dmabuf_formats().contains(FOURCC, MODIFIER) && numa::gpu::display::available(raw::card_frugal())
    }
    #[cfg(not(feature = "gpu"))]
    {
        let _ = state;
        false
    }
}

#[cfg(feature = "gpu")]
fn dmabuf_texture(state: &App, shown: numa::gpu::display::Shown) -> Option<gtk::gdk::Texture> {
    use numa::gpu::display::{FOURCC, MODIFIER};
    let (width, height, stride) = (shown.width, shown.height, shown.stride);
    let (fd, release) = shown.hand_over();

    let built = unsafe {
        gtk::gdk::DmabufTextureBuilder::new()
            .set_display(&state.canvas.display())
            .set_width(width)
            .set_height(height)
            .set_fourcc(FOURCC)
            .set_modifier(MODIFIER)
            .set_premultiplied(false)
            .set_n_planes(1)
            .set_fd(0, fd)
            .set_stride(0, stride)
            .set_offset(0, 0)
            .build_with_release_func(release)
    };
    match built {
        Ok(texture) => {
            if DMABUF.replace(Some(true)).is_none() {
                log::info!("render on the card: GTK takes the frame as a dmabuf");
            }
            Some(texture)
        }
        Err(err) => {
            log::info!("render on the card: GTK did not take the dmabuf ({err}); reading back from now on");
            DMABUF.set(Some(false));
            schedule_render(state);
            None
        }
    }
}
