// RENDER-017: the proxy's render on the card — numa-render's arithmetic, step
// for step, from its own source (`to_working_space`, `detail::denoise_colour`,
// `apply_basic`, `Look::apply`, `encode`). What each stage is for is written
// there; here is only how the card does it. `numa_render::card::plan` makes
// every table and constant, and says when the document asks for a stage this
// file does not have.
//
// One index per pixel: `colour` over the proxy (the camera's values into
// linear sRGB, into `lin`), `geometry` (the mirror, the quarter turn and the
// crop, into `work`, only when there are any), `blur_rows` (the first half of
// colour noise reduction's box, only when it is on) and `finish` (the rest of
// the stack, the eight bits, the histogram of every fourth pixel and the
// clipping overlay), those three over the frame rendered: `lin`, or `work`
// with geometry. `lin` is kept: a render whose colour stage is the last one's
// (any slider but the white balance and the profile) starts from it.
//
// RENDER-020: with masks, `finish` leaves the photograph's own adjustments in
// `work`; each mask is then a `mask` pass over it (after `mask_rows` where it
// has colour noise reduction of its own), faded in by its field, and `encode`
// does what comes after the masks and the eight bits.
//
// RENDER-021: the passes that read a pixel's neighbourhood work on planes of
// one value — log luminance, its box blurs, a guided filter's a and b — in
// `planes`, each pass told by its `step` what to read and write. Luminance
// noise reduction runs between the geometry and colour noise reduction
// (`luma_log`, a guided filter, `luma_apply` into `work`); HDR, Clarity and
// Texture after `finish` (which then also writes the log luminance): a
// quarter-size plane, its guided filter, the glow, Texture's guided filter
// over the frame, the base's mean (`pivot_sum`, `pivot_total`) and
// `tone_apply`.

// One set of per-pixel adjustments: the photograph's (`p.adjust`) or a
// mask's (`masks`). Offsets are into `tables`.
struct Adjust {
    flags: vec4<u32>,          // what it does, the tone curve's offset and length, the mixer table's offset
    basic: vec4<f32>,          // gain, contrast slope, saturation, vibrance
    mix_dims: vec4<u32>,
    mix_in: array<vec4<f32>, 3>,
    mix_out: array<vec4<f32>, 3>,
    places: vec4<u32>,         // the points' offset and count, the highlighted point's offset + 1, the grade's offset
    more: vec4<u32>,           // black and white's offset; a mask's curves' offset, field's offset, colour NR's radius
    gains: vec4<f32>,          // a mask's white balance gains, its colour NR's amount
    tint: vec4<f32>,           // a mask's Color at a luminance of 1, its strength
};

struct Params {
    size: vec4<u32>,           // width, height, flags, groups_x: the frame rendered
    src: vec4<u32>,            // the proxy's width, height, its groups_x, geometry flags
    out_info: vec4<u32>,       // row stride in pixels, overlay (1 shadows, 2 highlights), map offset, look offset
    mult: vec4<f32>,           // white balance multipliers, the lowest of them
    misc: vec4<f32>,           // clip (0 = none), colour NR amount, its radius, the base curve's toe
    m_in: array<vec4<f32>, 3>, // to ProPhoto (a profile), or the camera matrix
    m_out: array<vec4<f32>, 3>,// ProPhoto to linear sRGB
    map_dims: vec4<u32>,       // hue, saturation, value divisions, sRGB value axis
    look_dims: vec4<u32>,
    whites: vec4<f32>,         // the map's white, the look's
    curves: vec4<u32>,         // composite, red, green, blue: 1 where there is one
    curves_at: vec4<u32>,      // their offset
    tone_axis: vec4<f32>,      // ToneCurve's LOW and STEP
    crop: vec4<f32>,           // the crop's width and height, its centre in the turned frame
    crop2: vec4<f32>,          // the turned frame's half size, sin and cos of the angle
    keystone: vec4<f32>,       // vertical, horizontal, stretch
    vignette: array<vec4<f32>, 2>, // `effects::vignette_shape`
    adjust: Adjust,
    base: array<vec4<f32>, 14>,// the base curve's 53 values
};

const CAMERA: u32 = 1u;      // a raw: balance, clipping, profile
const RENDERING: u32 = 2u;   // with a DNG profile, else the matrix
const MAP: u32 = 4u;
const LOOK: u32 = 8u;
const CLIP: u32 = 16u;
const DENOISE: u32 = 32u;
const DISPLAY_REFERRED: u32 = 64u;
const GEOMETRY: u32 = 128u;
const VIGNETTE: u32 = 256u;
const MASKS: u32 = 512u;
const LUMA: u32 = 1024u;    // luminance noise reduction: the frame is in `work` after it
const LOCAL: u32 = 2048u;   // HDR, Clarity or Texture: `finish` into `work`, `encode` after

// `Adjust.flags.x`.
const BASIC: u32 = 1u;
const SLOPE: u32 = 2u;
const TONE: u32 = 4u;
const SATURATE: u32 = 8u;
const MIXER: u32 = 16u;
const POINTS: u32 = 32u;
const MONO: u32 = 64u;
const GRADE: u32 = 128u;
const GAINS: u32 = 256u;
const MASK_DENOISE: u32 = 512u;
const CURVES: u32 = 1024u;
const TINT: u32 = 2048u;
const CURVE_ONE: u32 = 4096u; // and the next three: composite, red, green, blue

// `p.src.w`: how the frame is turned.
const MIRROR: u32 = 1u;
const TRANSPOSE: u32 = 2u;
const FLIP_X: u32 = 4u;
const FLIP_Y: u32 = 8u;
const CROP: u32 = 16u;

const MIDDLE_GREY: f32 = 0.18;
const LOOKUP: u32 = 256u;

