use crate::buffers::{part, Buffers, Frame};
use crate::{block_on, ready, Gpu};
use numa_core::lens::LensProfile;
use rayon::prelude::*;
use std::time::Instant;

pub struct Mosaic<'a> {

    pub pixels: &'a [u16],
    pub width: usize,
    pub height: usize,

    pub black: [f32; 4],
    pub white: [f32; 4],

    pub cfa: [[u8; 6]; 6],

    pub xtrans: bool,

    pub active: [usize; 4],

    pub crop: [usize; 4],
}

pub struct Job<'a> {
    pub mosaic: Mosaic<'a>,
    pub baseline: f32,
    pub lens: Option<&'a LensProfile>,

    pub vignetting: bool,

    pub false_colour: Option<f32>,

    pub flips: (bool, bool, bool),

    pub proxy: Option<u32>,

    pub stride: usize,

    pub may_round: bool,
}

pub struct Developed {
    pub width: u32,
    pub height: u32,

    pub data: Vec<f32>,

    pub full: (u32, u32),

    pub factor: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Params {
    raw_w: u32,
    raw_h: u32,
    scaled_w: u32,
    scaled_h: u32,
    roi: [u32; 4],
    crop: [u32; 4],
    levels: [u32; 4],
    spare4: [u32; 4],
    baseline: f32,
    fc_black: f32,
    factor: f32,
    flags: u32,
    centre_x: f32,
    centre_y: f32,
    half_diag: f32,
    fit: f32,
    n_radii: u32,
    sgrow: u32,
    sgcol: u32,
    band_n: u32,
    out_w: u32,
    out_h: u32,
    stride: u32,
    n_samples: u32,
    t_box: u32,
    tw_from: u32,
    tw_odd: u32,
    spare: u32,
}

unsafe impl bytemuck::Zeroable for Params {}
unsafe impl bytemuck::Pod for Params {}

const VIGNETTING: u32 = 1;
const GEOMETRY: u32 = 2;
const TRANSPOSE: u32 = 8;
const FLIP_X: u32 = 16;
const FLIP_Y: u32 = 32;
const FINISHED_BENT: u32 = 64;
const T_LENS: usize = 180;

const PAD: usize = 12;

const BUDGET: u64 = 3 << 30;

fn budget() -> u64 {
    crate::env_bytes("NUMA_GPU_BUDGET")
        .unwrap_or_else(|| numa_core::power::available_memory().map_or(BUDGET, |room| BUDGET.min(room / 3)))
}

const WAIT: wgpu::PollType = wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(10)) };

pub fn develop(job: &Job, frugal: bool, exposure: impl FnOnce(&[f32]) -> f32) -> Result<Developed, String> {
    let gpu = ready(frugal).ok_or("no card ready")?;
    let gpu = &*gpu;
    let memory = gpu.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(gpu, job, exposure)));
    let invalid = block_on(validation.pop());
    let out_of_memory = block_on(memory.pop());

    let _ = gpu.device.poll(wgpu::PollType::Poll);
    match (result, out_of_memory, invalid) {
        (_, Some(error), _) | (_, _, Some(error)) => Err(format!("the card: {error}")),
        (Err(_), _, _) => Err("the card's develop panicked".into()),
        (Ok(result), None, None) => result,
    }
}

struct Laps {
    at: Option<Instant>,
    parts: Vec<(&'static str, f32)>,
}

impl Laps {
    fn lap(&mut self, gpu: &Gpu, name: &'static str) {
        if let Some(at) = self.at.as_mut() {
            let _ = gpu.device.poll(WAIT);
            let ms = at.elapsed().as_secs_f32() * 1000.0;
            match self.parts.iter_mut().find(|(part, _)| *part == name) {
                Some((_, total)) => *total += ms,
                None => self.parts.push((name, ms)),
            }
            *at = Instant::now();
        }
    }
}

struct Dispatch {
    entry: &'static str,
    record: usize,
    groups: (u32, u32),
}

struct Plan {
    table: Vec<u32>,
    params: Params,

    records: Vec<[u32; 4]>,

