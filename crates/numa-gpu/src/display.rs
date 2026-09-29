use crate::Gpu;
use std::ffi::{c_char, c_void, CStr};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

pub const FOURCC: u32 = u32::from_le_bytes(*b"AB24");

pub const MODIFIER: u64 = 0;

const BUFFERS: usize = 3;

pub struct Shown {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    fd: RawFd,
    free: Option<Arc<AtomicBool>>,
}

impl Shown {

    pub fn hand_over(mut self) -> (RawFd, impl FnOnce() + Send + 'static) {
        let free = self.free.take();
        (self.fd, move || {
            if let Some(free) = free {
                free.store(true, Ordering::Release);
            }
        })
    }
}

impl Drop for Shown {
    fn drop(&mut self) {
        if let Some(free) = self.free.take() {
            free.store(true, Ordering::Release);
        }
    }
}

struct Gbm {
    create_device: unsafe extern "C" fn(i32) -> *mut c_void,
    bo_create: unsafe extern "C" fn(*mut c_void, u32, u32, u32, u32) -> *mut c_void,
    bo_get_fd: unsafe extern "C" fn(*mut c_void) -> i32,
    bo_get_stride: unsafe extern "C" fn(*mut c_void) -> u32,
    bo_get_modifier: unsafe extern "C" fn(*mut c_void) -> u64,
    bo_destroy: unsafe extern "C" fn(*mut c_void),
}

const GBM_BO_USE_RENDERING: u32 = 1 << 2;
const GBM_BO_USE_LINEAR: u32 = 1 << 4;

fn gbm() -> Option<&'static Gbm> {
    static GBM: OnceLock<Option<Gbm>> = OnceLock::new();
    GBM.get_or_init(|| {

        unsafe {
            let library = libc::dlopen(c"libgbm.so.1".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if library.is_null() {
                log::info!("GPU: no libgbm; the render is read back for GTK");
                return None;
            }
            let symbol = |name: &CStr| {
                let found = libc::dlsym(library, name.as_ptr() as *const c_char);
                (!found.is_null()).then_some(found)
            };
            Some(Gbm {
                create_device: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(i32) -> *mut c_void>(symbol(c"gbm_create_device")?),
                bo_create: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void, u32, u32, u32, u32) -> *mut c_void>(symbol(c"gbm_bo_create")?),
                bo_get_fd: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void) -> i32>(symbol(c"gbm_bo_get_fd")?),
                bo_get_stride: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void) -> u32>(symbol(c"gbm_bo_get_stride")?),
                bo_get_modifier: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void) -> u64>(symbol(c"gbm_bo_get_modifier")?),
                bo_destroy: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void)>(symbol(c"gbm_bo_destroy")?),
            })
        }
    })
    .as_ref()
}

pub(crate) struct Pool {

    _node: std::fs::File,
    device: *mut c_void,
    buffers: Vec<Buffer>,
}

unsafe impl Send for Pool {}

struct Buffer {
    bo: *mut c_void,
    fd: OwnedFd,
    stride: u32,
    width: u32,
    height: u32,
    texture: wgpu::Texture,
    free: Arc<AtomicBool>,
}

impl Drop for Buffer {
    fn drop(&mut self) {
        self.texture.destroy();
        if let Some(gbm) = gbm() {

            unsafe { (gbm.bo_destroy)(self.bo) };
        }
    }
}

fn render_node(gpu: &Gpu) -> Option<std::fs::File> {
    let read = |path: std::path::PathBuf| std::fs::read_to_string(path).ok().and_then(|text| u32::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok());
    let nodes = std::fs::read_dir("/sys/class/drm").ok()?;
    let node = nodes.flatten().map(|entry| entry.file_name().to_string_lossy().into_owned()).find(|name| {
        let device = std::path::Path::new("/sys/class/drm").join(name).join("device");
        name.starts_with("renderD") && read(device.join("vendor")) == Some(gpu.pci.0) && read(device.join("device")) == Some(gpu.pci.1)
    })?;
    std::fs::OpenOptions::new().read(true).write(true).open(std::path::Path::new("/dev/dri").join(node)).ok()
}

