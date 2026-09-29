# MAC-BENCH — Metal, the mosaic proxy and the thread pool on the MacBook

What could not be measured on Linux, measured on the photographer's MacBook
(the self-hosted runner `mac` of Numa-mac), to set the GPU and thread policy
of the Apple apps with numbers. Branch `mac-bench` here (Metal for Apple,
`crates/numa-io/examples/mac_bench.rs`), branch `mac-bench` in Numa-mac
(`.github/workflows/mac-bench.yml`, `dev/mac-bench/`), results on Numa-mac's
`mac-bench-results` branch as `runs/<run number>/`.

Base: speed-merge `11af23e` (gpu-night, smart-night, speed-night merged).
Runs, all on AC and without a thermal warning:

- 36417275366: matrix and tolerance test.
- 36421340045: tolerance test per body in fast and safe math, the cost of
  safe math, threads, profile.
- 36422778464: mapped, iPhone limits, tile, background QoS.
- 36423830935: removed the 1.4 GB cache (raws and build output) from the
  runner.

## Method

- Raws: nine bodies, CC0 from raw.pixls.us (5D Mark III, R5, 5DS, A6000,
  A7R III, Z 6, Z 7, E-M1 II, X-T5 — the Linux harness's), cached on the
  runner. Not the photographer's photographs.
- One process per body and mode; one warm-up pass thrown away, medians of
  three kept passes. Modes: `decode` (rawler alone), `cpu-full` and
  `metal-full` (`decode_linear_best` on the processor / on Metal),
  `metal-proxy` (`proxy_from_card`, 2400 px), `mosaic-proxy`
  (`proxy_from_mosaic`, 2400 px).
- Time: wall per call, the process's CPU time, numa-io's stage laps and
  numa-gpu's phases (`NUMA_TIMING`).
- Energy, two ways, no root needed:
  - the process's own CPU energy, `proc_pid_rusage(RUSAGE_INFO_V6)`
    `ri_energy_nj` (and the P-cores' share of its CPU time);
  - the whole machine's CPU, GPU and DRAM energy from IOReport's "Energy
    Model" channels (as macmon reads them), less what the machine drew idle
    over the same time (0.8 s measured before each process). The GPU's
    energy is only here: `ri_energy_nj` is CPU only.
- Peak: `ri_lifetime_max_phys_footprint` of the process (Metal buffers count
  in it on Apple).
- Battery or AC, thermal state and load are logged around every phase.

## The machine

MacBook Pro Mac15,6: **Apple M3 Pro**, 11 cores (5 performance, 6
efficiency), 14-core GPU, 18 GB, macOS 26.0.1. On AC, battery full, Low
Power Mode off, no thermal warning recorded in any run. Run 1
(https://github.com/simmmmm/Numa-mac/actions/runs/36417275366, 28
September 14:09): the build took 71 s, and all 45 processes took 3 minutes.

## 1. Metal against the processor, and 3. the mosaic proxy (run 1)

After rawler's decode (the `decode` column is rawler alone; every other
column is that mode less it). ms is wall time. J is the machine's CPU + GPU +
DRAM energy less idle (IOReport). Peak is the process's footprint, Metal's
buffers included. Medians of three.

| body | rawler decode ms / J | processor, full ms / J / peak MB | Metal, full | Metal, proxy 2400 | mosaic proxy 2400 (processor) |
|---|---|---|---|---|---|
| 5D3 | 81 / 0.26 | 107 / 2.89 / 809 | 75 / 0.89 / 1338 | 67 / 0.66 / 1196 | 86 / 2.24 / 300 |
| R5 | 182 / 2.18 | 206 / 5.71 / 1359 | 133 / 1.77 / 2417 | 106 / 1.36 / 1975 | 93 / 2.68 / 478 |
| 5DS | 138 / 0.98 | 238 / 6.59 / 1904 | 184 / 2.29 / 2976 | 123 / 1.80 / 2443 | 102 / 2.98 / 606 |
| A6000 | 27 / 0.08 | 128 / 2.58 / 729 | 95 / 0.68 / 1314 | 83 / 0.53 / 1109 | 96 / 1.70 / 155 |
| A7R3 | 32 / 0.04 | 214 / 4.74 / 1254 | 147 / 1.37 / 2223 | 126 / 0.98 / 1764 | 111 / 2.12 / 200 |
| Z6 | 92 / 0.35 | 113 / 3.06 / 946 | 70 / 0.94 / 1494 | 56 / 0.77 / 1238 | 82 / 2.29 / 271 |
| Z7 | 131 / 0.89 | 208 / 5.79 / 1687 | 135 / 1.87 / 2630 | 107 / 1.48 / 2133 | 93 / 2.75 / 465 |
| E-M1 II | 135 / 0.42 | 87 / 2.38 / 634 | 58 / 0.67 / 1129 | 51 / 0.53 / 983 | 90 / 2.25 / 141 |
| X-T5 | 155 / 2.53 | 693 / 19.92 / 1810 | 308 / 5.37 / 2016 | 281 / 4.85 / 1627 | 123 / 3.27 / 231 |
| **nine** | | **1994 ms / 53.7 J** | **1204 / 15.8** | **1001 / 13.0** | **876 / 22.3** |

Where the joules go, over the nine (J, after the decode):

| | machine CPU | GPU | DRAM | the process's own CPU (rusage) |
|---|---|---|---|---|
| processor, full | 49.8 | 0 | 3.9 | 46.9 |
| Metal, full | 5.6 | 6.4 | 3.8 | 4.7 |
| Metal, proxy | 4.3 | 5.8 | 2.9 | 3.7 |
| mosaic proxy | 21.9 | 0 | 0.4 | 21.6 |

The process's own `ri_energy_nj` and the machine's CPU energy less idle
agree to within about 6 %, so the two meters check each other. The GPU's
share is only visible through IOReport.

What it says:

- **Metal takes a third of the processor's energy** for the same full
  develop: 15.8 J against 53.7 J over the nine, and 40 % less time (1.2 s
  against 2.0 s). On the X-T5, Markesteijn is 19.9 J on the processor and
  5.4 J on Metal.
- **For the editor's proxy, Metal's proxy costs the least energy** (13.0 J,
  0.5–1.8 J a Bayer frame). **The mosaic proxy is the fastest on the whole**
  (876 ms), and much the fastest on the X-T5 (123 against 281 ms), but it
  costs 1.7× Metal's energy, because it keeps all eleven cores busy (1.1–2.7
  CPU-seconds a frame).
  - On the smaller Bayer sensors (5D3, A6000, Z 6, E-M1) Metal's proxy is
    both faster and cheaper.
  - On the 45–50 MP ones (R5, 5DS, A7R III, Z 7) the mosaic proxy is 10–20
    ms faster, and Metal's proxy uses half the energy.