    develop: Vec<Vec<Dispatch>>,
    output: Vec<Vec<Dispatch>>,

    chunks: Vec<Chunk>,
    n_samples: usize,
    out: (u32, u32),
    full: (u32, u32),
    proxy: bool,

    frame: Frame,
}

fn plan(gpu: &Gpu, job: &Job, budget: u64) -> Result<Plan, String> {
    let m = &job.mosaic;
    let [ax, ay, aw, ah] = m.active;
    let [cx, cy, cw, ch] = m.crop;
    if aw < 32 || ah < 32 || ax + aw > m.width || ay + ah > m.height || cx + cw > aw || cy + ch > ah || cw < 3 || ch < 3 {
        return Err("a frame the card does not handle".into());
    }
    if m.pixels.len() < m.width * m.height || job.lens.is_some_and(|lens| lens.radii.len() > 64) {
        return Err("a frame the card does not handle".into());
    }

    let (transpose, flip_x, flip_y) = job.flips;
    let full = if transpose { (ch as u32, cw as u32) } else { (cw as u32, ch as u32) };
    let proxy = job.proxy.and_then(|edge| numa_core::image::proxy_size(full.0, full.1, edge));
    let out = proxy.unwrap_or(full);
    let bends = job.lens.is_some_and(|lens| lens.bends_anything());
    let crop_n = cw * ch;

    let mut table: Vec<u32> = m.cfa.iter().flatten().map(|&colour| u32::from(colour)).collect();
    let (hexagon, sgrow, sgcol) = hexagon(&m.cfa);
    table.extend(hexagon.iter().map(|&offset| offset as u32));
    debug_assert_eq!(table.len(), T_LENS);
    let n_radii = lens_tables(job.lens, &mut table);
    let t_box = table.len() as u32;
    let mut widest_box = 1;
    if let Some((w, h)) = proxy {
        let (columns, rows) = (numa_core::image::box_edges(full.0, w), numa_core::image::box_edges(full.1, h));
        widest_box = columns.iter().chain(&rows).map(|(start, end)| end - start).max().unwrap_or(1) as usize;
        table.extend(columns.iter().map(|edge| edge.0));
        table.extend(columns.iter().map(|edge| edge.1));
        table.extend(rows.iter().map(|edge| edge.0));
        table.extend(rows.iter().map(|edge| edge.1));
    }
    if widest_box > 64 {
        return Err("a proxy too small for the card's boxes".into());
    }
    let levels = level_tables(m, &mut table);

    let tile = gpu.tile_pixels;
    let rows_of = |width: usize, cost: usize| (tile / (width * cost).max(1)).clamp(16, 4096);
    let mut records = Vec::new();

    let band_rows = match m.xtrans {
        true => {
            let most = (gpu.max_binding / (48 * aw as u64)).min(budget / 4 / (68 * aw as u64)) as usize;
            rows_of(aw, 1).min(ah).min(most.saturating_sub(2 * PAD)).max(16) + 2 * PAD
        }
        false => 0,
    };
    let mut develop = tiled(&mut records, "scale", ah, rows_of(aw, 1), aw);
    develop.extend(match m.xtrans {
        true => (cy..cy + ch)
            .step_by(band_rows - 2 * PAD)
            .map(|y0| {
                let y1 = (y0 + band_rows - 2 * PAD).min(cy + ch);
                let w0 = y0.saturating_sub(PAD);
                let rows = (y1 + PAD).min(ah) - w0;
                records.push([y0, y1, w0, rows].map(|v| v as u32));
                let record = records.len() - 1;
                let mut band: Vec<Dispatch> = ["mk_green", "mk_solitary", "mk_rb", "mk_blocks", "mk_drv", "mk_homo"]
                    .into_iter()
                    .map(|entry| Dispatch { entry, record, groups: (aw.div_ceil(16) as u32, rows.div_ceil(16) as u32) })
                    .collect();
                band.push(Dispatch { entry: "mk_final", record, groups: (cw.div_ceil(16) as u32, (y1 - y0).div_ceil(16) as u32) });
                band
            })
            .collect(),
        false => {
            let mut steps = tiled(&mut records, "ppg_green", ah, rows_of(aw, 1), aw);
            steps.extend(tiled(&mut records, "ppg_rb", ch, rows_of(cw, 1), cw));
            steps
        }
    });
    if bends {
        develop.extend(tiled(&mut records, "geometry", ch, rows_of(cw, 1), cw));
    }
    if job.false_colour.is_some() {
        develop.extend(tiled(&mut records, "false_colour", ch, rows_of(cw, 4), cw));
    }
    let n_samples = crop_n.div_ceil(job.stride);
    develop.extend(tiled(&mut records, "samples", n_samples, tile / 16, 0));
    let (entry, per) = match proxy {
        Some(_) => ("proxy", rows_of(out.0 as usize, widest_box * widest_box)),
        None => ("full", rows_of(out.0 as usize, 1)),
    };
    let (output, chunks, chunk) = output(&mut records, gpu, entry, out, per);

    let (centre_x, centre_y) = (cw as f32 / 2.0, ch as f32 / 2.0);
    let (tw_from, tw_odd) = last_tile(aw);
    let params = Params {
        raw_w: m.width as u32,
        raw_h: m.height as u32,
        scaled_w: (m.width / 2 * 2) as u32,
        scaled_h: (m.height / 2 * 2) as u32,
        roi: [ax, ay, aw, ah].map(|v| v as u32),
        crop: [cx, cy, cw, ch].map(|v| v as u32),
        levels,
        spare4: [0; 4],
        baseline: job.baseline,
        fc_black: job.false_colour.unwrap_or(0.0),
        factor: 1.0,
        flags: (job.lens.is_some() && job.vignetting) as u32 * VIGNETTING
            | bends as u32 * GEOMETRY
            | (bends != job.false_colour.is_some()) as u32 * FINISHED_BENT
            | transpose as u32 * TRANSPOSE
            | flip_x as u32 * FLIP_X
            | flip_y as u32 * FLIP_Y,
        centre_x,
        centre_y,
        half_diag: (centre_x * centre_x + centre_y * centre_y).sqrt().max(1.0),
        fit: job.lens.filter(|_| bends).map_or(1.0, numa_core::lens::geometry_fit),
        n_radii,
        sgrow: sgrow as u32,
        sgcol: sgcol as u32,
        band_n: (aw * band_rows) as u32,
        out_w: out.0,
        out_h: out.1,
        stride: job.stride as u32,
        n_samples: n_samples as u32,
        t_box,
        tw_from,
        tw_odd,
        spare: 0,
    };
    let floats = |n: usize| (n * 4) as u64;
    let frame = Frame {
        mosaic: (m.width * m.height).div_ceil(2) as u64 * 4,
        sites: floats(aw * ah),
        work: floats(if m.xtrans { 12 * aw * band_rows } else { aw * ah }),
        drv: floats(4 * aw * band_rows),
        homo: floats(aw * band_rows),
        pixels: crop_n as u64,
        bent: bends || job.false_colour.is_some(),
        chunk,
        samples: floats(3 * n_samples),
        table: floats(table.len()),
    };
    Ok(Plan { table, params, records, develop, output, chunks, n_samples, out, full, proxy: proxy.is_some(), frame })
}

fn output(records: &mut Vec<[u32; 4]>, gpu: &Gpu, entry: &'static str, out: (u32, u32), per: usize) -> (Vec<Vec<Dispatch>>, Vec<Chunk>, u64) {
    let (width, height) = (out.0 as usize, out.1 as usize);
    let rows = (STAGING.min(gpu.max_binding) / (12 * width as u64)).max(1) as usize;
    let per = per.min(rows);
    let (mut output, mut chunks) = (Vec::new(), Vec::new());
    for c0 in (0..height).step_by(rows) {
        let c1 = (c0 + rows).min(height);
        let first = output.len();
        for y0 in (c0..c1).step_by(per) {
            let y1 = (y0 + per).min(c1);
            records.push([y0, y1, c0, 0].map(|v| v as u32));
            output.push(vec![Dispatch { entry, record: records.len() - 1, groups: (width.div_ceil(16) as u32, (y1 - y0).div_ceil(16) as u32) }]);
        }
        chunks.push((c0..c1, first..output.len()));
    }
    (output, chunks, (12 * width * rows.min(height)) as u64)
}

type Chunk = (std::ops::Range<usize>, std::ops::Range<usize>);

fn tiled(records: &mut Vec<[u32; 4]>, entry: &'static str, rows: usize, per: usize, width: usize) -> Vec<Vec<Dispatch>> {
    (0..rows)
        .step_by(per)
        .map(|y0| {
            let y1 = (y0 + per).min(rows);
            records.push([y0 as u32, y1 as u32, 0, 0]);
            let groups = match width {
                0 => ((y1 - y0).div_ceil(256) as u32, 1),
                _ => (width.div_ceil(16) as u32, (y1 - y0).div_ceil(16) as u32),
            };
            vec![Dispatch { entry, record: records.len() - 1, groups }]
        })
        .collect()
}

fn run(gpu: &Gpu, job: &Job, exposure: impl FnOnce(&[f32]) -> f32) -> Result<Developed, String> {
    let mut laps = Laps { at: std::env::var_os("NUMA_TIMING").map(|_| Instant::now()), parts: Vec::new() };
    let started = Instant::now();
    let budget = budget();
    let mut plan = plan(gpu, job, budget)?;
    let m = &job.mosaic;
    let sizes = plan.frame.fit(gpu, budget, job.may_round)?;
    let pipelines = gpu.pipelines(sizes.half).ok_or("no f16 on this card")?;

    let mosaic = upload_mosaic(gpu, m, plan.frame.mosaic);
    let bands = gpu.band_stride * plan.records.len() as u64;
    let buffers = Buffers::make(gpu, &plan.frame, &sizes, mosaic, (std::mem::size_of::<Params>() as u64, bands));
    let queue = &gpu.queue;
    queue.write_buffer(&buffers.params, 0, bytemuck::bytes_of(&plan.params));
    for (at, record) in plan.records.iter().enumerate() {
        queue.write_buffer(&buffers.bands, at as u64 * gpu.band_stride, bytemuck::cast_slice(record));
    }
    queue.write_buffer(&buffers.table, 0, bytemuck::cast_slice(&plan.table));
    queue.submit([]);
    laps.lap(gpu, "upload");
    let groups = buffers.groups(gpu);

    let out_floats = 3 * plan.out.0 as usize * plan.out.1 as usize;
    let back = (!gpu.mapped).then(|| Staging::new(gpu, plan.frame.chunk.max(plan.frame.samples)));
    let (samples, data) = std::thread::scope(|scope| {
        let data = scope.spawn(|| {
            let mut data = vec![0.0f32; out_floats];
            data.par_chunks_mut(1 << 18).for_each(|chunk| chunk.fill(0.0));
            data
        });
        submit(gpu, pipelines, &groups, &plan.develop);
        if let Some(back) = &back {
            back.prefault(gpu);
        }
        laps.lap(gpu, "develop");
        let mut samples = vec![0.0f32; 3 * plan.n_samples];
        let read = read(gpu, back.as_ref(), &buffers.samp, &mut samples);
        (read.map(|_| samples), data.join().map_err(|_| "no memory for the frame".to_string()))
    });
    let (samples, mut data) = (samples?, data?);
    plan.params.factor = exposure(&samples);
    laps.lap(gpu, "samples");

    queue.write_buffer(&buffers.params, std::mem::offset_of!(Params, factor) as u64, bytemuck::bytes_of(&plan.params.factor));
    let row = 3 * plan.out.0 as usize;
    for (rows, submissions) in &plan.chunks {
        submit(gpu, pipelines, &groups, &plan.output[submissions.clone()]);
        laps.lap(gpu, "out");
        read(gpu, back.as_ref(), &buffers.outp, &mut data[rows.start * row..rows.end * row])?;
        laps.lap(gpu, "read back");
    }

    buffers.destroy();
    if let Some(back) = &back {
        back.buffer.destroy();
    }

    match plan.proxy && plan.chunks.len() == 1 {
        true => crate::render::keep(gpu, plan.out.0, plan.out.1, &data, buffers.outp.clone()),
        false => buffers.outp.destroy(),
    }
    if laps.at.is_some() {
        log::info!(
            "gpu develop {}x{} → {}x{} in {:.0} ms, {} MB on the card{}: {}",
            m.crop[2],
            m.crop[3],
            plan.out.0,
            plan.out.1,
            started.elapsed().as_secs_f32() * 1000.0,
            sizes.need >> 20,
            if sizes.half { " in f16 planes" } else { "" },
            laps.parts.iter().map(|(name, ms)| format!("{name} {ms:.1}")).collect::<Vec<_>>().join(", ")
        );
    }
    Ok(Developed { width: plan.out.0, height: plan.out.1, data, full: plan.full, factor: plan.params.factor })
}

fn read(gpu: &Gpu, back: Option<&Staging>, source: &wgpu::Buffer, data: &mut [f32]) -> Result<(), String> {
    match back {
        Some(back) => back.read(gpu, source, data),
        None => read_mapped(gpu, source, data),
    }
}

fn submit(gpu: &Gpu, pipelines: &crate::Pipelines, groups: &[wgpu::BindGroup; 3], submissions: &[Vec<Dispatch>]) {
    if std::env::var_os("NUMA_GPU_PROFILE").is_some() {
        let mut spent: Vec<(&str, f32)> = Vec::new();
        for dispatch in submissions.iter().flatten() {
            let _ = gpu.device.poll(WAIT);
            let at = Instant::now();
            let one = Dispatch { entry: dispatch.entry, record: dispatch.record, groups: dispatch.groups };
            submit_each(gpu, pipelines, groups, &[vec![one]]);
            let _ = gpu.device.poll(WAIT);
            let ms = at.elapsed().as_secs_f32() * 1000.0;
            match spent.iter_mut().find(|(entry, _)| *entry == dispatch.entry) {
                Some((_, total)) => *total += ms,
                None => spent.push((dispatch.entry, ms)),
            }
        }
        log::info!("gpu profile: {}", spent.iter().map(|(entry, ms)| format!("{entry} {ms:.1}")).collect::<Vec<_>>().join(", "));
        return;
    }
    submit_each(gpu, pipelines, groups, submissions);
}

fn submit_each(gpu: &Gpu, pipelines: &crate::Pipelines, groups: &[wgpu::BindGroup; 3], submissions: &[Vec<Dispatch>]) {
    let pipeline = |name: &str| &pipelines.iter().find(|(entry, _)| *entry == name).expect("every entry point has a pipeline").1;
    let mut in_flight = std::collections::VecDeque::new();
    for dispatches in submissions {
        if in_flight.len() >= 4 {
            let oldest = in_flight.pop_front();
            let _ = gpu.device.poll(wgpu::PollType::Wait { submission_index: oldest, timeout: Some(std::time::Duration::from_secs(10)) });
        }
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            for dispatch in dispatches {
                pass.set_pipeline(pipeline(dispatch.entry));
                pass.set_bind_group(0, &groups[part(dispatch.entry)], &[(dispatch.record as u64 * gpu.band_stride) as u32]);
                pass.dispatch_workgroups(dispatch.groups.0, dispatch.groups.1, 1);
            }
        }
        in_flight.push_back(gpu.queue.submit([encoder.finish()]));
    }
}

