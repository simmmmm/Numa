use std::ffi::{c_void, CStr};
use std::sync::OnceLock;

use numa_core::space::ColourSpace;

#[repr(C)]
struct BasicInfo {
    have_container: i32,
    xsize: u32,
    ysize: u32,
    bits_per_sample: u32,
    exponent_bits_per_sample: u32,
    intensity_target: f32,
    min_nits: f32,
    relative_to_max_display: i32,
    linear_below: f32,
    uses_original_profile: i32,
    have_preview: i32,
    have_animation: i32,
    orientation: i32,
    num_color_channels: u32,
    num_extra_channels: u32,
    alpha_bits: u32,
    alpha_exponent_bits: u32,
    alpha_premultiplied: i32,
    preview: [u32; 2],
    animation: [u32; 4],
    intrinsic_xsize: u32,
    intrinsic_ysize: u32,
    padding: [u8; 100],
}

#[repr(C)]
struct PixelFormat {
    num_channels: u32,
    data_type: i32,
    endianness: i32,
    align: usize,
}

const SUCCESS: i32 = 0;
const NEED_MORE_OUTPUT: i32 = 2;
const TYPE_UINT16: i32 = 3;
const NATIVE_ENDIAN: i32 = 0;
const SETTING_EFFORT: i32 = 0;

type Encoder = c_void;
type Settings = c_void;
type Runner = unsafe extern "C" fn();

struct Library {
    create: unsafe extern "C" fn(*const c_void) -> *mut Encoder,
    destroy: unsafe extern "C" fn(*mut Encoder),
    set_runner: unsafe extern "C" fn(*mut Encoder, Option<Runner>, *mut c_void) -> i32,
    init_info: unsafe extern "C" fn(*mut BasicInfo),
    set_info: unsafe extern "C" fn(*mut Encoder, *const BasicInfo) -> i32,
    set_icc: unsafe extern "C" fn(*mut Encoder, *const u8, usize) -> i32,
    settings: unsafe extern "C" fn(*mut Encoder, *const Settings) -> *mut Settings,
    set_option: unsafe extern "C" fn(*mut Settings, i32, i64) -> i32,
    set_distance: unsafe extern "C" fn(*mut Settings, f32) -> i32,
    set_lossless: unsafe extern "C" fn(*mut Settings, i32) -> i32,
    use_boxes: unsafe extern "C" fn(*mut Encoder) -> i32,
    add_box: unsafe extern "C" fn(*mut Encoder, *const u8, *const u8, usize, i32) -> i32,
    add_frame: unsafe extern "C" fn(*const Settings, *const PixelFormat, *const c_void, usize) -> i32,
    close_input: unsafe extern "C" fn(*mut Encoder),
    process: unsafe extern "C" fn(*mut Encoder, *mut *mut u8, *mut usize) -> i32,
    runner: Option<(Runner, unsafe extern "C" fn(*const c_void, usize) -> *mut c_void, unsafe extern "C" fn(*mut c_void))>,
}

unsafe impl Send for Library {}
unsafe impl Sync for Library {}

fn open(names: &[&CStr]) -> Option<*mut c_void> {
    names.iter().find_map(|name| {
        let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        (!handle.is_null()).then_some(handle)
    })
}

unsafe fn symbol<T>(handle: *mut c_void, name: &CStr) -> Option<T> {
    let address = libc::dlsym(handle, name.as_ptr());
    (!address.is_null()).then(|| std::mem::transmute_copy(&address))
}

fn library() -> Option<&'static Library> {
    static LIBRARY: OnceLock<Option<Library>> = OnceLock::new();
    LIBRARY
        .get_or_init(|| unsafe {
            let jxl = open(&[c"libjxl.so.0.12", c"libjxl.so.0.11", c"libjxl.so.0.10", c"libjxl.so.0.9", c"libjxl.so.0.8", c"libjxl.so.0.7", c"libjxl.so"])?;

            let threads = open(&[c"libjxl_threads.so.0.12", c"libjxl_threads.so.0.11", c"libjxl_threads.so.0.10", c"libjxl_threads.so.0.9", c"libjxl_threads.so.0.8", c"libjxl_threads.so.0.7", c"libjxl_threads.so"]);
            let runner = threads.and_then(|threads| {
                Some((
                    symbol(threads, c"JxlThreadParallelRunner")?,
                    symbol(threads, c"JxlThreadParallelRunnerCreate")?,
                    symbol(threads, c"JxlThreadParallelRunnerDestroy")?,
                ))
            });
            Some(Library {
                create: symbol(jxl, c"JxlEncoderCreate")?,
                destroy: symbol(jxl, c"JxlEncoderDestroy")?,
                set_runner: symbol(jxl, c"JxlEncoderSetParallelRunner")?,
                init_info: symbol(jxl, c"JxlEncoderInitBasicInfo")?,
                set_info: symbol(jxl, c"JxlEncoderSetBasicInfo")?,
                set_icc: symbol(jxl, c"JxlEncoderSetICCProfile")?,
                settings: symbol(jxl, c"JxlEncoderFrameSettingsCreate")?,
                set_option: symbol(jxl, c"JxlEncoderFrameSettingsSetOption")?,
                set_distance: symbol(jxl, c"JxlEncoderSetFrameDistance")?,
                set_lossless: symbol(jxl, c"JxlEncoderSetFrameLossless")?,
                use_boxes: symbol(jxl, c"JxlEncoderUseBoxes")?,
                add_box: symbol(jxl, c"JxlEncoderAddBox")?,
                add_frame: symbol(jxl, c"JxlEncoderAddImageFrame")?,
                close_input: symbol(jxl, c"JxlEncoderCloseInput")?,
                process: symbol(jxl, c"JxlEncoderProcessOutput")?,
                runner,
            })
        })
        .as_ref()
}