- **Memory is where Metal pays.** The Metal develop's peak footprint is
  1.0–3.0 GB, against 0.14–0.6 GB for the mosaic proxy: the frame-sized
  planes (the mosaic, linear, bent, out, sites, work) all live at once.
  That is harmless on an 18 GB Mac and decisive on an iPhone (below).
- Metal's own phases, in ms, from `NUMA_TIMING` (the waits between them
  are the laps' own):

  | | upload | develop | samples | out | read back |
  |---|---|---|---|---|---|
  | R5, full | 19 | 80 | 3 | 9 | 19 |
  | R5, proxy | 20 | 73 | 3 | 6 | 2 |
  | X-T5, full | 18 | 251 | 3 | 8 | 18 |

  So upload and read back are 15–25 % of a full develop, and the proxy's
  read back is almost nothing. That bounds what unified-memory mapping can
  save (§2).
- The device and all 15 pipelines are made in about 120 ms at start.
- rawler's own decode runs mostly on one core. The single-threaded Sony
  decodes (A6000, A7R III) ran 80–90 % on the efficiency cores at the
  default quality of service.

### Correctness on Metal: fast math is the difference (run 2)

Run 2 (https://github.com/simmmmm/Numa-mac/actions/runs/36421340045, 14:22,
on AC) ran `the_card_develops_what_the_processor_does` once per body, every
stage, in Metal's default math and in safe math. Safe math is set through a
wgpu-hal 30.0.1 vendored on this branch, with `MTLCompileOptions.mathMode`
settable by `NUMA_METAL_MATH`; wgpu itself leaves Metal's default, which is
fast.