fn read_mapped(gpu: &Gpu, source: &wgpu::Buffer, data: &mut [f32]) -> Result<(), String> {
    let bytes = (data.len() * 4) as u64;
    let (sender, receiver) = std::sync::mpsc::channel();
    source.map_async(wgpu::MapMode::Read, 0..bytes, move |result| {
        let _ = sender.send(result);
    });
    gpu.device.poll(WAIT).map_err(|err| format!("the card did not finish: {err}"))?;
    receiver.recv().map_err(|_| "no answer from the card".to_string())?.map_err(|err| err.to_string())?;
    {
        let view = source.get_mapped_range(0..bytes).map_err(|err| err.to_string())?;
        let from: &[f32] = bytemuck::cast_slice(&view);
        data.par_chunks_mut(1 << 18).zip(from.par_chunks(1 << 18)).for_each(|(to, from)| to.copy_from_slice(from));
    }
    source.unmap();
    Ok(())
}

fn upload_mosaic(gpu: &Gpu, m: &Mosaic, size: u64) -> wgpu::Buffer {
    let sites = &m.pixels[..m.width * m.height];
    let even = sites.len() / 2 * 2;
    let bytes: &[u8] = bytemuck::cast_slice(&sites[..even]);

    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mosaic"),
        size,
        usage: wgpu::BufferUsages::STORAGE | if gpu.mapped { wgpu::BufferUsages::MAP_WRITE } else { wgpu::BufferUsages::COPY_DST },
        mapped_at_creation: gpu.mapped,
    });
    if gpu.mapped {
        {
            let mut view = buffer.get_mapped_range_mut(..).expect("mapped at creation");
            fill(view.slice(..bytes.len()), bytes);
            if even < sites.len() {
                view.slice(bytes.len()..bytes.len() + 4).copy_from_slice(bytemuck::cast_slice(&[sites[even], 0]));
            }
        }
        buffer.unmap();
        return buffer;
    }
    match wgpu::BufferSize::new(bytes.len() as u64).and_then(|size| gpu.queue.write_buffer_with(&buffer, 0, size)) {
        Some(mut view) => fill(view.slice(..), bytes),
        None => gpu.queue.write_buffer(&buffer, 0, bytes),
    }
    if even < sites.len() {
        gpu.queue.write_buffer(&buffer, even as u64 * 2, bytemuck::cast_slice(&[sites[even], 0]));
    }
    buffer
}

