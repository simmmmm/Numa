use std::sync::LazyLock;

use alloc::borrow::ToOwned as _;

use objc2::{
    available,
    rc::{autoreleasepool, Retained},
    runtime::ProtocolObject,
    ClassType, Message,
};
use objc2_core_foundation::{CFString, CGSize};
use objc2_core_graphics::CGColorSpace;
use objc2_foundation::NSObjectProtocol;
use objc2_metal::MTLTextureType;
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
use parking_lot::{Mutex, RwLock};

use super::OsFeatures;

#[cfg(target_os = "macos")]
fn hosting_window(
    start: Retained<objc2_quartz_core::CALayer>,
) -> Option<Retained<objc2::runtime::NSObject>> {
    let mut current = Some(start);
    while let Some(layer) = current {
        if let Some(delegate) = layer.delegate() {
            return unsafe { objc2::msg_send![&*delegate, window] };
        }
        current = layer.superlayer();
    }
    None
}

impl super::Surface {
    pub fn new(layer: Retained<CAMetalLayer>) -> Self {
        Self {
            render_layer: Mutex::new(layer),
            swapchain_format: RwLock::new(None),
            extent: RwLock::new(wgt::Extent3d::default()),
        }
    }

    pub fn from_layer(layer: &CAMetalLayer) -> Self {
        assert!(layer.isKindOfClass(CAMetalLayer::class()));
        Self::new(layer.retain())
    }

    pub fn render_layer(&self) -> &Mutex<Retained<CAMetalLayer>> {
        &self.render_layer
    }

    pub(super) fn display_hdr_info(&self) -> Option<wgt::DisplayHdrInfo> {
        #[cfg(target_os = "macos")]
        {
            use objc2::rc::Retained;
            use objc2::runtime::NSObject;

            if objc2::MainThreadMarker::new().is_none() {

                static WARN_ONCE: std::sync::Once = std::sync::Once::new();
                WARN_ONCE.call_once(|| {
                    log::warn!(
                        "Surface::display_hdr_info() was called from thread {:?} \
                         and will return None. On the Metal backend, it must be \
                         called from the main thread to succeed.",
                        std::thread::current().id()
                    );
                });
                return None;
            }

            let render_layer = {
                let guard = self.render_layer.lock();
                guard.clone()
            };

            let screen: Retained<NSObject> = autoreleasepool(|_| {
                hosting_window(Retained::into_super(render_layer))
                    .and_then(|window| unsafe { objc2::msg_send![&*window, screen] })
            })?;

            let finite = |v: f64| v.is_finite().then_some(v as f32);

            let current: f64 = unsafe {
                objc2::msg_send![&*screen, maximumExtendedDynamicRangeColorComponentValue]
            };

            let high_dynamic_range = current.is_finite().then_some(current > 1.0);

            let (potential, reference) = if available!(macos = 10.15) {
                let potential: f64 = unsafe {
                    objc2::msg_send![
                        &*screen,
                        maximumPotentialExtendedDynamicRangeColorComponentValue
                    ]
                };
                let reference: f64 = unsafe {
                    objc2::msg_send![
                        &*screen,
                        maximumReferenceExtendedDynamicRangeColorComponentValue
                    ]
                };

                (finite(potential), finite(reference).filter(|&v| v > 0.0))
            } else {
                (None, None)
            };
            let headroom = wgt::DisplayHeadroom {
                current: finite(current),
                potential,
                reference,
            };

            let coarse = wgt::DisplayCoarseRange {
                high_dynamic_range,
                gamut: None,
            };

            let info = wgt::DisplayHdrInfo {
                luminance: None,
                headroom: Some(headroom),
                chromaticity: None,
                coarse: Some(coarse),
                bits_per_color: None,
            };
            Some(info)
        }
        #[cfg(not(target_os = "macos"))]
        {

            None
        }
    }