// What one dispatch is asked, `STEP` bytes apart in their buffer: offsets
// are into `planes`.
struct Step {
    what: vec4<u32>,           // the mask; a plane pass's feed and emit
    at: vec4<u32>,             // source, destination, guide, second source
    shape: vec4<u32>,          // the plane's width, height, the radius, groups_x
    more: vec4<u32>,           // tone_apply: the glow's, Texture's band's and the pivot's offsets (0: none)
    numbers: vec4<f32>,        // the guided filter's edge; luma_apply: contrast, amount; tone_apply: its four scales
};

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> src: array<f32>;
@group(0) @binding(2) var<storage, read_write> work: array<f32>;
@group(0) @binding(3) var<storage, read> tables: array<f32>;
@group(0) @binding(4) var<storage, read_write> rows: array<f32>;
@group(0) @binding(5) var<storage, read_write> out: array<u32>;
@group(0) @binding(6) var<storage, read_write> hist: array<atomic<u32>>;
// RENDER-020: which mask this pass is (a dynamic offset), every mask's
// adjustments, and their fields, two sixteen-bit weights a word.
@group(0) @binding(7) var<uniform> step: Step;
@group(0) @binding(8) var<storage, read> masks: array<Adjust>;
@group(0) @binding(9) var<storage, read> fields: array<u32>;
@group(0) @binding(10) var<storage, read_write> lin: array<f32>;
@group(0) @binding(11) var<storage, read_write> planes: array<f32>;

fn has(flag: u32) -> bool {
    return (p.size.z & flag) != 0u;
}

fn index_of(wid: vec3<u32>, lid: vec3<u32>) -> u32 {
    return (wid.y * p.size.w + wid.x) * 256u + lid.x;
}

fn source_index_of(wid: vec3<u32>, lid: vec3<u32>) -> u32 {
    return (wid.y * p.src.z + wid.x) * 256u + lid.x;
}

fn mul3(m: array<vec4<f32>, 3>, v: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        m[0].x * v.x + m[0].y * v.y + m[0].z * v.z,
        m[1].x * v.x + m[1].y * v.y + m[1].z * v.z,
        m[2].x * v.x + m[2].y * v.y + m[2].z * v.z,
    );
}

fn luminance(v: vec3<f32>) -> f32 {
    return 0.2126 * v.x + 0.7152 * v.y + 0.0722 * v.z;
}

// f32::rem_euclid(6.0), for the hues in sixths of a turn.
fn rem6(x: f32) -> f32 {
    let r = x - 6.0 * floor(x / 6.0);
    return select(r, 0.0, r >= 6.0);
}

fn rgb_to_hsv(rgb: vec3<f32>) -> vec3<f32> {
    let mx = max(max(rgb.x, rgb.y), rgb.z);
    let mn = min(min(rgb.x, rgb.y), rgb.z);
    let range = mx - mn;
    if (range <= 0.0 || mx <= 0.0) {
        return vec3<f32>(0.0, 0.0, mx);
    }
    var hue: f32;
    if (mx == rgb.x) {
        hue = (rgb.y - rgb.z) / range;
    } else if (mx == rgb.y) {
        hue = 2.0 + (rgb.z - rgb.x) / range;
    } else {
        hue = 4.0 + (rgb.x - rgb.y) / range;
    }
    return vec3<f32>(rem6(hue), range / mx, mx);
}

fn hsv_to_rgb(hsv: vec3<f32>) -> vec3<f32> {
    let v = hsv.z;
    let s = hsv.y;
    if (s <= 0.0) {
        return vec3<f32>(v);
    }
    let hue = rem6(hsv.x);
    let sector = floor(hue);
    let f = hue - sector;
    let pp = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    switch (i32(sector)) {
        case 0: { return vec3<f32>(v, t, pp); }
        case 1: { return vec3<f32>(q, v, pp); }
        case 2: { return vec3<f32>(pp, v, t); }
        case 3: { return vec3<f32>(pp, q, v); }
        case 4: { return vec3<f32>(t, pp, v); }
        default: { return vec3<f32>(v, pp, q); }
    }
}

fn srgb_encode(value: f32) -> f32 {
    let v = clamp(value, 0.0, 1.0);
    if (v <= 0.0031308) {
        return v * 12.92;
    }
    return 1.055 * pow(v, 1.0 / 2.4) - 0.055;
}

// `Table::at`: value-major, then hue, then saturation.
fn entry(start: u32, dims: vec4<u32>, h: u32, s: u32, v: u32) -> vec3<f32> {
    let at = start + ((v * dims.x + h) * dims.y + s) * 3u;
    return vec3<f32>(tables[at], tables[at + 1u], tables[at + 2u]);
}

// `Table::lookup`.
fn lookup(start: u32, dims: vec4<u32>, hsv: vec3<f32>) -> vec3<f32> {
    let hue = hsv.x * f32(dims.x) / 6.0;
    let hf = floor(hue);
    let ht = hue - hf;
    let n = i32(dims.x);
    let h0 = u32(((i32(hf) % n) + n) % n);
    let h1 = (h0 + 1u) % dims.x;
    let top_s = f32(dims.y - 1u);
    let sat = clamp(hsv.y * top_s, 0.0, top_s);
    let s0 = u32(floor(sat));
    let s1 = min(s0 + 1u, dims.y - 1u);
    let st = sat - f32(s0);
    var v0 = 0u;
    var v1 = 0u;
    var vt = 0.0;
    if (dims.z > 1u) {
        var encoded = hsv.z;
        if (dims.w != 0u) {
            encoded = srgb_encode(hsv.z);
        }
        let top_v = f32(dims.z - 1u);
        let value = clamp(encoded * top_v, 0.0, top_v);
        v0 = u32(floor(value));
        v1 = min(v0 + 1u, dims.z - 1u);
        vt = value - f32(v0);
    }
    let a0 = mix(entry(start, dims, h0, s0, v0), entry(start, dims, h0, s1, v0), st);
    let b0 = mix(entry(start, dims, h1, s0, v0), entry(start, dims, h1, s1, v0), st);
    let plane0 = mix(a0, b0, ht);
    if (dims.z <= 1u) {
        return plane0;
    }
    let a1 = mix(entry(start, dims, h0, s0, v1), entry(start, dims, h0, s1, v1), st);
    let b1 = mix(entry(start, dims, h1, s0, v1), entry(start, dims, h1, s1, v1), st);
    return mix(plane0, mix(a1, b1, ht), vt);
}