fn fill(to: wgpu::WriteOnly<'_, [u8]>, bytes: &[u8]) {
    const CHUNK: usize = 4 << 20;
    let (chunks, mut tail) = to.into_chunks::<CHUNK>();
    let at = bytes.len() - tail.len();
    tail.copy_from_slice(&bytes[at..]);
    let chunks: Vec<_> = chunks.into_iter().collect();
    rayon::scope(|scope| {
        for (to, from) in chunks.into_iter().zip(bytes.chunks_exact(CHUNK)) {
            scope.spawn(move |_| wgpu::WriteOnly::<[u8]>::from(to).copy_from_slice(from));
        }
    });
}

pub(crate) const STAGING: u64 = 64 << 20;

struct Staging {
    buffer: wgpu::Buffer,
    mapped: std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
}

impl Staging {
    fn new(gpu: &Gpu, bytes: u64) -> Self {
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("back"),
            size: bytes.clamp(16, STAGING),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (sender, mapped) = std::sync::mpsc::channel();
        buffer.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        Self { buffer, mapped }
    }

    fn prefault(&self, gpu: &Gpu) {
        let _ = gpu.device.poll(wgpu::PollType::Poll);
        if let Ok(Ok(())) = self.mapped.try_recv() {
            if let Ok(view) = self.buffer.get_mapped_range(..) {
                let touched: u64 = view.par_chunks(1 << 20).map(|chunk| chunk.iter().step_by(4096).map(|&b| u64::from(b)).sum::<u64>()).sum();
                std::hint::black_box(touched);
            }
            self.buffer.unmap();
        }
    }

