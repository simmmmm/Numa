use crate::{block_on, ready, Gpu};
use numa_core::image::LinearImage;
use numa_render::card::{Adjustments, CameraProfileStage, MaskPlan, Plan, TablePlan, Tail};
use numa_render::detail::LumaShape;
use numa_render::effects::HazeShape;
use numa_render::local::ToneShape;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub fn render_enabled() -> bool {
    crate::enabled() && std::env::var("NUMA_GPU_RENDER").as_deref() != Ok("0")
}

pub struct Rendered {
    pub width: u32,
    pub height: u32,

    pub histogram: [u32; HISTOGRAM],
    pub pixels: Pixels,
}

pub const HISTOGRAM: usize = 770;

pub enum Pixels {

    Rgba { bytes: Vec<u8>, stride: usize },

    #[cfg(target_os = "linux")]
    Dmabuf(crate::display::Shown),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Output {
    ReadBack,
    #[cfg(target_os = "linux")]
    Dmabuf,
}

pub type Overlay = u32;

pub fn render(proxy: &LinearImage, plan: &Plan, overlay: Overlay, frugal: bool, output: Output) -> Result<Rendered, String> {
    let gpu = ready(frugal).ok_or("no card ready")?;
    let gpu = &*gpu;
    let state = gpu.render.as_ref().ok_or("the render did not build on this card")?;
    let memory = gpu.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(gpu, state, proxy, plan, overlay, output)));
    let invalid = block_on(validation.pop());
    let out_of_memory = block_on(memory.pop());
    match (result, out_of_memory, invalid) {
        (_, Some(error), _) | (_, _, Some(error)) => Err(format!("the card: {error}")),
        (Err(_), _, _) => Err("the card's render panicked".into()),
        (Ok(result), None, None) => result,
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Adjust {
    flags: [u32; 4],
    basic: [f32; 4],
    mix_dims: [u32; 4],
    mix_in: [[f32; 4]; 3],
    mix_out: [[f32; 4]; 3],
    places: [u32; 4],
    more: [u32; 4],
    gains: [f32; 4],
    tint: [f32; 4],
    grain: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Params {
    size: [u32; 4],
    src: [u32; 4],
    out_info: [u32; 4],
    mult: [f32; 4],
    misc: [f32; 4],
    m_in: [[f32; 4]; 3],
    m_out: [[f32; 4]; 3],
    map_dims: [u32; 4],
    look_dims: [u32; 4],
    whites: [f32; 4],
    curves: [u32; 4],
    curves_at: [u32; 4],
    tone_axis: [f32; 4],
    crop: [f32; 4],
    crop2: [f32; 4],
    keystone: [f32; 4],
    vignette: [[f32; 4]; 2],
    grain: [[f32; 4]; 2],
    adjust: Adjust,
    base: [[f32; 4]; 14],
}

unsafe impl bytemuck::Zeroable for Adjust {}
unsafe impl bytemuck::Pod for Adjust {}
unsafe impl bytemuck::Zeroable for Params {}
unsafe impl bytemuck::Pod for Params {}

const CAMERA: u32 = 1;
const RENDERING: u32 = 2;
const MAP: u32 = 4;
const LOOK: u32 = 8;
const CLIP: u32 = 16;
const DENOISE: u32 = 32;
const DISPLAY_REFERRED: u32 = 64;
const GEOMETRY: u32 = 128;
const VIGNETTE: u32 = 256;
const MASKS: u32 = 512;
const DEHAZE: u32 = 4096;
const GRAIN: u32 = 8192;
const KEPT: u32 = 16384;
const AGX: u32 = 32768;

const BASIC: u32 = 1;
const SLOPE: u32 = 2;
const TONE: u32 = 4;
const SATURATE: u32 = 8;
const MIXER: u32 = 16;
const POINTS: u32 = 32;
const MONO: u32 = 64;
const GRADE: u32 = 128;
const GAINS: u32 = 256;
const MASK_DENOISE: u32 = 512;
const CURVES: u32 = 1024;
const TINT: u32 = 2048;
const CURVE_ONE: u32 = 4096;
const MASK_GRAIN: u32 = 65536;

const MIRROR: u32 = 1;
const TRANSPOSE: u32 = 2;
const FLIP_X: u32 = 4;
const FLIP_Y: u32 = 8;
const CROP: u32 = 16;

const MAX_MASKS: usize = 32;
const MAX_STEPS: usize = 128;
const STEP: u32 = 256;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Step {
    what: [u32; 4],
    at: [u32; 4],
    shape: [u32; 4],
    more: [u32; 4],
    numbers: [f32; 4],
}

unsafe impl bytemuck::Zeroable for Step {}
unsafe impl bytemuck::Pod for Step {}

const LUMA: u32 = 1024;
const LOCAL: u32 = 2048;
const FEED_ONE: u32 = 0;
const FEED_SQUARES: u32 = 1;
const FEED_PAIR: u32 = 2;
const FEED_DIFFERENCE: u32 = 3;
const FEED_CHROMA: u32 = 4;
const EMIT_ONE: u32 = 0;
const EMIT_AB: u32 = 1;
const EMIT_GUIDED: u32 = 2;

const WHOLE: u32 = 0;
const TO_TONE: u32 = 1;
const FROM_TONE: u32 = 2;

const SETTLED: u32 = 2;

struct Planes {

    log: u32,
    pair: u32,
    ab: u32,
    mid: u32,
    small: (u32, u32),

    s: u32,
    pair_s: u32,
    ab_s: u32,
    base_s: u32,
    bloom_s: u32,
    partials: u32,
    pivot: u32,
    copy: u32,

    haze: u32,

    wide: u32,
    len: u32,
}

impl Planes {

    fn of(plan: &Plan, workgroups: u32) -> Self {
        let copy = plan.masks.iter().any(wide);
        let haze = plan.dehaze.iter().chain(plan.masks.iter().filter_map(|mask| mask.dehaze.as_ref())).next();
        let moire = plan.tail.moire.is_some() || plan.masks.iter().any(|mask| mask.tail.moire.is_some());
        let haze_cells = haze.map_or(0, |haze| (haze.grid.0 * haze.grid.1) as u32);
        let planes = Self::new(plan.width, plan.height, workgroups, copy, haze_cells, moire);
        let any = copy || haze.is_some() || plan.denoise_luma.is_some() || plan.local.is_some() || plan.tail.is_some();
        Self { len: if any { planes.len } else { 0 }, ..planes }
    }

    fn new(width: u32, height: u32, workgroups: u32, copy: bool, haze_cells: u32, moire: bool) -> Self {
        let n = width * height;
        let factor = numa_render::local::ToneShape::SUBSAMPLE as u32;
        let small = ((width / factor).max(1), (height / factor).max(1));
        let ns = small.0 * small.1;
        let s = 6 * n;
        let partials = s + 7 * ns;
        let copy_at = partials + workgroups + 1;
        let haze = if copy { copy_at + 3 * n } else { copy_at };
        let wide = haze + if haze_cells > 0 { 6 * haze_cells + 1 } else { 0 };
        let len = if moire { wide + 3 * n } else { wide };
        Planes { log: 0, pair: n, ab: 3 * n, mid: 5 * n, small, s, pair_s: s + ns, ab_s: s + 3 * ns, base_s: s + 5 * ns, bloom_s: s + 6 * ns, partials, pivot: partials + workgroups, copy: copy_at, haze, wide, len }
    }
}

fn rows(m: [[f32; 3]; 3]) -> [[f32; 4]; 3] {
    m.map(|row| [row[0], row[1], row[2], 0.0])
}

fn put(table: &TablePlan, tables: &mut Vec<f32>) -> (u32, [u32; 4]) {
    let start = tables.len() as u32;
    tables.extend(table.entries.iter().flatten());
    let [h, s, v] = table.divisions;
    (start, [h, s, v, u32::from(table.srgb)])
}

fn adjust_of(plan: &Adjustments, tables: &mut Vec<f32>) -> Adjust {
    let mut a: Adjust = bytemuck::Zeroable::zeroed();
    a.gains = [1.0, 1.0, 1.0, 0.0];
    let mut flags = 0;
    if let Some(basic) = &plan.basic {
        flags |= BASIC;
        a.basic = [basic.gain, basic.slope.unwrap_or(1.0), basic.saturation, basic.vibrance];
        if basic.slope.is_some() {
            flags |= SLOPE;
        }
        if let Some(stops) = &basic.tone {
            flags |= TONE;
            a.flags[1] = tables.len() as u32;
            a.flags[2] = stops.len() as u32;
            tables.extend(stops);
        }
        if basic.saturation != 0.0 || basic.vibrance != 0.0 {
            flags |= SATURATE;
        }
    }
    if let Some(mixer) = &plan.mixer {
        flags |= MIXER;
        (a.flags[3], a.mix_dims) = put(&mixer.table, tables);
        a.mix_in = rows(mixer.into);
        a.mix_out = rows(mixer.out);
    }
    if let Some(points) = &plan.points {
        flags |= POINTS;
        a.places[0] = tables.len() as u32;
        a.places[1] = points.points.len() as u32;
        tables.extend(points.points.iter().flatten());
        if let Some(highlight) = points.highlight {
            a.places[2] = tables.len() as u32 + 1;
            tables.extend(highlight);
        }
    }
    if let Some(grey) = &plan.monochrome {
        flags |= MONO;
        a.more[0] = tables.len() as u32;
        tables.extend(grey);
    }
    if let Some((tints, (bend, reach))) = &plan.grading {
        flags |= GRADE;
        a.places[3] = tables.len() as u32;
        tables.extend(tints.iter().flatten());
        tables.extend([*bend, *reach, 0.0, 0.0]);
    }
    a.flags[0] = flags;
    a
}

fn mask_of(mask: &MaskPlan, field_at: u32, tables: &mut Vec<f32>) -> Adjust {
    let mut a = adjust_of(&mask.adjustments, tables);
    if let Some([r, g, b]) = mask.gains {
        a.flags[0] |= GAINS;
        a.gains = [r, g, b, 0.0];
    }
    if let Some((amount, radius)) = mask.denoise_colour {
        a.flags[0] |= MASK_DENOISE;
        a.gains[3] = amount;
        a.more[3] = radius;
    }
    if let Some(curves) = &mask.curves {
        a.flags[0] |= CURVES;
        a.more[1] = tables.len() as u32;
        for (k, curve) in curves.iter().enumerate() {
            match curve {
                Some(lookup) => {
                    a.flags[0] |= CURVE_ONE << k;
                    tables.extend(lookup);
                }
                None => tables.extend([0.0; numa_core::curve::LOOKUP]),
            }
        }
    }
    if let Some(([r, g, b], strength)) = mask.tint {
        a.flags[0] |= TINT;
        a.tint = [r, g, b, strength];
    }
    if let Some([strength, cell, rough]) = mask.grain {
        a.flags[0] |= MASK_GRAIN;
        a.grain = [strength, cell, rough, 0.0];
    }
    a.more[2] = field_at;
    a
}

fn packed(field: &[f32]) -> Vec<u32> {
    use rayon::prelude::*;
    let weight = |w: f32| (w.clamp(0.0, 1.0) * 65535.0 + 0.5) as u32;
    field.par_chunks(2).map(|pair| weight(pair[0]) | weight(pair.get(1).copied().unwrap_or(0.0)) << 16).collect()
}

fn pack(plan: &Plan, groups_x: u32, source_groups_x: u32, stride_px: u32, overlay: Overlay, fields_at: &[u32]) -> (Params, Vec<f32>, Vec<Adjust>, u64) {
    let mut p: Params = bytemuck::Zeroable::zeroed();
    let mut tables: Vec<f32> = vec![0.0; 4];
    let mut flags = 0;
    if let Some(camera) = &plan.camera {
        flags |= CAMERA;
        p.mult = [camera.multipliers[0], camera.multipliers[1], camera.multipliers[2], camera.lowest];
        if let Some(clip) = camera.clip {
            flags |= CLIP;
            p.misc[0] = clip;
        }
        match &camera.profile {
            CameraProfileStage::Rendering { to_prophoto, map, look, to_srgb } => {
                flags |= RENDERING;
                p.m_in = rows(*to_prophoto);
                p.m_out = rows(*to_srgb);
                if let Some(map) = map {
                    flags |= MAP;
                    (p.out_info[2], p.map_dims) = put(map, &mut tables);
                    p.whites[0] = map.white;
                }
                if let Some(look) = look {
                    flags |= LOOK;
                    (p.out_info[3], p.look_dims) = put(look, &mut tables);
                    p.whites[1] = look.white;
                }
            }
            CameraProfileStage::Matrix(matrix) => p.m_in = rows(*matrix),
        }
    }

    let colour_key = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (flags, bytemuck::bytes_of(&p), bytemuck::cast_slice::<f32, u8>(&tables)).hash(&mut hasher);
        hasher.finish()
    };
    if let Some((amount, radius)) = plan.denoise_colour {
        flags |= DENOISE;
        p.misc[1] = amount;
        p.misc[2] = radius as f32;
    }
    p.misc[3] = plan.toe;
    p.tone_axis = [numa_render::card::TONE_LOW, numa_render::card::TONE_STEP, 0.0, 0.0];
    p.adjust = adjust_of(&plan.adjustments, &mut tables);
    let masks: Vec<Adjust> = plan.masks.iter().zip(fields_at).map(|(mask, at)| mask_of(mask, *at, &mut tables)).collect();
    if !masks.is_empty() {
        flags |= MASKS;
    }
    if plan.denoise_luma.is_some() {
        flags |= LUMA;
    }
    if plan.dehaze.is_some() {
        flags |= DEHAZE;
    }
    if plan.prefix_kept() {
        flags |= KEPT;
    }
    if plan.local.is_some() {
        flags |= LOCAL;
    }
    if let Some([stops, aspect, circle, power, start, soft, corner]) = plan.vignette {
        flags |= VIGNETTE;
        p.vignette = [[stops, aspect, circle, power], [start, soft, corner, 0.0]];
    }
    if let Some([strength, cell, rough]) = plan.grain {
        flags |= GRAIN;
        p.grain[0] = [strength, cell, rough, 0.0];
    }
    p.grain[1] = [plan.full[0], plan.full[1], 0.0, 0.0];
    p.curves_at[0] = tables.len() as u32;
    for (k, curve) in plan.curves.iter().enumerate() {
        match curve {
            Some(lookup) => {
                p.curves[k] = 1;
                tables.extend(lookup);
            }
            None => tables.extend([0.0; numa_core::curve::LOOKUP]),
        }
    }
    if plan.display_referred {
        flags |= DISPLAY_REFERRED;
    }
    if plan.agx {
        flags |= AGX;
    }
    for (k, value) in plan.base_curve.iter().enumerate() {
        p.base[k / 4][k % 4] = *value;
    }
    let mut turn = 0;
    if let Some(geometry) = &plan.geometry {
        flags |= GEOMETRY;
        let (transpose, flip_x, flip_y) = geometry.turn;
        turn = u32::from(geometry.mirror) * MIRROR | u32::from(transpose) * TRANSPOSE | u32::from(flip_x) * FLIP_X | u32::from(flip_y) * FLIP_Y;
        if let Some(crop) = &geometry.crop {
            turn |= CROP;
            p.crop = [crop.size.0, crop.size.1, crop.centre.0, crop.centre.1];
            p.crop2 = [crop.half.0, crop.half.1, crop.sin, crop.cos];
            p.keystone = [crop.keystone.0, crop.keystone.1, crop.stretch, 0.0];
        }
    }
    p.size = [plan.width, plan.height, flags, groups_x];
    p.src = [plan.source.0, plan.source.1, source_groups_x, turn];
    p.out_info[0] = stride_px;
    p.out_info[1] = overlay;
    (p, tables, masks, colour_key)
}

pub(crate) struct State {
    layout: wgpu::BindGroupLayout,
    colour: wgpu::ComputePipeline,
    blur_rows: wgpu::ComputePipeline,
    geometry: wgpu::ComputePipeline,
    finish: wgpu::ComputePipeline,
    mask_rows: wgpu::ComputePipeline,
    mask_copy: wgpu::ComputePipeline,
    mask: wgpu::ComputePipeline,
    encode: wgpu::ComputePipeline,
    plane_rows: wgpu::ComputePipeline,
    plane_cols: wgpu::ComputePipeline,
    luma_log: wgpu::ComputePipeline,
    luma_apply: wgpu::ComputePipeline,
    subsample: wgpu::ComputePipeline,
    pivot_sum: wgpu::ComputePipeline,
    pivot_total: wgpu::ComputePipeline,
    tone_apply: wgpu::ComputePipeline,
    haze_patches: wgpu::ComputePipeline,
    haze_air: wgpu::ComputePipeline,
    haze_apply: wgpu::ComputePipeline,
    settle: wgpu::ComputePipeline,
    mask_settle: wgpu::ComputePipeline,
    tail_log: wgpu::ComputePipeline,
    sharpen_apply: wgpu::ComputePipeline,
    defringe_apply: wgpu::ComputePipeline,
    moire_apply: wgpu::ComputePipeline,

    resident: Mutex<Vec<Resident>>,

    work: Mutex<Option<Work>>,
    #[cfg(target_os = "linux")]
    pub(crate) display: Mutex<Option<crate::display::Pool>>,
}

pub(crate) fn build(device: &wgpu::Device) -> Option<State> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("render"),
        source: wgpu::ShaderSource::Wgsl(include_str!("render.wgsl").into()),
    });
    let entry = |binding: u32, ty: wgpu::BindingType| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE, ty, count: None };
    let storage = |read_only| wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render"),
        entries: &[
            entry(0, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }),
            entry(1, storage(true)),
            entry(2, storage(false)),
            entry(3, storage(true)),
            entry(4, storage(false)),
            entry(5, storage(false)),
            entry(6, storage(false)),
            entry(7, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Step>() as u64) }),
            entry(8, storage(true)),
            entry(9, storage(true)),
            entry(10, storage(false)),
            entry(11, storage(false)),
            entry(12, storage(false)),
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("render"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
    let pipeline = |name: &str| {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(name),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some(name),
            compilation_options: Default::default(),
            cache: None,
        })
    };
    let (colour, geometry, blur_rows, finish) = (pipeline("colour"), pipeline("geometry"), pipeline("blur_rows"), pipeline("finish"));
    let (mask_rows, mask_copy, mask, encode) = (pipeline("mask_rows"), pipeline("mask_copy"), pipeline("mask"), pipeline("encode"));
    let (plane_rows, plane_cols, luma_log, luma_apply) = (pipeline("plane_rows"), pipeline("plane_cols"), pipeline("luma_log"), pipeline("luma_apply"));
    let (subsample, pivot_sum, pivot_total, tone_apply) = (pipeline("subsample"), pipeline("pivot_sum"), pipeline("pivot_total"), pipeline("tone_apply"));
    let (haze_patches, haze_air, haze_apply) = (pipeline("haze_patches"), pipeline("haze_air"), pipeline("haze_apply"));
    let (settle, mask_settle, tail_log) = (pipeline("settle"), pipeline("mask_settle"), pipeline("tail_log"));
    let (sharpen_apply, defringe_apply, moire_apply) = (pipeline("sharpen_apply"), pipeline("defringe_apply"), pipeline("moire_apply"));
    if let Some(error) = block_on(scope.pop()) {
        log::warn!("GPU: the render did not build ({error}); rendering on the processor");
        return None;
    }
    Some(State {
        layout,
        colour,
        blur_rows,
        geometry,
        finish,
        mask_rows,
        mask_copy,
        mask,
        encode,
        plane_rows,
        plane_cols,
        luma_log,
        luma_apply,
        subsample,
        pivot_sum,
        pivot_total,
        tone_apply,
        haze_patches,
        haze_air,
        haze_apply,
        settle,
        mask_settle,
        tail_log,
        sharpen_apply,
        defringe_apply,
        moire_apply,
        resident: Mutex::new(Vec::new()),
        work: Mutex::new(None),
        #[cfg(target_os = "linux")]
        display: Mutex::new(None),
    })
}