    pub(super) fn dimensions(&self) -> wgt::Extent3d {
        let (size, scale) = {
            let render_layer = self.render_layer.lock();
            let bounds = render_layer.bounds();
            let contents_scale = render_layer.contentsScale();
            (bounds.size, contents_scale)
        };

        wgt::Extent3d {
            width: (size.width * scale) as u32,
            height: (size.height * scale) as u32,
            depth_or_array_layers: 1,
        }
    }
}

unsafe impl Send for ColorSpaces {}
unsafe impl Sync for ColorSpaces {}

struct ColorSpaces {
    extended_display_p3: &'static CFString,
    itur_bt2100_pq: &'static CFString,
    itur_bt2100_hlg: &'static CFString,
}

static COLOR_SPACES: LazyLock<Result<ColorSpaces, crate::SurfaceError>> = LazyLock::new(|| {

    let lib = unsafe {
        libloading::Library::new("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics")
            .map_err(|_| crate::SurfaceError::Other("error loading CoreGraphics"))?
    };
    fn lookup(
        lib: &libloading::Library,
        name: &[u8],
    ) -> Result<&'static CFString, crate::SurfaceError> {
        let sym = unsafe { lib.get(name) }
            .map_err(|_| crate::SurfaceError::Other("error resolving symbol in CoreGraphics"))?;
        Ok(*sym)
    }
    let extended_display_p3 = lookup(&lib, b"kCGColorSpaceExtendedDisplayP3\0")?;
    let itur_bt2100_pq = lookup(&lib, b"kCGColorSpaceITUR_2100_PQ\0")?;
    let itur_bt2100_hlg = lookup(&lib, b"kCGColorSpaceITUR_2100_HLG\0")?;
    Ok(ColorSpaces {
        extended_display_p3,
        itur_bt2100_pq,
        itur_bt2100_hlg,
    })
});

impl crate::Surface for super::Surface {
    type A = super::Api;

    unsafe fn configure(
        &self,
        device: &super::Device,
        config: &crate::SurfaceConfiguration,
    ) -> Result<(), crate::SurfaceError> {
        log::debug!("build swapchain {config:?}");

        let caps = &device.shared.private_texture_format_caps;
        *self.swapchain_format.write() = Some(config.format);
        *self.extent.write() = config.extent;

        let render_layer = self.render_layer.lock();
        let framebuffer_only = config.usage == wgt::TextureUses::COLOR_TARGET;
        let display_sync = match config.present_mode {
            wgt::PresentMode::Fifo => true,
            wgt::PresentMode::Immediate => false,
            m => unreachable!("Unsupported present mode: {m:?}"),
        };

        let drawable_size = CGSize::new(config.extent.width as _, config.extent.height as _);

        match config.composite_alpha_mode {
            wgt::CompositeAlphaMode::Opaque => render_layer.setOpaque(true),
            wgt::CompositeAlphaMode::PostMultiplied => render_layer.setOpaque(false),
            _ => (),
        }

        let device_raw = &device.shared.device;
        render_layer.setDevice(Some(device_raw));
        render_layer.setPixelFormat(caps.map_format(config.format));
        render_layer.setFramebufferOnly(framebuffer_only);

        let wants_edr = config.color_space.is_hdr();
        if wants_edr != render_layer.wantsExtendedDynamicRangeContent() {
            render_layer.setWantsExtendedDynamicRangeContent(wants_edr);
        }

        let colorspace_name: Option<&'static CFString> = match config.color_space {
            wgt::SurfaceColorSpace::Auto => {
                unreachable!("wgpu-core resolves `Auto` before configuring the surface")
            }

            wgt::SurfaceColorSpace::Srgb => None,
            wgt::SurfaceColorSpace::ExtendedSrgbLinear => {
                Some(unsafe { objc2_core_graphics::kCGColorSpaceExtendedLinearSRGB })
            }
            wgt::SurfaceColorSpace::ExtendedSrgb => {
                Some(unsafe { objc2_core_graphics::kCGColorSpaceExtendedSRGB })
            }
            wgt::SurfaceColorSpace::ExtendedDisplayP3 => {

                Some(
                    COLOR_SPACES
                        .as_ref()
                        .map_err(|e| e.clone())?
                        .extended_display_p3,
                )
            }
            wgt::SurfaceColorSpace::DisplayP3 => {
                Some(unsafe { objc2_core_graphics::kCGColorSpaceDisplayP3 })
            }
            wgt::SurfaceColorSpace::Bt2100Pq | wgt::SurfaceColorSpace::Bt2100Hlg => {

                if !available!(macos = 11.0, ios = 14.0, tvos = 14.0, visionos = 1.0) {
                    unreachable!("BT.2100 PQ/HLG color spaces are only reported on macOS 11.0+/iOS 14.0+/tvOS 14.0+");
                }
                Some(if config.color_space == wgt::SurfaceColorSpace::Bt2100Pq {
                    COLOR_SPACES.as_ref().map_err(|e| e.clone())?.itur_bt2100_pq
                } else {
                    COLOR_SPACES
                        .as_ref()
                        .map_err(|e| e.clone())?
                        .itur_bt2100_hlg
                })
            }
        };
        let colorspace = colorspace_name.and_then(|name| CGColorSpace::with_name(Some(name)));
        render_layer.setColorspace(colorspace.as_deref());