    fn read(&self, gpu: &Gpu, source: &wgpu::Buffer, data: &mut [f32]) -> Result<(), String> {

        if let Ok(result) = self.mapped.try_recv().or_else(|_| {
            gpu.device.poll(WAIT).ok();
            self.mapped.try_recv()
        }) {
            if result.is_ok() {
                self.buffer.unmap();
            }
        }
        let piece = (self.buffer.size() / 4) as usize;
        for (at, part) in data.chunks_mut(piece).enumerate() {
            let bytes = (part.len() * 4) as u64;
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            encoder.copy_buffer_to_buffer(source, at as u64 * piece as u64 * 4, &self.buffer, 0, bytes);
            gpu.queue.submit([encoder.finish()]);
            let (sender, receiver) = std::sync::mpsc::channel();
            self.buffer.map_async(wgpu::MapMode::Read, 0..bytes, move |result| {
                let _ = sender.send(result);
            });
            gpu.device.poll(WAIT).map_err(|err| format!("the card did not finish: {err}"))?;
            receiver.recv().map_err(|_| "no answer from the card".to_string())?.map_err(|err| err.to_string())?;
            {
                let view = self.buffer.get_mapped_range(0..bytes).map_err(|err| err.to_string())?;
                let from: &[f32] = bytemuck::cast_slice(&view);
                part.par_chunks_mut(1 << 18).zip(from.par_chunks(1 << 18)).for_each(|(to, from)| to.copy_from_slice(from));
            }
            self.buffer.unmap();
        }
        Ok(())
    }
}