struct Resident {
    key: u64,
    buffer: wgpu::Buffer,
    used: u64,
}

const RESIDENT: usize = 4;

static TICK: AtomicU64 = AtomicU64::new(0);

fn key_of(width: u32, height: u32, data: &[f32]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64 ^ (u64::from(width) << 32 | u64::from(height));
    for value in data.iter().step_by(997).chain(data.last()) {
        hash = (hash ^ u64::from(value.to_bits())).wrapping_mul(0x100_0000_01b3);
    }
    hash
}

pub(crate) fn keep(gpu: &Gpu, width: u32, height: u32, data: &[f32], buffer: wgpu::Buffer) {
    let Some(state) = gpu.render.as_ref().filter(|_| render_enabled()) else {
        buffer.destroy();
        return;
    };
    let key = key_of(width, height, data);
    remember(state, key, buffer);
}

fn remember(state: &State, key: u64, buffer: wgpu::Buffer) {
    let mut resident = state.resident.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(at) = resident.iter().position(|kept| kept.key == key) {
        resident.remove(at).buffer.destroy();
    }
    while resident.len() >= RESIDENT {
        let oldest = (0..resident.len()).min_by_key(|&at| resident[at].used).expect("not empty");
        resident.remove(oldest).buffer.destroy();
    }
    resident.push(Resident { key, buffer, used: TICK.fetch_add(1, Ordering::Relaxed) });
}