| body | fast math: demosaic max / > 1e-3 | fast: proxy, 8-bit | fast: test | **safe math: demosaic max** | safe: proxy, 8-bit | safe: test |
|---|---|---|---|---|---|---|
| 5D3 | 5.8e-1 / 0.037 % | 3 levels, 0.16 % moved | FAIL | **0** | 1 level, 0.011 % | pass |
| R5 | 2.1e-2 / 0.053 % | 1, 0.42 % | pass | **0** | 1, 0.0096 % | pass |
| 5DS | 5.5e-1 / 0.013 % | 3, 0.16 % | FAIL | **0** | 1, 0.029 % | pass |
| A6000 | 3.2e-2 / 0.047 % | 4, 0.38 % | FAIL | **0** | 1, 0.025 % | pass |
| A7R III | 2.2e-2 / 0.060 % | 2, 0.22 % | pass | **0** | 1, 0.010 % | pass |
| Z 6 | 3.8e-2 / 0.064 % | 2, 0.23 % | pass | **0** | 1, 0.0065 % | pass |
| Z 7 | 2.7e-1 / 0.23 % | – | FAIL (demosaic) | **0** | 1, 0.017 % | pass |
| E-M1 II | 3.0e-2 / 0.25 % | – | FAIL (demosaic) | **0** | 1, 0.013 % | pass |
| X-T5 | 4.2e-2 / 0.017 % | 2, 0.052 % | pass | 4.5e-2 / 0.011 % | 2, 0.041 % | pass |

- **In safe math, Metal's PPG is bit for bit the processor's** on all
  eight Bayer bodies (max difference 0). Every body passes, and the
  rendered proxy is at most 1 level apart, which is exactly what Vulkan
  measured on Linux.
- In fast math, five of the nine fail the tolerance. A few PPG direction
  ties flip, and one pixel moves by up to 0.65.
- X-T5 (Markesteijn) is the same in both modes and within tolerance. Its
  differences are the near-ties already written up for Vulkan.

What safe math costs (5D3, R5, X-T5, medians, per body):

| | fast | safe |
|---|---|---|
| Metal full develop | 309 ms, 4.34 J | 330 ms (+7 %), 4.74 J (+9 %) |
| Metal proxy | 289 ms, 3.99 J | 311 ms (+8 %), 4.49 J (+13 %) |

This is the whole call, rawler's decode included, so safe math is about 20
ms and 0.5 J more a frame. **Policy: on Apple, compile the develop in safe
math.** wgpu 30 has no switch for it. Either upstream a
`MTLCompileOptions.mathMode` option (wgpu-hal's Metal device, a few lines,
as on this branch), or keep the patch. The alternative is to make the
shader's PPG tie-breaks immune to reassociation, which is harder to prove.

## 4. The thread pool on the M3 Pro (run 2)

rayon's global pool at 2, 4, 6, 8 and all 11 threads, over 5D3, R5 and
X-T5, whole call (decode included), medians. "ut" is the pool's threads at
utility QoS.