pub fn is_available() -> bool {
    library().is_some()
}

fn distance(quality: f32) -> f32 {
    match quality {
        q if q >= 100.0 => 0.0,
        q if q >= 30.0 => 0.1 + (100.0 - q) * 0.09,
        q => 53.0 / 3000.0 * q * q - 23.0 / 20.0 * q + 25.0,
    }
}

pub fn encode(pixels: &[u16], width: u32, height: u32, space: ColourSpace, quality: u8, exif: Option<&[u8]>, xmp: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let library = library().ok_or("libjxl is not installed")?;
    let lossless = quality >= 100;
    let check = |status: i32, what: &str| match status {
        SUCCESS => Ok(()),
        _ => Err(format!("libjxl refused {what}")),
    };
    unsafe {
        let encoder = (library.create)(std::ptr::null());
        if encoder.is_null() {
            return Err("libjxl could not start".to_string());
        }

        struct Owned<'a>(&'a Library, *mut Encoder, *mut c_void);
        impl Drop for Owned<'_> {
            fn drop(&mut self) {
                unsafe {
                    (self.0.destroy)(self.1);
                    if let (Some((_, _, destroy)), false) = (self.0.runner, self.2.is_null()) {
                        destroy(self.2);
                    }
                }
            }
        }
        let mut owned = Owned(library, encoder, std::ptr::null_mut());
        if let Some((run, create, _)) = library.runner {
            owned.2 = create(std::ptr::null(), rayon::current_num_threads());
            if !owned.2.is_null() {
                check((library.set_runner)(encoder, Some(run), owned.2), "its threads")?;
            }
        }

        let mut info: BasicInfo = std::mem::zeroed();
        (library.init_info)(&mut info);
        info.xsize = width;
        info.ysize = height;
        info.bits_per_sample = 16;
        info.num_color_channels = 3;

        info.uses_original_profile = lossless as i32;
        check((library.set_info)(encoder, &info), "the image size")?;
        let icc = crate::icc::profile(space);
        check((library.set_icc)(encoder, icc.as_ptr(), icc.len()), "the colour profile")?;

        if exif.is_some() || xmp.is_some() {
            check((library.use_boxes)(encoder), "metadata")?;
        }
        if let Some(exif) = exif {

            let mut body = vec![0u8; 4];
            body.extend_from_slice(exif);
            check((library.add_box)(encoder, b"Exif".as_ptr(), body.as_ptr(), body.len(), 0), "the camera's data")?;
        }
        if let Some(xmp) = xmp {
            check((library.add_box)(encoder, b"xml ".as_ptr(), xmp.as_ptr(), xmp.len(), 0), "the description")?;
        }

        let settings = (library.settings)(encoder, std::ptr::null());
        check((library.set_option)(settings, SETTING_EFFORT, 7), "the effort")?;
        match lossless {
            true => check((library.set_lossless)(settings, 1), "lossless")?,
            false => check((library.set_distance)(settings, distance(quality as f32)), "the quality")?,
        }
        let format = PixelFormat { num_channels: 3, data_type: TYPE_UINT16, endianness: NATIVE_ENDIAN, align: 0 };
        check(
            (library.add_frame)(settings, &format, pixels.as_ptr().cast(), std::mem::size_of_val(pixels)),
            "the photograph",
        )?;
        (library.close_input)(encoder);

        let mut out = vec![0u8; 1 << 20];
        let mut written = 0usize;
        loop {
            let mut next = out.as_mut_ptr().add(written);
            let mut room = out.len() - written;
            let status = (library.process)(encoder, &mut next, &mut room);
            written = out.len() - room;
            match status {
                SUCCESS => break,
                NEED_MORE_OUTPUT => out.resize(out.len() * 2, 0),
                _ => return Err("libjxl could not encode the photograph".to_string()),
            }
        }
        out.truncate(written);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_maps_as_cjxl_does() {
        assert_eq!(distance(100.0), 0.0);
        assert!((distance(90.0) - 1.0).abs() < 1e-5);
        assert!((distance(30.0) - 6.4).abs() < 1e-4);
        assert!(distance(10.0) > distance(30.0));
    }

    #[test]
    fn a_frame_is_a_jpeg_xl_file() {
        if !is_available() {
            println!("libjxl is not installed here");
            return;
        }
        let (width, height) = (96u32, 64u32);

        let mut seed = 1u32;
        let pixels: Vec<u16> = (0..width * height * 3)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 16) as u16
            })
            .collect();
        let exif = b"II*\0\x08\0\0\0\0\0\0\0\0\0";
        let lossy = encode(&pixels, width, height, ColourSpace::DisplayP3, 90, Some(exif), Some(b"<x:xmpmeta/>")).unwrap();
        let lossless = encode(&pixels, width, height, ColourSpace::Srgb, 100, None, None).unwrap();

        assert_eq!(&lossy[..12], &[0, 0, 0, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A]);
        assert!(lossy.windows(4).any(|w| w == b"Exif"), "the camera's data went in");
        assert!(lossless.len() > lossy.len());
        if let Ok(out) = std::env::var("JXL_OUT") {
            std::fs::write(format!("{out}/lossy.jxl"), &lossy).unwrap();
            std::fs::write(format!("{out}/lossless.jxl"), &lossless).unwrap();
        }
    }
}