fn proxy_on_card(gpu: &Gpu, state: &State, proxy: &LinearImage) -> (u64, wgpu::Buffer) {
    let key = key_of(proxy.width, proxy.height, &proxy.data);
    {
        let mut resident = state.resident.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(kept) = resident.iter_mut().find(|kept| kept.key == key) {
            kept.used = TICK.fetch_add(1, Ordering::Relaxed);
            return (key, kept.buffer.clone());
        }
    }
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("proxy"),
        size: (proxy.data.len() * 4).max(16) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    gpu.queue.write_buffer(&buffer, 0, bytemuck::cast_slice(&proxy.data));
    remember(state, key, buffer.clone());
    (key, buffer)
}

pub fn forget_proxies() {
    for frugal in [false, true] {
        let gpu = ready(frugal);
        if let Some(state) = gpu.as_ref().and_then(|gpu| gpu.render.as_ref()) {
            for kept in state.resident.lock().unwrap_or_else(std::sync::PoisonError::into_inner).drain(..) {
                kept.buffer.destroy();
            }
            if let Some(work) = state.work.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
                work.destroy();
            }
        }
    }
}

struct Work {
    width: u32,
    height: u32,
    source: (u32, u32),
    params: wgpu::Buffer,
    work: wgpu::Buffer,
    rows: wgpu::Buffer,
    out: wgpu::Buffer,
    hist: wgpu::Buffer,
    tables: wgpu::Buffer,
    back: wgpu::Buffer,