| | threads | 5D3 ms | R5 ms | X-T5 ms | mean ms | CPU-s | on P-cores | the process's J | machine J |
|---|---|---|---|---|---|---|---|---|---|
| processor, full | 2 | 361 | 842 | 2740 | 1314 | 2.70 | 94 % | 12.7 | 14.0 |
| | 4 | 243 | 496 | 1475 | 738 | 2.82 | 94 % | 12.6 | 13.8 |
| | 6 | 212 | 433 | 1128 | 591 | 3.16 | 82 % | 12.1 | 13.0 |
| | 8 | 197 | 408 | 1021 | 542 | 3.62 | 66 % | 11.2 | 12.3 |
| | **11** | 196 | 392 | 860 | **482** | 4.17 | 53 % | **10.3** | **11.4** |
| | 11, ut | 188 | 389 | 855 | 477 | 4.17 | 54 % | 10.3 | 11.5 |
| mosaic proxy | 2 | 370 | 605 | 921 | 632 | 1.38 | 93 % | 5.6 | 5.8 |
| | 4 | 231 | 349 | 522 | 368 | 1.44 | 93 % | 5.5 | 5.7 |
| | 6 | 196 | 305 | 390 | 297 | 1.55 | 81 % | 5.2 | 5.4 |
| | 8 | 180 | 287 | 368 | 278 | 1.70 | 67 % | 4.7 | 4.9 |
| | **11** | 166 | 273 | 276 | **239** | 1.87 | 55 % | **4.3** | **4.4** |

**On this Mac, all the cores are both the fastest and the cheapest.** This
is the opposite of Linux, where 16 → 4 threads saved 40 % of the CPU time.

The reason is visible in the P-core column. With 2–4 threads, macOS puts
them on the performance cores (94 %) and clocks them high. From 6 threads
up, the efficiency cores take a growing share of the work, and they do it
for much less energy per instruction. More CPU-seconds, fewer joules: 4.2
CPU-s for 10.3 J at 11 threads, against 2.7 CPU-s for 12.7 J at 2.

Utility QoS changed nothing at 4 or 11 threads, because macOS still runs
utility work on the P-cores. Only background QoS is held to the E-cores
(run 3). **Policy: on Apple Silicon, leave rayon at every core**
(`available_parallelism`). The Linux cap on the pool does not carry over,
and neither does PERF-024's "a quarter of the cores when frugal": on Apple,
frugal should instead mean E-cores through QoS (run 3).

## Metal per entry point (run 2, `NUMA_GPU_PROFILE`, a wait after every dispatch)

- R5, 45 MP: scale 55, ppg_green 11, ppg_rb 16, geometry 19, samples 5,
  full 16 ms; the proxy pass 20 ms.
- X-T5: scale 46, the seven Markesteijn passes 32–44 each (245 ms in all),
  geometry 16, false colour 47; full 15, proxy 21 ms.

`scale` (the black and white levels) should be one of the cheapest passes.
Here it is the most expensive, because it runs as about 45 submissions of
1 M pixels each (`tile_pixels` for an integrated GPU). The profiler's wait
after every dispatch inflates that, but the tile size is the next thing to
tune on Apple (run 3: `NUMA_GPU_TILE`).

## 2. Unified memory (run 3, https://github.com/simmmmm/Numa-mac/actions/runs/36422778464)

`NUMA_GPU_MAPPED=1` (Apple only; needs `MAPPABLE_PRIMARY_BUFFERS`, which
Metal has). The mosaic is written straight into a `STORAGE | MAP_WRITE`
buffer mapped at creation. The frame and the samples are read by mapping
the `STORAGE | MAP_READ` buffers the shader wrote. No staging buffer and no
copy on the GPU either way; wgpu makes these buffers `StorageModeShared`.
Safe math, medians, ms:

| | upload | read back (full) | whole call, full | whole call, proxy |
|---|---|---|---|---|
| 5D3, staged → mapped | 10.7 → 6.1 | 9.6 → 6.7 | 154 → 147 | 147 → 138 |
| R5 | 19.1 → 8.8 | 19.4 → 13.6 | 315 → 298 | 292 → 279 |
| 5DS | 21.4 → 9.9 | 22.2 → 14.8 | 303 → 271 | 265 → 250 |
| X-T5 | 17.9 → 9.9 | 17.5 → 11.9 | 518 → 502 | 500 → 487 |
| mean of the four, J | | | 4.32 → 4.14 | 3.96 → 3.84 |

