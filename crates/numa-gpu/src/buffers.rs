use crate::Gpu;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Home {

    Own,
    Mosaic,
    Sites,
    Work,
    Drv,
    Homo,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Frame {
    pub mosaic: u64,
    pub sites: u64,
    pub work: u64,
    pub drv: u64,
    pub homo: u64,

    pub pixels: u64,

    pub bent: bool,

    pub chunk: u64,
    pub samples: u64,
    pub table: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Sizes {
    pub half: bool,
    pub plane: u64,
    pub lin: [Home; 3],
    pub bent: [Home; 3],

    pub need: u64,
    pub largest: u64,
}

impl Frame {

    pub fn sizes(&self, half: bool, staging: u64) -> Sizes {
        let plane = self.pixels * if half { 2 } else { 4 };
        let dead = [(Home::Mosaic, self.mosaic), (Home::Sites, self.sites), (Home::Work, self.work), (Home::Drv, self.drv), (Home::Homo, self.homo)];
        let mut taken = Vec::new();
        let mut home = |candidates: &[Home]| {
            let found = dead
                .iter()
                .filter(|(home, bytes)| candidates.contains(home) && !taken.contains(home) && *bytes >= plane)
                .min_by_key(|(_, bytes)| *bytes)
                .map(|(home, _)| *home);
            taken.extend(found);
            found.unwrap_or(Home::Own)
        };

        let lin = [(); 3].map(|_| home(&[Home::Mosaic]));
        let bent = match self.bent {
            true => [(); 3].map(|_| home(&[Home::Mosaic, Home::Sites, Home::Work, Home::Drv, Home::Homo])),
            false => [Home::Own; 3],
        };
        let own = |homes: &[Home; 3]| homes.iter().filter(|home| **home == Home::Own).count() as u64 * plane;
        let fixed = [self.mosaic, self.sites, self.work, self.drv, self.homo, self.chunk, self.samples, self.table, staging];
        let need = fixed.iter().sum::<u64>() + own(&lin) + if self.bent { own(&bent) } else { 0 };
        let largest = fixed.into_iter().chain([plane]).max().unwrap_or(0);
        Sizes { half, plane, lin, bent, need, largest }
    }

    pub fn fit(&self, gpu: &Gpu, budget: u64, may_round: bool) -> Result<Sizes, String> {
        let staging = if gpu.mapped { 0 } else { crate::develop::STAGING.min(self.chunk.max(self.samples)) };
        let fits = |sizes: &Sizes| sizes.need <= budget && sizes.largest <= gpu.max_binding;
        let full = self.sizes(false, staging);
        if fits(&full) && !crate::half_always() {
            return Ok(full);
        }
        let half = self.sizes(true, staging);
        if (may_round || crate::half_always()) && gpu.device.features().contains(wgpu::Features::SHADER_F16) && fits(&half) {
            return Ok(half);
        }
        log::info!(
            "gpu develop: {} MB in all ({} in f16), the largest {} MB; the card is given {} MB, {} MB a buffer",
            full.need >> 20,
            half.need >> 20,
            full.largest >> 20,
            budget >> 20,
            gpu.max_binding >> 20
        );
        Err("the frame needs more of the card than it is given".into())
    }
}

pub(crate) struct Buffers {
    pub params: wgpu::Buffer,
    pub bands: wgpu::Buffer,
    pub table: wgpu::Buffer,
    pub mosaic: wgpu::Buffer,
    pub sites: wgpu::Buffer,
    pub work: wgpu::Buffer,
    pub drv: wgpu::Buffer,
    pub homo: wgpu::Buffer,
    pub lin: [wgpu::Buffer; 3],
    pub bent: [wgpu::Buffer; 3],
    pub outp: wgpu::Buffer,
    pub samp: wgpu::Buffer,

    spare: [wgpu::Buffer; 2],
}

impl Buffers {
    pub fn make(gpu: &Gpu, frame: &Frame, sizes: &Sizes, mosaic: wgpu::Buffer, (params, bands): (u64, u64)) -> Self {
        let storage = wgpu::BufferUsages::STORAGE;
        let upload = storage | wgpu::BufferUsages::COPY_DST;

        let back = storage | if gpu.mapped { wgpu::BufferUsages::MAP_READ } else { wgpu::BufferUsages::COPY_SRC };
        let buffer = |label: &str, size: u64, usage: wgpu::BufferUsages| {
            gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: size.max(16), usage, mapped_at_creation: false })
        };
        let sites = buffer("sites", frame.sites, storage);
        let work = buffer("work", frame.work, storage);
        let drv = buffer("drv", frame.drv, storage);
        let homo = buffer("homo", frame.homo, storage);
        let plane = |home: Home, label: &str| match home {
            Home::Own => buffer(label, sizes.plane, storage),
            Home::Mosaic => mosaic.clone(),
            Home::Sites => sites.clone(),
            Home::Work => work.clone(),
            Home::Drv => drv.clone(),
            Home::Homo => homo.clone(),
        };
        let lin = [plane(sizes.lin[0], "lin r"), plane(sizes.lin[1], "lin g"), plane(sizes.lin[2], "lin b")];
        let bent = match frame.bent {
            true => [plane(sizes.bent[0], "bent r"), plane(sizes.bent[1], "bent g"), plane(sizes.bent[2], "bent b")],
            false => [(); 3].map(|_| buffer("bent", 0, storage)),
        };
        Self {
            params: buffer("params", params, upload),
            bands: buffer("bands", bands, upload),
            table: buffer("table", frame.table, upload),
            mosaic,
            sites,
            work,
            drv,
            homo,
            lin,
            bent,
            outp: buffer("out", frame.chunk, back),
            samp: buffer("samples", frame.samples, back),
            spare: [buffer("spare", 0, storage), buffer("spare", 0, storage)],
        }
    }

    pub fn groups(&self, gpu: &Gpu) -> [wgpu::BindGroup; 3] {
        let [ro, rw] = &self.spare;
        let lin = [&self.lin[0], &self.lin[1], &self.lin[2]];
        let bent = [&self.bent[0], &self.bent[1], &self.bent[2]];
        let none = [rw, rw, rw];
        let (p, b, t) = (&self.params, &self.bands, &self.table);
        [
            self.group(gpu, [p, b, &self.mosaic, t, rw, rw, rw], none, none, [rw, rw, &self.sites]),
            self.group(gpu, [p, b, ro, t, &self.work, &self.drv, &self.homo], lin, none, [rw, rw, &self.sites]),
            self.group(gpu, [p, b, ro, t, rw, rw, rw], lin, bent, [&self.outp, &self.samp, rw]),
        ]
    }

    fn group(&self, gpu: &Gpu, first: [&wgpu::Buffer; 7], lin: [&wgpu::Buffer; 3], bent: [&wgpu::Buffer; 3], last: [&wgpu::Buffer; 3]) -> wgpu::BindGroup {
        let entries: Vec<_> = first
            .into_iter()
            .chain(lin)
            .chain(bent)
            .chain(last)
            .enumerate()
            .map(|(binding, buffer)| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: match binding {
                    1 => wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer, offset: 0, size: wgpu::BufferSize::new(16) }),
                    _ => buffer.as_entire_binding(),
                },
            })
            .collect();
        gpu.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("develop"), layout: &gpu.layout, entries: &entries })
    }

    pub fn destroy(&self) {
        let planes = self.lin.iter().chain(&self.bent);
        for buffer in [&self.mosaic, &self.sites, &self.work, &self.drv, &self.homo, &self.samp].into_iter().chain(planes) {
            buffer.destroy();
        }
    }
}