fn lens_tables(lens: Option<&LensProfile>, table: &mut Vec<u32>) -> u32 {
    let Some(lens) = lens else { return 0 };
    let n = lens.radii.len();
    let bits = |v: f32| v.to_bits();
    table.extend(lens.radii.iter().map(|&r| bits(r)));
    for values in [&lens.transmission, &lens.distortion, &lens.red, &lens.blue] {
        let last = values.last().copied().unwrap_or(0.0);
        table.extend((0..n).map(|i| bits(values.get(i).copied().unwrap_or(last))));
        table.push(bits(last));
    }
    n as u32
}

fn level_tables(m: &Mosaic, table: &mut Vec<u32>) -> [u32; 4] {
    let mut starts = [0u32; 4];
    let mut made: Vec<([f32; 2], u32)> = Vec::new();
    for k in 0..4 {
        let (black, range) = (m.black[k], m.white[k] - m.black[k]);
        starts[k] = match made.iter().find(|(levels, _)| *levels == [black, range]) {
            Some((_, start)) => *start,
            None => {
                let start = table.len() as u32;
                let clip = |v: f32| if v.is_sign_negative() { 0.0 } else { v };
                table.extend((0..=u16::MAX).map(|count| (clip(count as f32 - black) / range).to_bits()));
                made.push(([black, range], start));
                start
            }
        };
    }
    starts
}