impl Pool {
    fn open(gpu: &Gpu) -> Option<Self> {
        if !gpu.dmabuf {
            log::info!("GPU: {} does not import a dmabuf; the render is read back for GTK", gpu.name);
            return None;
        }
        let gbm = gbm()?;
        let node = render_node(gpu)?;

        let device = unsafe { (gbm.create_device)(node.as_raw_fd()) };
        (!device.is_null()).then_some(Pool { _node: node, device, buffers: Vec::new() })
    }

    fn take(&mut self, gpu: &Gpu, width: u32, height: u32) -> Result<&Buffer, String> {

        self.buffers.retain(|buffer| (buffer.width, buffer.height) == (width, height) || !buffer.free.load(Ordering::Acquire));
        let free = self.buffers.iter().position(|buffer| (buffer.width, buffer.height) == (width, height) && buffer.free.load(Ordering::Acquire));
        let at = match free {
            Some(at) => at,
            None if self.buffers.iter().filter(|buffer| (buffer.width, buffer.height) == (width, height)).count() < BUFFERS => {
                self.buffers.push(make(gpu, self.device, width, height)?);
                self.buffers.len() - 1
            }
            None => return Err("GTK still holds every buffer".into()),
        };
        let buffer = &self.buffers[at];
        buffer.free.store(false, Ordering::Release);
        Ok(buffer)
    }
}

const PAD: u32 = 64;

fn make(gpu: &Gpu, device: *mut c_void, width: u32, height: u32) -> Result<Buffer, String> {
    let gbm = gbm().ok_or("no libgbm")?;

    let (bo, fd, stride, modifier) = unsafe {
        let bo = (gbm.bo_create)(device, width.next_multiple_of(PAD), height.next_multiple_of(PAD), FOURCC, GBM_BO_USE_RENDERING | GBM_BO_USE_LINEAR);
        if bo.is_null() {
            return Err("libgbm made no buffer".into());
        }
        let fd = (gbm.bo_get_fd)(bo);
        if fd < 0 {
            (gbm.bo_destroy)(bo);
            return Err("libgbm gave no dmabuf".into());
        }
        (bo, OwnedFd::from_raw_fd(fd), (gbm.bo_get_stride)(bo), (gbm.bo_get_modifier)(bo))
    };
    let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
    let imported = fd.try_clone().map_err(|err| err.to_string()).and_then(|for_wgpu| {

        unsafe {
            let hal = gpu.device.as_hal::<wgpu::hal::api::Vulkan>().ok_or("not Vulkan")?;
            hal.texture_from_dmabuf_fd(
                for_wgpu,
                &wgpu::hal::TextureDescriptor {
                    label: Some("shown"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUses::COPY_DST,
                    memory_flags: wgpu::hal::MemoryFlags::empty(),
                    view_formats: Vec::new(),
                },
                modifier,
                u64::from(stride),
                0,
            )
            .map_err(|err| format!("the card did not import the buffer: {err}"))
        }
    });
    let hal_texture = match imported {
        Ok(texture) => texture,
        Err(err) => {

            unsafe { (gbm.bo_destroy)(bo) };
            return Err(err);
        }
    };

    let texture = unsafe {
        gpu.device.create_texture_from_hal::<wgpu::hal::api::Vulkan>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some("shown"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::TextureUses::UNINITIALIZED,
        )
    };
    Ok(Buffer { bo, fd, stride, width, height, texture, free: Arc::new(AtomicBool::new(true)) })
}

pub(crate) fn show(
    gpu: &Gpu,
    state: &crate::render::State,
    encoder: &mut wgpu::CommandEncoder,
    out: &wgpu::Buffer,
    width: u32,
    height: u32,
    stride_px: u32,
) -> Result<Shown, String> {
    let mut pool = state.display.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if pool.is_none() {
        *pool = Some(Pool::open(gpu).ok_or("no dmabuf on this card")?);
    }
    let buffer = pool.as_mut().expect("opened above").take(gpu, width, height)?;
    encoder.copy_buffer_to_texture(
        wgpu::TexelCopyBufferInfo {
            buffer: out,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride_px * 4), rows_per_image: None },
        },
        buffer.texture.as_image_copy(),
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    Ok(Shown { width, height, stride: buffer.stride, fd: buffer.fd.as_raw_fd(), free: Some(buffer.free.clone()) })
}

pub fn available(frugal: bool) -> bool {
    crate::ready(frugal).is_some_and(|gpu| gpu.dmabuf) && gbm().is_some()
}