- Upload is **halved**. What is left is the one parallel memcpy from
  rawler's `Vec`. Read back of a full frame is a third cheaper; the proxy's
  was already 1.7 ms. The GPU passes run at the same speed on shared
  buffers.
- In all: **5–10 % off the whole call, 3–4 % off the energy**. Worth
  having, not a game-changer. A true zero-copy upload (Metal's
  `newBufferWithBytesNoCopy` over a page-aligned mosaic) would take the
  remaining ~9 ms. It needs wgpu-hal interop and rawler decoding into
  page-aligned memory, so it is not done here.
- Peak footprint is unchanged: the planes are the same size wherever they
  live.

## Metal's submission size (run 3)

`tile_pixels` is 1 M source pixels per submission for an integrated GPU,
chosen on Linux so the desktop never waits behind a photograph. On the
M3 Pro, 8 M is 4 % faster (X-T5 full 519 → 499 ms, R5 317 → 308, 5D3
160 → 153) and 3 % cheaper (4.66 → 4.52 J). 64 M pushes the X-T5's
Markesteijn band buffers past the 3 GB budget (3.2 GB), so it fell back to
the processor. **On Apple: 8 M.**

## Background QoS: the efficiency cores (run 3)

rayon's threads at background QoS, which macOS keeps on the E-cores (2 %
of the CPU time on P-cores), over 5D3, R5 and X-T5, per body:

| | default QoS, 11 threads | background, 11 | background, 6 |
|---|---|---|---|
| processor, full | 482 ms, 11.4 J | 3883 ms, **3.3 J** | 4077 ms, 3.1 J |
| mosaic proxy | 239 ms, 4.4 J | 1563 ms, **1.1 J** | 1627 ms, 1.05 J |

**Background QoS costs 3.5–4× less energy and runs 6–8× slower.** That is
the lever for work nobody is waiting for: the decode ahead, thumbnails,
edited previews. On Apple, PERF-024's `background` pool should use
background QoS on every thread (`lower_priority` sets nothing there today)
instead of a quarter of the cores. What the photographer is waiting for
stays at default or user-initiated QoS on every core.

## 5. An iPhone's limits, played on the Mac (run 3)

`NUMA_GPU_MAX_BUFFER` and `NUMA_GPU_BUDGET` hold the Metal proxy to a
smaller device's limits. The first line is a guess at an A-series iPhone;
its real `maxBufferLength` has not been measured, and the Apple app should
log numa-gpu's limit line on first open. The second is more generous.

| body | GPU buffers needed: total / largest (MB) | 256 MB a buffer, 1 GB in all | 1 GB a buffer, 2 GB in all |
|---|---|---|---|
| 5D3 22 MP | < 1024 / < 256 | Metal, 60 ms | Metal |
| E-M1 II 20 MP | < 1024 / < 256 | Metal, 55 ms | Metal |
| A6000 24 MP | 832 / 274 | processor | Metal, 66 ms |
| Z 6 24 MP | 842 / 278 | processor | Metal, 63 ms |
| X-T5 40 MP | 1279 / 455 | processor | Metal, 336 ms |
| A7R III 42 MP | 1428 / 482 | processor | Metal, 110 ms |
| R5 45 MP | 1514 / 512 | processor | Metal, 109 ms |
| Z 7 46 MP | 1534 / 520 | processor | Metal, 106 ms |
| 5DS 50 MP | 1702 / 575 | processor | Metal, 118 ms |

The fallback works: every refused frame was developed on the processor,
correctly. The table shows what makes the difference:

- The largest buffer is always an interleaved RGB plane: `lin`, `bent` or
  the proxy's source, 12 bytes a pixel. **Splitting the planes** (R, G and B
  as three bindings: 29 are allowed, 12 used) divides it by three. That
  lets the 24 MP bodies through a 256 MB limit and keeps every body under
  200 MB a buffer. This change was not made here: it touches every pass
  in develop.wgsl.
- The **total** is the harder limit: 0.8–1.7 GB of GPU buffers, and a
  process peak of 1.0–2.4 GB for the Metal proxy (run 1). Getting a
  45–50 MP frame under ~1 GB takes more than splitting the planes. It
  also needs f16 for `lin` and `bent` (half; Markesteijn's near-ties would
  have to be re-measured), and `sites`/`work` reused as the band buffers.
- The **mosaic proxy's peak is 0.14–0.6 GB** for the same frames.

## Policy

Measured on an M3 Pro MacBook Pro, on AC. Everything about iPads and
iPhones below is extrapolated from it and says so.

**Every Apple device**

- **Compile Metal in safe math.** Fast math, Metal's default, fails the
  tolerance test on five of nine bodies. Safe math is bit-exact on Bayer
  and costs about 7 % on a Bayer develop and 24 % on the X-T5's
  Markesteijn (251 → 310 ms). This needs a `mathMode` option in wgpu-hal
  (upstream it, or keep the vendored patch on this branch).
- **rayon at every core** for what the photographer is waiting for. It is
  both the fastest and the cheapest on the M3 Pro.
- **Background QoS** for all speculative work: 3.5–4× less energy.
- **Frugal on Apple is not "no GPU".** There is no discrete card to wake,
  and Metal is the cheapest way to develop any frame. Low Power Mode
  should keep Metal for the photograph on screen and put everything
  speculative at background QoS.
- `tile_pixels` 8 M. Mapped (unified-memory) buffers when convenient:
  5–10 % faster.

**Mac (measured).** 1:1 and export on Metal: 3.4× less energy than the
processor, 1.65× faster after the decode. The editor's proxy on Metal as
well: the least energy of all routes, 0.5–1.8 J per Bayer frame. The
mosaic proxy is about 10 % faster over the nine, and much faster on the
X-T5 (123 against 281 ms), but it costs 1.7× the energy. If the X-T5's
latency matters more than its joules, the mosaic proxy is the one to use
for X-Trans. Memory (1–3 GB peak) is a non-issue on a Mac.

**iPad, M-series (extrapolated).** The same CPU and GPU families, fewer
cores (4–6 P, 4–6 E, 8–10 GPU cores against 5 P, 6 E, 14 here): expect
the Metal times about 1.3–1.7× longer and similar joules per frame. Same
policy as the Mac, with one gate: take the Metal route only when the
frame's GPU need (numa-gpu already computes it) plus the process's other
memory fits well inside `os_proc_available_memory()`. Fall back to the
mosaic proxy otherwise. That gate matters on 8 GB iPads with a 45–50 MP
frame (2.4–3.0 GB peak); a 16 GB iPad Pro will not notice.

**iPhone, A-series (extrapolated from the limits played above).** Memory
decides, not energy.

- **Editor proxy: the mosaic proxy on the processor** as the default. Peak
  0.14–0.6 GB, and 1.7–3.3 J per frame measured on the Mac's cores. Run
  its speculative uses at background QoS.
- **Metal** only when the frame fits the device's `maxBufferLength` (log
  it) and a budget of about a third of `os_proc_available_memory()`.
  Without plane splitting that is the 20–22 MP bodies; with it, the 24 MP
  ones.
- 1:1 and export: Metal when the frame fits, else the processor's full
  develop.
- To bring 45–50 MP frames to Metal on an iPhone: split the planes, f16
  for `lin` and `bent`, reuse the band buffers. Then run this bench's
  `iphone` phase again, and the tolerance test.

Not measured: anything on battery (the Mac was on AC in every run), a real
iPad or iPhone, and Metal's energy on Apple's smaller GPUs.