fn hexagon(cfa: &[[u8; 6]; 6]) -> ([i32; 144], usize, usize) {
    const ORTH: [i32; 12] = [1, 0, 0, 1, -1, 0, 0, -1, 1, 0, 0, 1];
    const PATT: [[i32; 16]; 2] = [
        [0, 1, 0, -1, 2, 0, -1, 0, 1, 1, 1, -1, 0, 0, 0, 0],
        [0, 1, 0, -2, 1, 0, -2, 0, 1, 1, -2, -2, 1, -1, -1, 1],
    ];
    let colour = |row: i32, col: i32| cfa[row.rem_euclid(6) as usize][col.rem_euclid(6) as usize];
    let mut hex = [0i32; 144];
    let (mut sgrow, mut sgcol) = (0, 0);
    for row in 0..3i32 {
        for col in 0..3i32 {
            let g = usize::from(colour(row, col) == 1);
            let mut ng = 0;
            for d in (0..10).step_by(2) {
                if colour(row + ORTH[d], col + ORTH[d + 2]) == 1 {
                    ng = 0;
                } else {
                    ng += 1;
                }
                if ng == 4 {
                    (sgrow, sgcol) = (row as usize, col as usize);
                }
                if ng == g + 1 {
                    for c in 0..8 {
                        let v = ORTH[d] * PATT[g][c * 2] + ORTH[d + 1] * PATT[g][c * 2 + 1];
                        let h = ORTH[d + 2] * PATT[g][c * 2] + ORTH[d + 3] * PATT[g][c * 2 + 1];
                        let at = ((row as usize * 3 + col as usize) * 8 + (c ^ (g * 2 & d))) * 2;
                        (hex[at], hex[at + 1]) = (v, h);
                    }
                }
            }
        }
    }
    (hex, sgrow, sgcol)
}