pub(crate) fn part(entry: &str) -> usize {
    match entry {
        "scale" => 0,
        _ if entry.starts_with("ppg_") || entry.starts_with("mk_") => 1,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1 << 20;

    fn bayer(w: u64, h: u64) -> Frame {
        let n = w * h;
        Frame { mosaic: 2 * n, sites: 4 * n, work: 4 * n, drv: 0, homo: 0, pixels: n, bent: true, chunk: 64 * MB, samples: 12 * n / 37, table: MB }
    }

    #[test]
    fn the_planes_fit_where_the_passes_are_done() {
        let a6000 = bayer(6000, 4000).sizes(false, 0);
        assert_eq!(a6000.lin, [Home::Own; 3], "a float plane is larger than the mosaic");
        assert_eq!(a6000.bent, [Home::Sites, Home::Work, Home::Own]);
        assert!(a6000.need <= 1 << 30 && a6000.largest <= 256 * MB, "{a6000:?}");

        let fifty = bayer(8688, 5792);
        let full = fifty.sizes(false, 0);
        assert!(full.need > 1 << 30, "{full:?}");
        let half = fifty.sizes(true, 0);
        assert_eq!(half.lin, [Home::Mosaic, Home::Own, Home::Own]);
        assert_eq!(half.bent, [Home::Sites, Home::Work, Home::Own]);
        assert!(half.need <= 1 << 30 && half.largest <= 256 * MB, "{half:?}");

        let flat = Frame { bent: false, ..fifty }.sizes(false, 0);
        assert_eq!(flat.need, full.need - fifty.pixels * 4);
    }
}