        render_layer.setMaximumDrawableCount(config.maximum_frame_latency as usize + 1);
        render_layer.setDrawableSize(drawable_size);

        if available!(macos = 10.13, ios = 11.0, tvos = 11.0, visionos = 1.0) {
            render_layer.setAllowsNextDrawableTimeout(false);
        }
        if OsFeatures::display_sync() {
            render_layer.setDisplaySyncEnabled(display_sync);
        }

        Ok(())
    }

    unsafe fn unconfigure(&self, _device: &super::Device) {
        *self.swapchain_format.write() = None;
    }

    unsafe fn acquire_texture(
        &self,
        _timeout: Option<core::time::Duration>,
        _fence: &super::Fence,
    ) -> Result<crate::AcquiredSurfaceTexture<super::Api>, crate::SurfaceError> {
        let render_layer = self.render_layer.lock();

        #[cfg(target_os = "macos")]
        {

            use objc2::rc::Retained;

            if let Some(window) = hosting_window(Retained::into_super(render_layer.clone())) {
                const NS_WINDOW_OCCLUSION_STATE_VISIBLE: usize = 1 << 1;
                let occlusion_state: usize = unsafe { objc2::msg_send![&*window, occlusionState] };
                if occlusion_state & NS_WINDOW_OCCLUSION_STATE_VISIBLE == 0 {
                    return Err(crate::SurfaceError::Occluded);
                }
            }
        }

        let (drawable, texture) = match autoreleasepool(|_| {
            render_layer
                .nextDrawable()
                .map(|drawable| (drawable.to_owned(), drawable.texture().to_owned()))
        }) {
            Some(pair) => pair,
            None => return Err(crate::SurfaceError::Timeout),
        };

        let swapchain_format = self.swapchain_format.read().unwrap();
        let extent = self.extent.read();
        let suf_texture = super::SurfaceTexture {
            texture: super::Texture {
                raw: texture,
                format: swapchain_format,
                raw_type: MTLTextureType::Type2D,
                array_layers: 1,
                mip_levels: 1,
                copy_size: crate::CopyExtent {
                    width: extent.width,
                    height: extent.height,
                    depth: 1,
                },
                _drop_guard: None,
            },
            drawable: ProtocolObject::from_retained(drawable),
            present_with_transaction: render_layer.presentsWithTransaction(),
        };

        Ok(crate::AcquiredSurfaceTexture {
            texture: suf_texture,
            suboptimal: false,
        })
    }

    unsafe fn discard_texture(&self, _texture: super::SurfaceTexture) {}
}