    steps: wgpu::Buffer,

    planes: wgpu::Buffer,
    masks: wgpu::Buffer,
    fields: wgpu::Buffer,
    slots: Vec<Option<u64>>,

    lin: wgpu::Buffer,
    lin_key: Option<u64>,

    kept: wgpu::Buffer,
    kept_key: Option<u64>,
}

impl Work {
    fn new(device: &wgpu::Device, (width, height): (u32, u32), source: (u32, u32), slots: usize) -> Self {
        use wgpu::BufferUsages as U;
        let buffer = |label: &str, size: u64, usage: wgpu::BufferUsages| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: size.max(16), usage, mapped_at_creation: false });

        let stride_px = width.next_multiple_of(64);

        let pixels = u64::from(source.0) * u64::from(source.1);
        let out = u64::from(stride_px) * u64::from(height) * 4;
        Self {
            width,
            height,
            source,
            params: buffer("render params", std::mem::size_of::<Params>() as u64, U::UNIFORM | U::COPY_DST),
            work: buffer("render work", pixels * 12, U::STORAGE),
            rows: buffer("render rows", pixels * 12, U::STORAGE),
            out: buffer("render out", out, U::STORAGE | U::COPY_SRC),
            hist: buffer("render histogram", HISTOGRAM as u64 * 4, U::STORAGE | U::COPY_SRC | U::COPY_DST),
            tables: buffer("render tables", 1 << 16, U::STORAGE | U::COPY_DST),
            back: buffer("render back", out + HISTOGRAM as u64 * 4, U::MAP_READ | U::COPY_DST),
            steps: buffer("render steps", MAX_STEPS as u64 * u64::from(STEP), U::UNIFORM | U::COPY_DST),
            planes: buffer("render planes", 16, U::STORAGE),
            masks: buffer("render masks", (MAX_MASKS * std::mem::size_of::<Adjust>()) as u64, U::STORAGE | U::COPY_DST),
            fields: buffer("render fields", slots as u64 * field_words(width, height) * 4, U::STORAGE | U::COPY_DST),
            slots: vec![None; slots],
            lin: buffer("render lin", pixels * 12, U::STORAGE),
            lin_key: None,
            kept: buffer("render kept", 16, U::STORAGE),
            kept_key: None,
        }
    }

    fn fits(&self, (width, height): (u32, u32), source: (u32, u32), slots: usize) -> bool {
        (self.width, self.height, self.source) == (width, height, source) && self.slots.len() >= slots
    }

    fn fields_for(&mut self, queue: &wgpu::Queue, masks: &[MaskPlan]) -> Vec<u32> {
        let words = field_words(self.width, self.height);
        let mut slot_of: Vec<Option<usize>> = masks.iter().map(|mask| self.slots.iter().position(|held| *held == Some(mask.key))).collect();
        let mut free = (0..self.slots.len()).filter(|slot| !slot_of.contains(&Some(*slot))).collect::<Vec<_>>().into_iter();
        for (mask, slot) in masks.iter().zip(&mut slot_of) {
            if slot.is_none() {
                let at = free.next().expect("a slot for every mask");
                queue.write_buffer(&self.fields, at as u64 * words * 4, bytemuck::cast_slice(&packed(&mask.field())));
                self.slots[at] = Some(mask.key);
                *slot = Some(at);
            }
        }
        slot_of.into_iter().map(|slot| (slot.expect("placed") as u64 * words) as u32).collect()
    }

    fn destroy(self) {
        for buffer in [self.params, self.work, self.rows, self.out, self.hist, self.tables, self.back, self.steps, self.masks, self.fields, self.lin, self.planes, self.kept] {
            buffer.destroy();
        }
    }
}