// `Table::apply`, looked up at `white` (`Rendering::through`).
fn apply_table(start: u32, dims: vec4<u32>, white: f32, rgb_in: vec3<f32>) -> vec3<f32> {
    let rgb = rgb_in / white;
    var hsv = rgb_to_hsv(rgb);
    if (hsv.z <= 0.0) {
        return rgb * white;
    }
    let c = lookup(start, dims, hsv);
    hsv.x = rem6(hsv.x + c.x / 60.0);
    hsv.y = clamp(hsv.y * c.y, 0.0, 1.0);
    hsv.z = hsv.z * c.z;
    return hsv_to_rgb(hsv) * white;
}

// `to_working_space`, for one pixel.
@compute @workgroup_size(256)
fn colour(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let i = source_index_of(wid, lid);
    if (i >= p.src.x * p.src.y) {
        return;
    }
    let camera = vec3<f32>(src[i * 3u], src[i * 3u + 1u], src[i * 3u + 2u]);
    var rgb = camera;
    if (has(CAMERA)) {
        let mult = p.mult.xyz;
        var balanced = camera * mult;
        // `neutralise_clipping`.
        if (has(CLIP)) {
            let clip = p.misc.x;
            let highest = max(max(camera.x, camera.y), camera.z);
            let blend = clamp((highest - clip * 0.9) / (clip * 0.1), 0.0, 1.0);
            if (blend > 0.0) {
                let held = min(balanced, vec3<f32>(clip * p.mult.w));
                balanced = balanced + (held - balanced) * blend;
            }
        }
        if (has(RENDERING)) {
            rgb = mul3(p.m_in, balanced);
            if (has(MAP)) {
                rgb = apply_table(p.out_info.z, p.map_dims, p.whites.x, rgb);
            }
            if (has(LOOK)) {
                rgb = apply_table(p.out_info.w, p.look_dims, p.whites.y, rgb);
            }
            rgb = max(mul3(p.m_out, rgb), vec3<f32>(0.0));
        } else {
            rgb = mul3(p.m_in, balanced / mult);
        }
        rgb = max(rgb, vec3<f32>(0.0));
    }
    lin[i * 3u] = rgb.x;
    lin[i * 3u + 1u] = rgb.y;
    lin[i * 3u + 2u] = rgb.z;
}

fn turned(flag: u32) -> bool {
    return (p.src.w & flag) != 0u;
}

// `LinearImage::oriented`, twice: a pixel of the mirrored and turned frame
// back to the proxy's, as an index into `lin`.
fn from_turned(x: u32, y: u32) -> u32 {
    let w = p.src.x;
    let h = p.src.y;
    var fx = x;
    var fy = y;
    if (turned(TRANSPOSE)) {
        fx = y;
        fy = x;
    }
    if (turned(FLIP_X)) {
        fx = w - 1u - fx;
    }
    if (turned(FLIP_Y)) {
        fy = h - 1u - fy;
    }
    if (turned(MIRROR)) {
        fx = w - 1u - fx;
    }
    return (fy * w + fx) * 3u;
}

fn turned_pixel(x: i32, y: i32, tw: i32, th: i32) -> vec3<f32> {
    let at = from_turned(u32(clamp(x, 0, tw - 1)), u32(clamp(y, 0, th - 1)));
    return vec3<f32>(lin[at], lin[at + 1u], lin[at + 2u]);
}

// `LinearImage::sample` over the turned frame: bilinear, black beyond it.
fn sample(x: f32, y: f32, tw: i32, th: i32) -> vec3<f32> {
    if (x < -1.0 || y < -1.0 || x > f32(tw) || y > f32(th)) {
        return vec3<f32>(0.0);
    }
    let x0 = floor(x);
    let y0 = floor(y);
    let tx = x - x0;
    let ty = y - y0;
    let ix = i32(x0);
    let iy = i32(y0);
    let a = turned_pixel(ix, iy, tw, th);
    let b = turned_pixel(ix + 1, iy, tw, th);
    let c = turned_pixel(ix, iy + 1, tw, th);
    let d = turned_pixel(ix + 1, iy + 1, tw, th);
    let top = a + (b - a) * tx;
    let bottom = c + (d - c) * tx;
    return top + (bottom - top) * ty;
}

// `geometry_of`: the mirror, the quarter turn and `LinearImage::cropped`.
@compute @workgroup_size(256)
fn geometry(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let i = index_of(wid, lid);
    let width = p.size.x;
    if (i >= width * p.size.y) {
        return;
    }
    let column = i % width;
    let row = i / width;
    var pixel: vec3<f32>;
    if (turned(CROP)) {
        var tw = i32(p.src.x);
        var th = i32(p.src.y);
        if (turned(TRANSPOSE)) {
            tw = i32(p.src.y);
            th = i32(p.src.x);
        }
        // `source_map`.
        let dy = f32(row) - p.crop.y / 2.0 + 0.5;
        let dx = f32(column) - p.crop.x / 2.0 + 0.5;
        let sin = p.crop2.z;
        let cos = p.crop2.w;
        let stretch = p.keystone.z;
        let rx = (dx * cos - dy * sin) / stretch;
        let ry = (dx * sin + dy * cos) * stretch;
        let qx = p.crop.z + rx - p.crop2.x;
        let qy = p.crop.w + ry - p.crop2.y;
        let depth = 1.0 - p.keystone.x * (qy / p.crop2.y) - p.keystone.y * (qx / p.crop2.x);
        let reach = 1.0 / max(depth, 0.05);
        let sx = p.crop2.x + qx * reach;
        let sy = p.crop2.y + qy * reach;
        pixel = sample(sx - 0.5, sy - 0.5, tw, th);
    } else {
        let at = from_turned(column, row);
        pixel = vec3<f32>(lin[at], lin[at + 1u], lin[at + 2u]);
    }
    work[i * 3u] = pixel.x;
    work[i * 3u + 1u] = pixel.y;
    work[i * 3u + 2u] = pixel.z;
}

fn rows_at(j: u32) -> vec3<f32> {
    return vec3<f32>(rows[j * 3u], rows[j * 3u + 1u], rows[j * 3u + 2u]);
}

fn work_at(j: u32) -> vec3<f32> {
    return vec3<f32>(work[j * 3u], work[j * 3u + 1u], work[j * 3u + 2u]);
}