fn last_tile(width: usize) -> (u32, u32) {
    let Some(left) = (0..width).step_by(40).filter(|left| width - left > 24).last() else {
        return (u32::MAX, 0);
    };
    ((left + PAD) as u32, (64.min(width - left) % 2) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hexagon_finds_six_greens_round_every_red_and_blue() {
        let rows = ["GbGGrG", "rGrbGb", "GbGGrG", "GrGGbG", "bGbrGr", "GrGGbG"];
        let cfa: [[u8; 6]; 6] = std::array::from_fn(|r| {
            std::array::from_fn(|c| match rows[r].as_bytes()[c] {
                b'r' => 0,
                b'G' => 1,
                _ => 2,
            })
        });
        let (hex, sgrow, sgcol) = hexagon(&cfa);
        assert_eq!((sgrow, sgcol), (1, 1));
        for row in 0..3 {
            for col in 0..3 {
                if cfa[row][col] == 1 {
                    continue;
                }
                for slot in 0..6 {
                    let at = ((row * 3 + col) * 8 + slot) * 2;
                    let (dy, dx) = (hex[at], hex[at + 1]);
                    let colour = cfa[(row as i32 + 6 + dy) as usize % 6][(col as i32 + 6 + dx) as usize % 6];
                    assert_eq!(colour, 1, "slot {slot} of ({row}, {col}) at ({dy}, {dx})");
                }
            }
        }
    }

    #[test]
    fn a_small_frame_goes_through_every_pass() {
        for frugal in [false, true] {
            if !crate::open_now(frugal) {
                assert!(!crate::broken(), "the develop did not build on this card");
                println!("skipped: no {} card", if frugal { "integrated" } else { "suitable" });
                continue;
            }
            small_frame(frugal);
        }
    }

    fn small_frame(frugal: bool) {
        let xtrans_rows = ["GbGGrG", "rGrbGb", "GbGGrG", "GrGGbG", "bGbrGr", "GrGGbG"];
        let pixels: Vec<u16> = (0..520 * 520).map(|i| 1024 + ((i % 520) * 7 + (i / 520) * 3) as u16 % 512).collect();
        let lens = numa_core::lens::manual_lens_profile(40.0, 50.0);
        for xtrans in [false, true] {
            let cfa: [[u8; 6]; 6] = std::array::from_fn(|r| {
                std::array::from_fn(|c| match xtrans {
                    true => match xtrans_rows[r].as_bytes()[c] {
                        b'r' => 0,
                        b'G' => 1,
                        _ => 2,
                    },
                    false => [[0, 1], [1, 2]][r % 2][c % 2],
                })
            });
            for proxy in [None, Some(200)] {
                let job = Job {
                    mosaic: Mosaic {
                        pixels: &pixels,
                        width: 520,
                        height: 520,
                        black: [1000.0; 4],
                        white: [16383.0; 4],
                        cfa,
                        xtrans,
                        active: [4, 4, 512, 512],
                        crop: [0, 0, 512, 510],
                    },
                    baseline: 2.0,
                    lens: Some(&lens),
                    vignetting: true,
                    false_colour: xtrans.then_some(1e-4),
                    flips: (true, false, true),
                    proxy,
                    stride: 37,
                    may_round: false,
                };
                let done = develop(&job, frugal, |samples| {
                    assert!(samples.iter().all(|v| v.is_finite()));
                    1.5
                })
                .expect("the card develops a small frame");
                assert_eq!(done.full, (510, 512), "turned");
                assert_eq!((done.width, done.height), proxy.map_or((510, 512), |_| (199, 200)));
                let (lo, hi) = done.data.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));

                let bad = done.data.iter().position(|v| !v.is_finite() || *v < -0.01 || *v >= 1.0);
                assert!(bad.is_none(), "xtrans {xtrans}, proxy {proxy:?}: {lo}..{hi}, first bad at {bad:?}");
            }
        }
    }

    #[test]
    fn only_an_odd_last_tile_is_odd() {
        assert_eq!(last_tile(64 + 40), (52, 0));
        assert_eq!(last_tile(81), (52, 1));
        assert_eq!(last_tile(20), (u32::MAX, 0));
    }
}