type Dispatch<'a> = (&'a wgpu::ComputePipeline, (u32, u32), Step);

fn dispatches<'a>(state: &'a State, plan: &Plan, (colour, kept): (bool, bool), (groups, source_groups): ((u32, u32), (u32, u32)), planes: &Planes) -> Vec<Dispatch<'a>> {
    let mut passes: Vec<Dispatch> = Vec::new();
    let frame = |pipeline, step: Step| (pipeline, groups, step);
    let whole = (plan.width, plan.height);
    if !kept {
        prefix(state, &mut passes, plan, colour, (groups, source_groups), planes);
    }
    passes.push(frame(&state.finish, Step::default()));
    if let Some(local) = &plan.local {
        tone_map(state, &mut passes, whole, groups, local, planes, 0);
    }

    for (index, mask) in plan.masks.iter().enumerate() {
        let m = index as u32;
        let copy = if wide(mask) { planes.copy } else { 0 };
        let mut held = 0;
        if let Some(haze) = &mask.dehaze {
            dehaze(state, &mut passes, groups, haze, planes, [m, 0, 0, copy]);
            held = 1;
        }
        if let Some(luma) = &mask.denoise_luma {
            passes.push(frame(&state.mask_copy, Step { what: [m, 0, held, copy], ..Step::default() }));
            denoise_luma(state, &mut passes, whole, groups, luma, planes, copy);
            held = 1;
        }
        if mask.denoise_colour.is_some() {
            passes.push(frame(&state.mask_rows, Step { what: [m, WHOLE, held, copy], ..Step::default() }));
        }
        if mask.tail.is_some() {
            passes.push(frame(&state.mask_settle, Step { what: [m, WHOLE, held, copy], ..Step::default() }));
            tail(state, &mut passes, whole, groups, &mask.tail, planes, copy);
            held = SETTLED;
        }
        let step = |phase| Step { what: [m, phase, held, copy], ..Step::default() };
        match &mask.local {
            None => passes.push(frame(&state.mask, step(WHOLE))),
            Some(local) => {
                passes.push(frame(&state.mask, step(TO_TONE)));
                tone_map(state, &mut passes, whole, groups, local, planes, copy);
                passes.push(frame(&state.mask, step(FROM_TONE)));
            }
        }
    }
    if !plan.masks.is_empty() || plan.local.is_some() {
        passes.push(frame(&state.encode, Step::default()));
    }
    passes
}

