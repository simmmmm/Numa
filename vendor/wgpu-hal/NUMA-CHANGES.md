# wgpu-hal, as Numa carries it

This is wgpu-hal 30.0.1 as published on crates.io, MIT or Apache-2.0
(`LICENSE.MIT`, `LICENSE.APACHE`), with the one change below. The workspace's
`[patch.crates-io]` puts it under wgpu 30; Numa-mac's workspace patches it the
same way (a `[patch]` only counts in the workspace being built).

Left out of the published crate: `src/dx12`, `src/gles` and `src/auxil/dxgi`
(backends Numa never enables: Vulkan on Linux, Metal on Apple), `Cargo.lock`
and `Cargo.toml.orig`. Enabling wgpu's `dx12` or `gles` feature would fail to
build, which is the intent.

## Changes

### RENDER-018: Metal compiles in safe math (28 September 2026)

- `src/metal/device.rs`, `load_shader`: `MTLCompileOptions.mathMode` is set
  to `Safe` (`fastMathEnabled = false` before macOS 15 / iOS 18). wgpu 30
  sets neither, so Metal compiles with its default, fast math, and has no
  option to change that — not in `wgpu::ShaderModuleDescriptor`, not in
  naga's MSL options; an MSL passthrough module is compiled with default
  options too.
- Why: under fast math five of nine test bodies fail Numa's tolerance test
  (PPG's direction ties flip); in safe math PPG is bit for bit the
  processor's on every Bayer body and all nine pass. It costs about 7 % of a
  Metal develop (`docs/MAC_BENCH.md`, runs 2 and 3).

To move to a newer wgpu: copy the new wgpu-hal from the registry, remove the
same folders, and apply the hunk in `src/metal/device.rs` again (search for
`NUMA`). Once wgpu exposes the math mode, drop this crate and the `[patch]`.