// The frame before luminance noise reduction: the colour stage's, or the
// geometry's.
fn shaped_at(j: u32) -> vec3<f32> {
    if (has(GEOMETRY)) {
        return work_at(j);
    }
    return vec3<f32>(lin[j * 3u], lin[j * 3u + 1u], lin[j * 3u + 2u]);
}

// The frame before `finish`: that, or luminance noise reduction's.
fn frame_at(j: u32) -> vec3<f32> {
    if (has(LUMA)) {
        return work_at(j);
    }
    return shaped_at(j);
}

fn put_work(i: u32, pixel: vec3<f32>) {
    work[i * 3u] = pixel.x;
    work[i * 3u + 1u] = pixel.y;
    work[i * 3u + 2u] = pixel.z;
}

// Colour noise reduction, first half: the box along the row — of the frame,
// or of `work` with a mask's gains on it first, the mask's own copy.
fn box_row(i: u32, r: i32, gains: vec3<f32>, masked: bool) {
    let width = p.size.x;
    let x = i32(i % width);
    let y = i / width;
    var sum = vec3<f32>(0.0);
    for (var dx = -r; dx <= r; dx++) {
        let j = y * width + u32(clamp(x + dx, 0, i32(width) - 1));
        if (masked) {
            sum += max(work_at(j) * gains, vec3<f32>(0.0));
        } else {
            sum += frame_at(j);
        }
    }
    let window = f32(2 * r + 1);
    rows[i * 3u] = sum.x / window;
    rows[i * 3u + 1u] = sum.y / window;
    rows[i * 3u + 2u] = sum.z / window;
}

@compute @workgroup_size(256)
fn blur_rows(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let i = index_of(wid, lid);
    if (i < p.size.x * p.size.y) {
        box_row(i, min(i32(p.misc.z), 64), vec3<f32>(1.0), false);
    }
}

@compute @workgroup_size(256)
fn mask_rows(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let i = index_of(wid, lid);
    if (i < p.size.x * p.size.y) {
        let m = masks[step.what.x];
        box_row(i, min(i32(m.more.w), 64), m.gains.xyz, true);
    }
}

// `detail::blur_colour`'s second half: the box down the column of `rows`,
// and its colour put back at the pixel's own brightness.
fn recolour(pixel: vec3<f32>, i: u32, r: i32, amount: f32) -> vec3<f32> {
    let width = p.size.x;
    let x = i % width;
    let y = i / width;
    var soft = vec3<f32>(0.0);
    for (var dy = -r; dy <= r; dy++) {
        soft += rows_at(u32(clamp(i32(y) + dy, 0, i32(p.size.y) - 1)) * width + x);
    }
    soft = soft / f32(2 * r + 1);
    let soft_luma = luminance(soft);
    if (soft_luma <= 1e-6) {
        return pixel;
    }
    let recoloured = soft * (luminance(pixel) / soft_luma);
    return max(pixel + (recoloured - pixel) * amount, vec3<f32>(0.0));
}

// `ToneCurve::gain`.
fn tone_gain(a: Adjust, luma: f32) -> f32 {
    let last = f32(a.flags.z - 1u);
    let at = clamp((log2(max(luma, 1e-12)) - p.tone_axis.x) / p.tone_axis.y, 0.0, last);
    let below = u32(at);
    let above = min(below + 1u, a.flags.z - 1u);
    let t = at - f32(below);
    let lo = tables[a.flags.y + below];
    let hi = tables[a.flags.y + above];
    return exp2(lo + (hi - lo) * t);
}

fn does(a: Adjust, flag: u32) -> bool {
    return (a.flags.x & flag) != 0u;
}

// `apply_basic`, for one pixel.
fn basic(pixel_in: vec3<f32>, a: Adjust) -> vec3<f32> {
    var pixel = max(pixel_in * a.basic.x, vec3<f32>(0.0));
    if (does(a, SLOPE)) {
        pixel = MIDDLE_GREY * pow(pixel / MIDDLE_GREY, vec3<f32>(a.basic.y));
    }
    if (does(a, TONE)) {
        pixel = pixel * tone_gain(a, luminance(pixel));
    }
    if (does(a, SATURATE)) {
        let luma = luminance(pixel);
        let high = max(max(pixel.x, pixel.y), pixel.z);
        let low = min(min(pixel.x, pixel.y), pixel.z);
        var current = 0.0;
        if (high > 0.0) {
            current = (high - low) / high;
        }
        let factor = 1.0 + a.basic.z / 100.0 + (a.basic.w / 100.0) * (1.0 - current);
        pixel = max(luma + (pixel - luma) * factor, vec3<f32>(0.0));
    }
    return pixel;
}

// f32::rem_euclid(360.0).
fn rem360(x: f32) -> f32 {
    let r = x - 360.0 * floor(x / 360.0);
    return select(r, 0.0, r >= 360.0);
}

fn cbrt(x: f32) -> f32 {
    return sign(x) * pow(abs(x), 1.0 / 3.0);
}

// `point::oklch` and `point::from_oklch`.
fn oklch(rgb: vec3<f32>) -> vec3<f32> {
    let l = cbrt(0.41222147 * rgb.x + 0.53633255 * rgb.y + 0.05144599 * rgb.z);
    let m = cbrt(0.2119035 * rgb.x + 0.6806995 * rgb.y + 0.10739696 * rgb.z);
    let s = cbrt(0.08830246 * rgb.x + 0.28171885 * rgb.y + 0.6299787 * rgb.z);
    let lightness = 0.21045426 * l + 0.7936178 * m - 0.004072047 * s;
    let a = 1.9779985 * l - 2.4285922 * m + 0.4505937 * s;
    let b = 0.025904037 * l + 0.78277177 * m - 0.80867577 * s;
    return vec3<f32>(lightness, sqrt(a * a + b * b), rem360(degrees(atan2(b, a))));
}