fn prefix<'a>(state: &'a State, passes: &mut Vec<Dispatch<'a>>, plan: &Plan, colour: bool, (groups, source_groups): ((u32, u32), (u32, u32)), planes: &Planes) {
    let whole = (plan.width, plan.height);
    if colour {
        passes.push((&state.colour, source_groups, Step::default()));
    }
    if plan.geometry.is_some() {
        passes.push((&state.geometry, groups, Step::default()));
    }
    if let Some(haze) = &plan.dehaze {
        dehaze(state, passes, groups, haze, planes, [0; 4]);
    }
    if let Some(luma) = &plan.denoise_luma {
        passes.push((&state.luma_log, groups, Step::default()));
        denoise_luma(state, passes, whole, groups, luma, planes, 0);
    }
    if plan.denoise_colour.is_some() {
        passes.push((&state.blur_rows, groups, Step::default()));
    }
    if plan.prefix_kept() {
        passes.push((&state.settle, groups, Step::default()));
        tail(state, passes, whole, groups, &plan.tail, planes, 0);
    }
}

fn wide(mask: &MaskPlan) -> bool {
    mask.dehaze.is_some() || mask.denoise_luma.is_some() || mask.tail.is_some() || mask.local.is_some()
}

fn tail<'a>(state: &'a State, passes: &mut Vec<Dispatch<'a>>, whole: (u32, u32), groups: (u32, u32), tail: &Tail, planes: &Planes, copy: u32) {
    let what = [0, 0, 0, copy];
    let blur = |passes: &mut Vec<Dispatch<'a>>, radius: u32, into: u32| {
        passes.push(box_rows(state, whole, radius, FEED_ONE, [planes.log, planes.pair, 0, 0]));
        passes.push(box_cols(state, whole, radius, EMIT_ONE, [planes.pair, into, 0, 0], 0.0));
    };
    if let Some(sharpen) = &tail.sharpen {
        passes.push((&state.tail_log, groups, Step { what, ..Step::default() }));
        let (below, mixed) = (sharpen.below as u32, sharpen.t >= 1e-3);
        if below >= 1 {
            blur(passes, below, planes.mid);
        }
        if mixed {
            blur(passes, below + 1, planes.ab);
        }
        let step = Step { what, at: [planes.log, planes.mid, planes.ab, 0], shape: [below, u32::from(mixed), 0, 0], numbers: [sharpen.amount, sharpen.t, sharpen.floor, 0.0], ..Step::default() };
        passes.push((&state.sharpen_apply, groups, step));
    }
    if let Some((amount, radius)) = tail.defringe {
        passes.push((&state.tail_log, groups, Step { what, ..Step::default() }));
        blur(passes, radius, planes.mid);
        passes.push((&state.defringe_apply, groups, Step { what, at: [0, planes.mid, 0, 0], numbers: [amount, 0.0, 0.0, 0.0], ..Step::default() }));
    }
    if let Some((amount, near, far)) = tail.moire {
        let n = whole.0 * whole.1;

        for (radius, into) in [(near, planes.ab), (far, planes.wide)] {
            for channel in 0..3 {
                passes.push(plane(&state.plane_rows, whole, n, radius, [channel, FEED_CHROMA, 0, copy], [0, planes.pair, 0, 0], 0.0));
                passes.push(box_cols(state, whole, radius, EMIT_ONE, [planes.pair, into + channel * n, 0, 0], 0.0));
            }
        }
        passes.push((&state.moire_apply, groups, Step { what, at: [0, planes.ab, planes.wide, 0], numbers: [amount, 0.0, 0.0, 0.0], ..Step::default() }));
    }
}

fn dehaze<'a>(state: &'a State, passes: &mut Vec<Dispatch<'a>>, groups: (u32, u32), haze: &HazeShape, planes: &Planes, what: [u32; 4]) {
    let (grid_w, grid_h) = (haze.grid.0 as u32, haze.grid.1 as u32);
    let step = Step { what, at: [planes.haze, 0, 0, 0], shape: [grid_w, grid_h, haze.cell as u32, haze.take as u32], numbers: [haze.strength, 0.0, 0.0, 0.0], ..Step::default() };
    passes.push((&state.haze_patches, (grid_w, grid_h), step));
    passes.push((&state.haze_air, (1, 1), step));
    passes.push((&state.haze_apply, groups, step));
}

fn denoise_luma<'a>(state: &'a State, passes: &mut Vec<Dispatch<'a>>, whole: (u32, u32), groups: (u32, u32), luma: &LumaShape, planes: &Planes, copy: u32) {
    guided(state, passes, whole, luma.radius as u32, luma.floor, [planes.log, planes.pair, planes.ab, planes.mid]);
    if luma.contrast > 0.0 {
        let radius = 2 * luma.radius as u32;
        passes.push(box_rows(state, whole, radius, FEED_DIFFERENCE, [planes.log, planes.pair, 0, planes.mid]));
        passes.push(box_cols(state, whole, radius, EMIT_ONE, [planes.pair, planes.ab, 0, 0], 0.0));
    }
    let step = Step { what: [0, 0, 0, copy], at: [0, planes.mid, planes.ab, 0], numbers: [luma.contrast, luma.luminance, 0.0, 0.0], ..Step::default() };
    passes.push((&state.luma_apply, groups, step));
}

fn tone_map<'a>(state: &'a State, passes: &mut Vec<Dispatch<'a>>, whole: (u32, u32), groups: (u32, u32), local: &ToneShape, planes: &Planes, copy: u32) {
    let small = planes.small;
    let factor = ToneShape::SUBSAMPLE as u32;
    let epsilon = ToneShape::EPSILON;
    passes.push(plane(&state.subsample, small, small.0 * small.1, factor, [0; 4], [planes.log, planes.s, 0, 0], 0.0));
    guided(state, passes, small, local.small_radius as u32, epsilon, [planes.s, planes.pair_s, planes.ab_s, planes.base_s]);
    if let Some(radius) = local.bloom_radius {
        passes.push(box_rows(state, small, radius as u32, FEED_ONE, [planes.s, planes.pair_s, 0, 0]));
        passes.push(box_cols(state, small, radius as u32, EMIT_ONE, [planes.pair_s, planes.bloom_s, 0, 0], 0.0));
    }
    if local.texture {
        let radius = ToneShape::texture_radius(whole.0.max(whole.1) as f32) as u32;
        guided(state, passes, whole, radius, epsilon, [planes.log, planes.pair, planes.ab, planes.mid]);
    }
    passes.push((&state.pivot_sum, groups, Step { at: [planes.base_s, planes.partials, 0, 0], shape: [small.0, small.1, 0, 0], ..Step::default() }));
    passes.push((&state.pivot_total, (1, 1), Step { at: [planes.partials, planes.pivot, 0, 0], shape: [groups.0 * groups.1, 0, 0, 0], ..Step::default() }));
    let more = [if local.bloom_radius.is_some() { planes.bloom_s } else { 0 }, if local.texture { planes.mid } else { 0 }, planes.pivot, 0];
    let numbers = [local.base_scale, local.detail_scale, local.glow, local.texture_scale];
    passes.push((&state.tone_apply, groups, Step { what: [0, 0, 0, copy], at: [planes.base_s, 0, 0, 0], shape: [small.0, small.1, 0, 0], more, numbers }));
}

fn box_rows(state: &State, (width, height): (u32, u32), radius: u32, feed: u32, at: [u32; 4]) -> Dispatch<'_> {
    plane(&state.plane_rows, (width, height), width * height, radius, [0, feed, 0, 0], at, 0.0)
}

fn box_cols(state: &State, (width, height): (u32, u32), radius: u32, emit: u32, at: [u32; 4], epsilon: f32) -> Dispatch<'_> {
    plane(&state.plane_cols, (width, height), width * height.div_ceil(SEGMENT), radius, [0, 0, emit, 0], at, epsilon)
}

const SEGMENT: u32 = 64;

const HAZE_TAKE: usize = 16;

fn plane(pipeline: &wgpu::ComputePipeline, (width, height): (u32, u32), invocations: u32, radius: u32, what: [u32; 4], at: [u32; 4], epsilon: f32) -> Dispatch<'_> {
    let blocks = invocations.div_ceil(256);
    let groups = (blocks.min(32768), blocks.div_ceil(32768));
    (pipeline, groups, Step { what, at, shape: [width, height, radius, groups.0], numbers: [epsilon, 0.0, 0.0, 0.0], ..Step::default() })
}

fn guided<'a>(state: &'a State, passes: &mut Vec<Dispatch<'a>>, size: (u32, u32), radius: u32, epsilon: f32, [source, pair, ab, out]: [u32; 4]) {
    passes.push(box_rows(state, size, radius, FEED_SQUARES, [source, pair, 0, 0]));
    passes.push(box_cols(state, size, radius, EMIT_AB, [pair, ab, 0, 0], epsilon));
    passes.push(box_rows(state, size, radius, FEED_PAIR, [ab, pair, 0, 0]));
    passes.push(box_cols(state, size, radius, EMIT_GUIDED, [pair, out, source, 0], epsilon));
}

fn field_words(width: u32, height: u32) -> u64 {
    (u64::from(width) * u64::from(height)).div_ceil(2)
}

const WAIT: wgpu::PollType = wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(2)) };

fn read_back(gpu: &Gpu, back: &wgpu::Buffer, out_bytes: u64, read_frame: bool) -> Result<([u32; HISTOGRAM], Option<Vec<u8>>), String> {
    let range = if read_frame { 0..out_bytes + HISTOGRAM as u64 * 4 } else { out_bytes..out_bytes + HISTOGRAM as u64 * 4 };
    let (sender, receiver) = std::sync::mpsc::channel();
    back.map_async(wgpu::MapMode::Read, range.clone(), move |result| {
        let _ = sender.send(result);
    });
    gpu.device.poll(WAIT).map_err(|err| format!("the card did not finish: {err}"))?;
    receiver.recv().map_err(|_| "no answer from the card".to_string())?.map_err(|err| err.to_string())?;
    let mut histogram = [0u32; HISTOGRAM];
    let bytes = {
        let view = back.get_mapped_range(range).map_err(|err| err.to_string())?;
        let (frame, hist) = view.split_at(view.len() - HISTOGRAM * 4);
        histogram.copy_from_slice(bytemuck::cast_slice(hist));
        read_frame.then(|| {
            use rayon::prelude::*;
            let mut bytes = vec![0u8; frame.len()];
            bytes.par_chunks_mut(1 << 20).zip(frame.par_chunks(1 << 20)).for_each(|(to, from)| to.copy_from_slice(from));
            bytes
        })
    };
    back.unmap();
    Ok((histogram, bytes))
}