fn from_oklch(lch: vec3<f32>) -> vec3<f32> {
    let a = lch.y * cos(radians(lch.z));
    let b = lch.y * sin(radians(lch.z));
    let l = lch.x + 0.39633778 * a + 0.21580376 * b;
    let m = lch.x - 0.105561346 * a - 0.06385417 * b;
    let s = lch.x - 0.08948418 * a - 1.2914855 * b;
    let l3 = l * l * l;
    let m3 = m * m * m;
    let s3 = s * s * s;
    return vec3<f32>(
        4.0767417 * l3 - 3.3077116 * m3 + 0.23096994 * s3,
        -1.268438 * l3 + 2.6097574 * m3 - 0.34131938 * s3,
        -0.0041960863 * l3 - 0.7034186 * m3 + 1.7076147 * s3,
    );
}

// `PointColour::weight`: `target` is the lightness, chroma, hue and the range
// as a fraction.
fn point_weight(aim: vec4<f32>, lch: vec3<f32>) -> f32 {
    let wide = aim.w;
    let turned = rem360(lch.z - aim.z);
    let apart = min(turned, 360.0 - turned) * min(min(lch.y, aim.y) / 0.04, 1.0);
    let d_hue = apart / (15.0 + 45.0 * wide);
    let d_chroma = (lch.y - aim.y) / (0.05 + 0.15 * wide);
    let d_light = (lch.x - aim.x) / (0.2 + 0.4 * wide);
    let t = clamp(1.5 - sqrt(d_hue * d_hue + d_chroma * d_chroma + d_light * d_light), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn table4(at: u32) -> vec4<f32> {
    return vec4<f32>(tables[at], tables[at + 1u], tables[at + 2u], tables[at + 3u]);
}

// `apply_point_colours` in sRGB: `PointColours::apply`, floored.
fn point_colours(rgb: vec3<f32>, a: Adjust) -> vec3<f32> {
    let original = oklch(rgb);
    if (original.x <= 1e-4) {
        return max(rgb, vec3<f32>(0.0));
    }
    let colourful = min(original.y / 0.04, 1.0);
    var lch = original;
    for (var k = 0u; k < min(a.places.y, 64u); k++) {
        let at = a.places.x + k * 8u;
        let weight = point_weight(table4(at), original);
        if (weight <= 0.0) {
            continue;
        }
        let moves = table4(at + 4u) / 100.0;
        lch.x *= 1.0 + weight * colourful * moves.z * 0.5;
        lch.y = max(lch.y * (1.0 + weight * moves.y), 0.0);
        lch.z += weight * moves.x * 30.0;
    }
    var out = rgb;
    if (any(lch != original)) {
        out = from_oklch(lch);
    }
    if (a.places.z != 0u) {
        let weight = point_weight(table4(a.places.z - 1u), original);
        let grey = luminance(out);
        out = grey + (out - grey) * weight;
    }
    return max(out, vec3<f32>(0.0));
}

// `run_operations`' per-pixel part: the basic adjustments, the mixer's table
// in ProPhoto (`Look::apply`), the point colours.
fn adjust(pixel_in: vec3<f32>, a: Adjust) -> vec3<f32> {
    var pixel = pixel_in;
    if (does(a, BASIC)) {
        pixel = basic(pixel, a);
    }
    if (does(a, MIXER)) {
        let wide = mul3(a.mix_in, pixel);
        let looked = apply_table(a.flags.w, a.mix_dims, 1.0, wide);
        pixel = max(mul3(a.mix_out, looked), vec3<f32>(0.0));
    }
    if (does(a, POINTS)) {
        pixel = point_colours(pixel, a);
    }
    return pixel;
}

// `Mixer::grey_gain`: the band from the encoded hue, faded by the chroma.
fn grey_gain(rgb: vec3<f32>, at: u32) -> f32 {
    let c = pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
    let high = max(max(c.x, c.y), c.z);
    let low = min(min(c.x, c.y), c.z);
    if (high <= 0.0 || high == low) {
        return 1.0;
    }
    let chroma = high - low;
    var hue: f32;
    if (high == c.x) {
        hue = (c.y - c.z) / chroma;
    } else if (high == c.y) {
        hue = (c.z - c.x) / chroma + 2.0;
    } else {
        hue = (c.x - c.y) / chroma + 4.0;
    }
    // `surrounding`, over `mixer::BANDS`' hues.
    let bands = array<f32, 8>(0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 270.0, 300.0);
    let h = rem360(60.0 * hue);
    var below = 0u;
    var above = 0u;
    var blend = 0.0;
    for (var k = 0u; k < 8u; k++) {
        let next = (k + 1u) % 8u;
        let span = rem360(bands[next] - bands[k]);
        let along = rem360(h - bands[k]);
        if (along < span) {
            below = k;
            above = next;
            blend = select(0.0, along / span, span > 0.0);
            break;
        }
    }
    let value = tables[at + below] + (tables[at + above] - tables[at + below]) * blend;
    return exp2(value * chroma / high);
}

// `Grading::apply`.
fn grade(rgb: vec3<f32>, at: u32) -> vec3<f32> {
    let shape = table4(at + 16u);
    let shifted = pow(clamp(base_curve(max(luminance(rgb), 0.0)), 0.0, 1.0), shape.x);
    let shadow = pow(1.0 - shifted, shape.y);
    let highlight = pow(shifted, shape.y);
    let weights = vec4<f32>(shadow, max(1.0 - shadow - highlight, 0.0), highlight, 1.0);
    var gain = vec3<f32>(1.0);
    var stops = 0.0;
    for (var k = 0u; k < 4u; k++) {
        let tint = table4(at + k * 4u);
        gain *= 1.0 + tint.xyz * weights[k];
        stops += weights[k] * tint.w;
    }
    return max(rgb * gain * exp2(stops), vec3<f32>(0.0));
}

// After the masks: black and white, then the grade.
fn after(pixel_in: vec3<f32>, a: Adjust) -> vec3<f32> {
    var pixel = pixel_in;
    if (does(a, MONO)) {
        pixel = vec3<f32>(luminance(pixel) * grey_gain(pixel, a.more.x));
    }
    if (does(a, GRADE)) {
        pixel = grade(pixel, a.places.w);
    }
    return pixel;
}

// `effects::vignette` over the whole frame.
fn vignette(pixel: vec3<f32>, x: u32, y: u32) -> vec3<f32> {
    let v0 = p.vignette[0];
    let v1 = p.vignette[1];
    let aspect = v0.y;
    let dx = ((f32(x) + 0.5) / f32(p.size.x) - 0.5) * 2.0;
    let dy = ((f32(y) + 0.5) / f32(p.size.y) - 0.5) * 2.0;
    var c = vec2<f32>(dx, dy / aspect);
    if (aspect >= 1.0) {
        c = vec2<f32>(dx * aspect, dy);
    }
    let e = vec2<f32>(dx, dy) + (c - vec2<f32>(dx, dy)) * v0.z;
    let power = v0.w;
    var distance: f32;
    if (power == 2.0) {
        distance = length(e);
    } else {
        distance = pow(pow(abs(e.x), power) + pow(abs(e.y), power), 1.0 / power) * v1.z;
    }
    let low = v1.x - v1.y * 0.5;
    let t = clamp((distance - low) / max(v1.y, 1e-6), 0.0, 1.0);
    return pixel * exp2(v0.x * t * t * (3.0 - 2.0 * t));
}

// `tone::curve`: the cameras' curve in log2 exposure, between its quarter stops.
fn base_curve(value: f32) -> f32 {
    let stops = log2(max(value, 1e-6) / MIDDLE_GREY);
    let at = (stops + 8.0) * 4.0;
    if (at <= 0.0) {
        return p.base[0].x * exp2(at / 4.0 / p.misc.w);
    }
    let low = u32(floor(at));
    let t = fract(at);
    if (low + 1u >= 53u) {
        return p.base[13].x;
    }
    let a = p.base[low / 4u][low % 4u];
    let b = p.base[(low + 1u) / 4u][(low + 1u) % 4u];
    return a * (1.0 - t) + b * t;
}

fn base_at(k: u32) -> f32 {
    return p.base[k / 4u][k % 4u];
}

// `tone::scene_value_for`: the curve backwards, its segment found rather than
// bisected for.
fn scene_value_for(display: f32) -> f32 {
    let wanted = clamp(display, 1e-4, 1.0 - 1e-4);
    var at: f32;
    if (wanted <= base_at(0u)) {
        at = 4.0 * p.misc.w * log2(wanted / base_at(0u));
    } else {
        // The last stop below: base_at(low) < wanted <= base_at(low + 1).
        var low = 0u;
        var high = 52u;
        for (var step = 0u; step < 6u; step++) {
            let middle = (low + high) / 2u;
            if (base_at(middle) < wanted) {
                low = middle;
            } else {
                high = middle;
            }
        }
        at = f32(low) + (wanted - base_at(low)) / (base_at(low + 1u) - base_at(low));
    }
    return MIDDLE_GREY * exp2(max(at / 4.0 - 8.0, -24.0));
}

fn srgb_decode(code: f32) -> f32 {
    let c = clamp(code, 0.0, 1.0);
    if (c <= 0.04045) {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

// `through`: one display value through a curve's lookup.
fn through_at(start: u32, display: f32) -> f32 {
    let at = clamp(display, 0.0, 1.0) * f32(LOOKUP - 1u);
    let low = u32(floor(at));
    let high = min(low + 1u, LOOKUP - 1u);
    let fraction = at - f32(low);
    return tables[start + low] * (1.0 - fraction) + tables[start + high] * fraction;
}

fn through(curve: u32, display: f32) -> f32 {
    return through_at(p.curves_at.x + curve * LOOKUP, display);
}

// `finish_mask`'s curves: up to the display, through them, and back.
fn mask_curves(pixel: vec3<f32>, a: Adjust) -> vec3<f32> {
    var out = pixel;
    for (var channel = 0u; channel < 3u; channel++) {
        var display: f32;
        if (has(DISPLAY_REFERRED)) {
            display = srgb_encode(pixel[channel]);
        } else {
            display = clamp(base_curve(pixel[channel]), 0.0, 1.0);
        }
        if (does(a, CURVE_ONE)) {
            display = through_at(a.more.y, display);
        }
        if (does(a, CURVE_ONE << (channel + 1u))) {
            display = through_at(a.more.y + (channel + 1u) * LOOKUP, display);
        }
        if (has(DISPLAY_REFERRED)) {
            out[channel] = srgb_decode(display);
        } else {
            out[channel] = scene_value_for(display);
        }
    }
    return out;
}

// `encode`'s eight bits for one channel: `tone::shown`, the curves, quantised.
fn code(channel: u32, value: f32) -> u32 {
    var display: f32;
    if (has(DISPLAY_REFERRED)) {
        display = srgb_encode(value);
    } else {
        display = clamp(base_curve(value), 0.0, 1.0);
    }
    if (p.curves.x != 0u) {
        display = through(0u, display);
    }
    if (p.curves[channel + 1u] != 0u) {
        display = through(channel + 1u, display);
    }
    return u32(clamp(display * 255.0 + 0.5, 0.0, 255.0));
}

var<workgroup> local_hist: array<atomic<u32>, 770>;

// Everything after the masks, the eight bits, the histogram of every fourth
// pixel and the clipping overlay.
fn show(i: u32, pixel_in: vec3<f32>) {
    let x = i % p.size.x;
    let y = i / p.size.x;
    var pixel = after(pixel_in, p.adjust);
    if (has(VIGNETTE)) {
        pixel = vignette(pixel, x, y);
    }
    let r8 = code(0u, pixel.x);
    let g8 = code(1u, pixel.y);
    let b8 = code(2u, pixel.z);
    if (i % 4u == 0u) {
        atomicAdd(&local_hist[r8], 1u);
        atomicAdd(&local_hist[256u + g8], 1u);
        atomicAdd(&local_hist[512u + b8], 1u);
        if (r8 == 0u || g8 == 0u || b8 == 0u) {
            atomicAdd(&local_hist[768u], 1u);
        }
        if (r8 == 255u || g8 == 255u || b8 == 255u) {
            atomicAdd(&local_hist[769u], 1u);
        }
    }
    // `histogram::mark_clipping`, on the picture and not in its histogram.
    var packed = r8 | (g8 << 8u) | (b8 << 16u) | (255u << 24u);
    let overlay = p.out_info.y;
    if ((overlay & 2u) != 0u && (r8 == 255u || g8 == 255u || b8 == 255u)) {
        packed = 255u | (40u << 8u) | (40u << 16u) | (255u << 24u);
    } else if ((overlay & 1u) != 0u && (r8 == 0u || g8 == 0u || b8 == 0u)) {
        packed = 40u | (90u << 8u) | (255u << 16u) | (255u << 24u);
    }
    out[y * p.out_info.x + x] = packed;
}

fn clear_hist(lid: vec3<u32>) {
    for (var k = lid.x; k < 770u; k += 256u) {
        atomicStore(&local_hist[k], 0u);
    }
}

fn flush_hist(lid: vec3<u32>) {
    for (var k = lid.x; k < 770u; k += 256u) {
        let n = atomicLoad(&local_hist[k]);
        if (n > 0u) {
            atomicAdd(&hist[k], n);
        }
    }
}

// Colour noise reduction's column and recolouring, then everything after
// `prefix`: straight to the eight bits, or — with masks — back into `work`.
@compute @workgroup_size(256)
fn finish(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    clear_hist(lid);
    workgroupBarrier();
    let i = index_of(wid, lid);
    if (i < p.size.x * p.size.y) {
        var pixel = frame_at(i);
        if (has(DENOISE)) {
            pixel = recolour(pixel, i, min(i32(p.misc.z), 64), p.misc.y);
        }
        pixel = adjust(pixel, p.adjust);
        if (has(LOCAL)) {
            planes[i] = log_luma(pixel);
        }
        if (has(MASKS) || has(LOCAL)) {
            put_work(i, pixel);
        } else {
            show(i, pixel);
        }
    }
    workgroupBarrier();
    flush_hist(lid);
}

// RENDER-020: one mask of `apply_masks` — its copy of the frame, worked out
// where its field reaches and faded in by it.
@compute @workgroup_size(256)
fn mask(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let i = index_of(wid, lid);
    if (i >= p.size.x * p.size.y) {
        return;
    }
    let m = masks[step.what.x];
    let pair = unpack2x16unorm(fields[m.more.z + i / 2u]);
    let weight = select(pair.x, pair.y, (i & 1u) != 0u);
    if (weight <= 0.0) {
        return;
    }
    let base = work_at(i);
    var local = base;
    if (does(m, GAINS)) {
        local = max(local * m.gains.xyz, vec3<f32>(0.0));
    }
    if (does(m, MASK_DENOISE)) {
        local = recolour(local, i, min(i32(m.more.w), 64), m.gains.w);
    }
    local = adjust(local, m);
    if (does(m, CURVES)) {
        local = mask_curves(local, m);
    }
    local = after(local, m);
    if (does(m, TINT)) {
        let luma = luminance(local);
        local += (luma * m.tint.xyz - local) * m.tint.w;
    }
    put_work(i, base + (local - base) * weight);
}

// What comes after the masks, and the eight bits.
@compute @workgroup_size(256)
fn encode(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    clear_hist(lid);
    workgroupBarrier();
    let i = index_of(wid, lid);
    if (i < p.size.x * p.size.y) {
        show(i, work_at(i));
    }
    workgroupBarrier();
    flush_hist(lid);
}

// RENDER-021 ------------------------------------------------------------------

fn log_luma(pixel: vec3<f32>) -> f32 {
    return log2(max(luminance(pixel), 1e-5));
}

// A step's own index over its plane.
fn plane_index(wid: vec3<u32>, lid: vec3<u32>) -> u32 {
    return (wid.y * step.shape.w + wid.x) * 256u + lid.x;
}

// How `plane_rows` reads its source (`step.what.y`), and what `plane_cols`
// makes of the two means (`step.what.z`).
const FEED_ONE: u32 = 0u;
const FEED_SQUARES: u32 = 1u;
const FEED_PAIR: u32 = 2u;
const FEED_DIFFERENCE: u32 = 3u;
const EMIT_ONE: u32 = 0u;
const EMIT_AB: u32 = 1u;
const EMIT_GUIDED: u32 = 2u;

fn feed(j: u32) -> vec2<f32> {
    let at = step.at.x;
    switch (step.what.y) {
        case 1u: {
            let v = planes[at + j];
            return vec2<f32>(v, v * v);
        }
        case 2u: {
            return vec2<f32>(planes[at + 2u * j], planes[at + 2u * j + 1u]);
        }
        case 3u: {
            return vec2<f32>(planes[at + j] - planes[step.at.w + j], 0.0);
        }
        default: {
            return vec2<f32>(planes[at + j], 0.0);
        }
    }
}

// How many pixels one invocation of a column's box walks: the window is
// summed once and then slid, as `plane::blur_columns` does down a whole
// column. (Along a row the neighbours' reads are the ones next door, and one
// invocation a pixel is faster.)
const SEGMENT: u32 = 64u;

// `plane::blur_rows`, of one or two values: always two out, side by side.
@compute @workgroup_size(256)
fn plane_rows(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let i = plane_index(wid, lid);
    let width = step.shape.x;
    if (i >= width * step.shape.y) {
        return;
    }
    let r = i32(min(step.shape.z, 256u));
    let x = i32(i % width);
    let row = (i / width) * width;
    var sum = vec2<f32>(0.0);
    for (var dx = -r; dx <= r; dx++) {
        sum += feed(row + u32(clamp(x + dx, 0, i32(width) - 1)));
    }
    let mean = sum / f32(2 * r + 1);
    planes[step.at.y + 2u * i] = mean.x;
    planes[step.at.y + 2u * i + 1u] = mean.y;
}

fn pair_at(j: u32) -> vec2<f32> {
    return vec2<f32>(planes[step.at.x + 2u * j], planes[step.at.x + 2u * j + 1u]);
}

// `plane::blur_columns` of what `plane_rows` left, and then: the mean; a
// guided filter's a and b (`guided_by` with itself); or its answer, a × the
// guide + b. One invocation a segment of a column.
@compute @workgroup_size(256)
fn plane_cols(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let t = plane_index(wid, lid);
    let width = step.shape.x;
    let height = step.shape.y;
    if (t >= width * ((height + SEGMENT - 1u) / SEGMENT)) {
        return;
    }
    let r = i32(min(step.shape.z, 256u));
    let x = t % width;
    let first = (t / width) * SEGMENT;
    let last = i32(height) - 1;
    var sum = vec2<f32>(0.0);
    for (var dy = -r; dy <= r; dy++) {
        sum += pair_at(u32(clamp(i32(first) + dy, 0, last)) * width + x);
    }
    let window = f32(2 * r + 1);
    for (var y = first; y < min(first + SEGMENT, height); y++) {
        let mean = sum / window;
        let i = y * width + x;
        switch (step.what.z) {
            case 1u: {
                let variance = max(mean.y - mean.x * mean.x, 0.0);
                let weight = (mean.y - mean.x * mean.x) / (variance + step.numbers.x);
                planes[step.at.y + 2u * i] = weight;
                planes[step.at.y + 2u * i + 1u] = mean.x - weight * mean.x;
            }
            case 2u: {
                planes[step.at.y + i] = mean.x * planes[step.at.z + i] + mean.y;
            }
            default: {
                planes[step.at.y + i] = mean.x;
            }
        }
        sum += pair_at(u32(min(i32(y) + r + 1, last)) * width + x) - pair_at(u32(max(i32(y) - r, 0)) * width + x);
    }
}

// Luminance noise reduction's log luminance of the frame.
@compute @workgroup_size(256)
fn luma_log(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let i = index_of(wid, lid);
    if (i < p.size.x * p.size.y) {
        planes[i] = log_luma(shaped_at(i));
    }
}

// `detail::denoise`'s gain: the smoothed log luminance (`at.y`), the coarse
// band of what it took out put back (`at.z`), taken this much.
@compute @workgroup_size(256)
fn luma_apply(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let i = index_of(wid, lid);
    if (i >= p.size.x * p.size.y) {
        return;
    }
    var back = 0.0;
    if (step.numbers.x > 0.0) {
        back = planes[step.at.z + i] * step.numbers.x;
    }
    let gain = exp2((planes[step.at.y + i] + back - planes[i]) * step.numbers.y);
    // Read before written: with geometry the frame is `work` itself.
    put_work(i, max(shaped_at(i) * gain, vec3<f32>(0.0)));
}

// `plane::subsample`: the log luminance a quarter the size, box by box.
@compute @workgroup_size(256)
fn subsample(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let i = plane_index(wid, lid);
    let width = step.shape.x;
    if (i >= width * step.shape.y) {
        return;
    }
    let factor = step.shape.z;
    let x = i % width;
    let y = i / width;
    var sum = 0.0;
    for (var dy = 0u; dy < factor; dy++) {
        let sy = min(y * factor + dy, p.size.y - 1u);
        for (var dx = 0u; dx < factor; dx++) {
            sum += planes[sy * p.size.x + min(x * factor + dx, p.size.x - 1u)];
        }
    }
    planes[step.at.y + i] = sum / f32(factor * factor);
}

// `plane::upsample` of a small plane, at one pixel of the frame.
fn upsampled(at: u32, width: u32, height: u32, x: u32, y: u32) -> f32 {
    let fy = max((f32(y) + 0.5) * (f32(height) / f32(p.size.y)) - 0.5, 0.0);
    let y0 = min(u32(fy), height - 1u);
    let y1 = min(y0 + 1u, height - 1u);
    let ty = fy - f32(y0);
    let fx = max((f32(x) + 0.5) * (f32(width) / f32(p.size.x)) - 0.5, 0.0);
    let x0 = min(u32(fx), width - 1u);
    let x1 = min(x0 + 1u, width - 1u);
    let tx = fx - f32(x0);
    let top = planes[at + y0 * width + x0] * (1.0 - tx) + planes[at + y0 * width + x1] * tx;
    let bottom = planes[at + y1 * width + x0] * (1.0 - tx) + planes[at + y1 * width + x1] * tx;
    return top * (1.0 - ty) + bottom * ty;
}

var<workgroup> partial: array<f32, 256>;

fn reduce(lid: vec3<u32>) {
    for (var half = 128u; half > 0u; half /= 2u) {
        workgroupBarrier();
        if (lid.x < half) {
            partial[lid.x] += partial[lid.x + half];
        }
    }
    workgroupBarrier();
}

// The pivot: the mean of the base as it is laid over the frame — each
// workgroup's sum, then (`pivot_total`) theirs.
@compute @workgroup_size(256)
fn pivot_sum(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let i = index_of(wid, lid);
    var value = 0.0;
    if (i < p.size.x * p.size.y) {
        value = upsampled(step.at.x, step.shape.x, step.shape.y, i % p.size.x, i / p.size.x);
    }
    partial[lid.x] = value;
    reduce(lid);
    if (lid.x == 0u) {
        planes[step.at.y + wid.y * p.size.w + wid.x] = partial[0];
    }
}

@compute @workgroup_size(256)
fn pivot_total(@builtin(local_invocation_id) lid: vec3<u32>) {
    let count = step.shape.x;
    var sum = 0.0;
    for (var k = lid.x; k < count; k += 256u) {
        sum += planes[step.at.x + k];
    }
    partial[lid.x] = sum;
    reduce(lid);
    if (lid.x == 0u) {
        planes[step.at.y] = partial[0] / f32(p.size.x * p.size.y);
    }
}

// `local::tone_map`'s recombination: the base about the pivot, the detail
// (and Texture's band), the glow, as one gain on the pixel.
@compute @workgroup_size(256)
fn tone_apply(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let i = index_of(wid, lid);
    if (i >= p.size.x * p.size.y) {
        return;
    }
    let x = i % p.size.x;
    let y = i / p.size.x;
    let original = planes[i];
    let low = upsampled(step.at.x, step.shape.x, step.shape.y, x, y);
    let pivot = planes[step.more.z];
    let scales = step.numbers;
    var detail = (original - low) * scales.y;
    if (step.more.y != 0u) {
        let middle = planes[step.more.y + i];
        detail = (middle - low) * scales.y + (original - middle) * scales.w;
    }
    var mapped = pivot + (low - pivot) * scales.x + detail;
    if (step.more.x != 0u) {
        let spill = max(upsampled(step.more.x, step.shape.x, step.shape.y, x, y) - original, 0.0);
        mapped += spill * scales.z;
    }
    put_work(i, max(work_at(i) * exp2(mapped - original), vec3<f32>(0.0)));
}