fn run(gpu: &Gpu, state: &State, proxy: &LinearImage, plan: &Plan, overlay: Overlay, output: Output) -> Result<Rendered, String> {
    let (width, height) = (plan.width, plan.height);
    let source = plan.source;
    let source_pixels = u64::from(source.0) * u64::from(source.1);
    if (proxy.width, proxy.height) != source || proxy.data.len() as u64 != source_pixels * 3 || width == 0 || height == 0 {
        return Err("the plan is for another frame".into());
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > source_pixels || source_pixels * 12 > gpu.max_binding {
        return Err("a frame larger than the card binds".into());
    }
    let groups_of = |pixels: u64| {
        let blocks = pixels.div_ceil(256) as u32;
        (blocks.min(32768), blocks.div_ceil(32768))
    };
    let (groups, source_groups) = (groups_of(pixels), groups_of(source_pixels));
    let (proxy_key, src) = proxy_on_card(gpu, state, proxy);

    if plan.masks.len() > MAX_MASKS {
        return Err("more masks than the card takes".into());
    }
    let mut kept = state.work.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let stride_px = width.next_multiple_of(64);

    let slots = plan.masks.len().next_power_of_two();
    if !kept.as_ref().is_some_and(|work| work.fits((width, height), source, slots)) {
        if let Some(old) = kept.take() {
            old.destroy();
        }
        *kept = Some(Work::new(&gpu.device, (width, height), source, slots));
    }
    let work = kept.as_mut().expect("made above");
    let fields_at = work.fields_for(&gpu.queue, &plan.masks);
    let (params, tables, masks, colour_key) = pack(plan, groups.0, source_groups.0, stride_px, overlay, &fields_at);
    let table_bytes = (tables.len() * 4) as u64;
    if work.tables.size() < table_bytes {
        work.tables.destroy();
        work.tables = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("render tables"), size: table_bytes.next_power_of_two(), usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    }

    let colour_key = proxy_key ^ colour_key.rotate_left(1);

    let prefix_key = plan.prefix_key().map(|key| key ^ colour_key.rotate_left(7));
    let kept_hit = prefix_key.is_some() && work.kept_key == prefix_key;
    let colour = !kept_hit && work.lin_key != Some(colour_key);
    if colour {
        work.lin_key = None;
    }
    if prefix_key.is_some() && !kept_hit {
        work.kept_key = None;
        let bytes = u64::from(width) * u64::from(height) * 12;
        if work.kept.size() < bytes {
            work.kept.destroy();
            work.kept = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("render kept"), size: bytes, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
        }
    }

    if plan.dehaze.iter().chain(plan.masks.iter().filter_map(|mask| mask.dehaze.as_ref())).any(|haze| haze.take > HAZE_TAKE) {
        return Err("more hazy patches than the card picks".into());
    }
    let planes = Planes::of(plan, groups.0 * groups.1);
    let dispatches = dispatches(state, plan, (colour, kept_hit), (groups, source_groups), &planes);
    if dispatches.len() > MAX_STEPS {
        return Err("more passes than the card takes".into());
    }
    let plane_bytes = u64::from(planes.len) * 4;
    if work.planes.size() < plane_bytes {
        if plane_bytes > gpu.max_binding {
            return Err("planes larger than the card binds".into());
        }
        work.planes.destroy();
        work.planes = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("render planes"), size: plane_bytes, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
    }
    let work = &*work;
    gpu.queue.write_buffer(&work.params, 0, bytemuck::bytes_of(&params));
    gpu.queue.write_buffer(&work.tables, 0, bytemuck::cast_slice(&tables));
    if !masks.is_empty() {
        gpu.queue.write_buffer(&work.masks, 0, bytemuck::cast_slice(&masks));
    }

    let buffers = [&work.params, &src, &work.work, &work.tables, &work.rows, &work.out, &work.hist];
    let mut entries: Vec<_> = buffers
        .iter()
        .enumerate()
        .map(|(binding, buffer)| wgpu::BindGroupEntry { binding: binding as u32, resource: buffer.as_entire_binding() })
        .collect();
    entries.push(wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &work.steps, offset: 0, size: wgpu::BufferSize::new(std::mem::size_of::<Step>() as u64) }) });
    entries.push(wgpu::BindGroupEntry { binding: 8, resource: work.masks.as_entire_binding() });
    entries.push(wgpu::BindGroupEntry { binding: 9, resource: work.fields.as_entire_binding() });
    entries.push(wgpu::BindGroupEntry { binding: 10, resource: work.lin.as_entire_binding() });
    entries.push(wgpu::BindGroupEntry { binding: 11, resource: work.planes.as_entire_binding() });
    entries.push(wgpu::BindGroupEntry { binding: 12, resource: work.kept.as_entire_binding() });
    let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("render"), layout: &state.layout, entries: &entries });

    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.clear_buffer(&work.hist, 0, None);
    {
        let steps: Vec<u8> = dispatches.iter().flat_map(|(_, _, step)| bytemuck::bytes_of(step).iter().copied().chain(std::iter::repeat_n(0, STEP as usize - std::mem::size_of::<Step>()))).collect();
        gpu.queue.write_buffer(&work.steps, 0, &steps);
        let mut pass = encoder.begin_compute_pass(&Default::default());
        for (k, (pipeline, groups, _)) in dispatches.iter().enumerate() {
            pass.set_bind_group(0, &group, &[k as u32 * STEP]);
            pass.set_pipeline(pipeline);
            pass.dispatch_workgroups(groups.0, groups.1, 1);
        }
    }
    let out_bytes = u64::from(stride_px) * u64::from(height) * 4;

    #[cfg(target_os = "linux")]
    let shown = match output {
        Output::Dmabuf => crate::display::show(gpu, state, &mut encoder, &work.out, width, height, stride_px)
            .map_err(|err| log::debug!("render: {err}; reading back"))
            .ok(),
        Output::ReadBack => None,
    };
    #[cfg(not(target_os = "linux"))]
    let (shown, _) = (None::<()>, output);
    let read_frame = shown.is_none();
    if read_frame {
        encoder.copy_buffer_to_buffer(&work.out, 0, &work.back, 0, out_bytes);
    }
    encoder.copy_buffer_to_buffer(&work.hist, 0, &work.back, out_bytes, HISTOGRAM as u64 * 4);
    gpu.queue.submit([encoder.finish()]);

    let (histogram, bytes) = read_back(gpu, &work.back, out_bytes, read_frame)?;
    let pixels = match bytes {
        Some(bytes) => Pixels::Rgba { bytes, stride: stride_px as usize * 4 },
        #[cfg(target_os = "linux")]
        None => Pixels::Dmabuf(shown.expect("no read back, so a dmabuf")),
        #[cfg(not(target_os = "linux"))]
        None => unreachable!("only a read back off Linux"),
    };
    if let Some(work) = kept.as_mut() {

        if !kept_hit {
            work.lin_key = Some(colour_key);
        }
        if prefix_key.is_some() {
            work.kept_key = prefix_key;
        }
    }
    Ok(Rendered { width, height, histogram, pixels })
}
