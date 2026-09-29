// RENDER-010: the passes after the decoder, on the card. This file is what
// every pass shares, and Numa's own passes: the lens (OPTICS-001/002), the
// false-colour step (FT-022), the exposure samples (RENDER-008), the turn
// (TOOL-003) and the downscale to the proxy (RENDER-001). Each mirrors its
// processor twin in `numa-core::lens`, `numa-io::raw` or `numa-core::image`
// operation for operation, so the two differ only by how the card rounds.
// The demosaics are in demosaic.wgsl, appended to this one.

struct Params {
    raw_w: u32, raw_h: u32, scaled_w: u32, scaled_h: u32,
    roi_x: u32, roi_y: u32, roi_w: u32, roi_h: u32,
    crop_x: u32, crop_y: u32, crop_w: u32, crop_h: u32,
    levels: array<u32, 4>,
    spare4: array<u32, 4>,
    baseline: f32, fc_black: f32, factor: f32, flags: u32,
    centre_x: f32, centre_y: f32, half_diag: f32, fit: f32,
    n_radii: u32, sgrow: u32, sgcol: u32, band_n: u32,
    out_w: u32, out_h: u32, stride: u32, n_samples: u32,
    t_box: u32, tw_from: u32, tw_odd: u32, spare: u32,
}

// Every dispatch covers a tile of rows, y0..y1, so that no one submission
// keeps the card from the desktop for long. Markesteijn's bands also work
// rows w0..w0+rows around their tile.
struct Band { y0: u32, y1: u32, w0: u32, rows: u32 }

// Bounds on the loops the tables drive, so a bad table cannot keep the card
// busy: the host refuses a lens or a proxy that needs more.
const MAX_RADII: u32 = 64u;
const MAX_BOX: u32 = 64u;

@group(0) @binding(0) var<storage, read> P: Params;
@group(0) @binding(1) var<storage, read> band: Band;
// The sensor's counts, two to a word.
@group(0) @binding(2) var<storage, read> mosaic: array<u32>;
// The CFA, Markesteijn's hexagon, the lens's tables, the proxy's boxes, and
// what each count of the sensor becomes.
@group(0) @binding(3) var<storage, read> table: array<u32>;
@group(0) @binding(4) var<storage, read_write> work: array<f32>;
@group(0) @binding(5) var<storage, read_write> drv: array<f32>;
@group(0) @binding(6) var<storage, read_write> homo: array<u32>;
// The demosaiced frame, cropped, lifted and with its falloff put back: a
// buffer per colour, so no one buffer is a whole frame (an iPhone's largest
// is smaller than a 45 MP frame's three planes, RENDER-018).
@group(0) @binding(7) var<storage, read_write> lin_r: array<Plane>;
@group(0) @binding(8) var<storage, read_write> lin_g: array<Plane>;
@group(0) @binding(9) var<storage, read_write> lin_b: array<Plane>;
// The same after the lens's geometry.
@group(0) @binding(10) var<storage, read_write> bent_r: array<Plane>;
@group(0) @binding(11) var<storage, read_write> bent_g: array<Plane>;
@group(0) @binding(12) var<storage, read_write> bent_b: array<Plane>;
// What goes back to the processor: interleaved RGB, the right way up, a
// piece of rows at a time (from the band's `w0`).
@group(0) @binding(13) var<storage, read_write> outp: array<f32>;
@group(0) @binding(14) var<storage, read_write> samp: array<f32>;
// The active area's counts as the demosaics read them, scaled once.
@group(0) @binding(15) var<storage, read_write> sites: array<f32>;

// What a plane holds: f32, or f16 where a frame would not otherwise fit the
// card (an iPhone; the host swaps this line and enables f16).
alias Plane = f32;

fn lin_get(c: u32, i: u32) -> f32 {
    switch c {
        case 0u: { return f32(lin_r[i]); }
        case 1u: { return f32(lin_g[i]); }
        default: { return f32(lin_b[i]); }
    }
}

fn lin_set(c: u32, i: u32, v: f32) {
    switch c {
        case 0u: { lin_r[i] = Plane(v); }
        case 1u: { lin_g[i] = Plane(v); }
        default: { lin_b[i] = Plane(v); }
    }
}

fn bent_get(c: u32, i: u32) -> f32 {
    switch c {
        case 0u: { return f32(bent_r[i]); }
        case 1u: { return f32(bent_g[i]); }
        default: { return f32(bent_b[i]); }
    }
}

fn bent_set(c: u32, i: u32, v: f32) {
    switch c {
        case 0u: { bent_r[i] = Plane(v); }
        case 1u: { bent_g[i] = Plane(v); }
        default: { bent_b[i] = Plane(v); }
    }
}

const VIGNETTING: u32 = 1u;
const GEOMETRY: u32 = 2u;
const TRANSPOSE: u32 = 8u;
const FLIP_X: u32 = 16u;
const FLIP_Y: u32 = 32u;
// Where the finished frame is: `bent` rather than `lin`.
const FINISHED_BENT: u32 = 64u;

const T_CFA: u32 = 0u;
const T_HEX: u32 = 36u;
const T_LENS: u32 = 180u;

// The colour of a site of the active area: 0 red, 1 green, 2 blue.
fn colour(x: i32, y: i32) -> u32 {
    return table[T_CFA + u32(y % 6) * 6u + u32(x % 6)];
}

// A site of the active area, as rawler's `apply_scaling` leaves it: black
// subtracted per 2x2 position, clipped at zero, divided by white minus black.
// Its pass works in pairs of rows and pairs of columns, so an odd last row or
// column of the sensor is left as it was read — and so it is here.
//
// Looked up in a table the processor made rather than worked out: PPG
// decides between directions on exact ties, so one bit of difference is a
// different green, and neither the card's division nor its `fma` (which the
// driver may split into a multiply and an add) is correctly rounded.
// Markstein's correction, exact on the processor for every count, was not
// on the card.
fn scaled(x: u32, y: u32) -> f32 {
    let sx = x + P.roi_x;
    let sy = y + P.roi_y;
    let i = sy * P.raw_w + sx;
    let word = mosaic[i >> 1u];
    let v = select(word & 0xffffu, word >> 16u, (i & 1u) == 1u);
    if (sx >= P.scaled_w || sy >= P.scaled_h) {
        return f32(v);
    }
    return bitcast<f32>(table[P.levels[(sy & 1u) * 2u + (sx & 1u)] + v]);
}

// Every site of the active area, scaled, once — the demosaics read each
// one many times, and a count then its table entry is two loads in a row.
@compute @workgroup_size(16, 16)
fn scale(@builtin(global_invocation_id) id: vec3<u32>) {
    let y = band.y0 + id.y;
    if (id.x >= P.roi_w || y >= band.y1) {
        return;
    }
    sites[y * P.roi_w + id.x] = scaled(id.x, y);
}

fn mos(x: i32, y: i32) -> f32 {
    return sites[u32(y) * P.roi_w + u32(x)];
}

fn tf(i: u32) -> f32 {
    return bitcast<f32>(table[i]);
}

// The lens's tables follow its radii: transmission, distortion, red, blue,
// each n values and then the value `LensProfile::at` returns past its end.
fn t_table(k: u32) -> u32 {
    return T_LENS + P.n_radii + k * (P.n_radii + 1u);
}

// `LensProfile::at`.
fn lens_at(t: u32, r: f32) -> f32 {
    let n = P.n_radii;
    if (r <= tf(T_LENS)) {
        return tf(t);
    }
    for (var w = 1u; w < min(n, MAX_RADII); w++) {
        if (r <= tf(T_LENS + w)) {
            let r0 = tf(T_LENS + w - 1u);
            let r1 = tf(T_LENS + w);
            let fraction = clamp((r - r0) / max(r1 - r0, 1e-6), 0.0, 1.0);
            return tf(t + w - 1u) + (tf(t + w) - tf(t + w - 1u)) * fraction;
        }
    }
    return tf(t + n);
}

// `LensProfile::gain`.
fn gain(r: f32) -> f32 {
    let n = P.n_radii;
    if (n == 0u || r <= 0.0) {
        return 1.0;
    }
    let t = t_table(0u);
    if (r <= tf(T_LENS)) {
        let fraction = clamp(r / tf(T_LENS), 0.0, 1.0);
        let transmission = 1.0 + (tf(t) - 1.0) * fraction;
        return 1.0 / max(transmission, 0.05);
    }
    for (var w = 1u; w < min(n, MAX_RADII); w++) {
        if (r <= tf(T_LENS + w)) {
            let r0 = tf(T_LENS + w - 1u);
            let r1 = tf(T_LENS + w);
            let t0 = tf(t + w - 1u);
            let t1 = tf(t + w);
            let fraction = clamp((r - r0) / max(r1 - r0, 1e-6), 0.0, 1.0);
            return 1.0 / max(t0 + (t1 - t0) * fraction, 0.05);
        }
    }
    return 1.0 / max(tf(t + n), 0.05);
}

// A demosaiced pixel of the cropped frame on its way out of the demosaic:
// the baseline lift, then `correct_vignetting`'s gain.
fn store_lin(cx: u32, cy: u32, rgb: vec3<f32>) {
    var g = 1.0;
    if ((P.flags & VIGNETTING) != 0u) {
        let dx = f32(cx) + 0.5 - P.centre_x;
        let dy = f32(cy) + 0.5 - P.centre_y;
        g = gain(sqrt(dx * dx + dy * dy) / P.half_diag);
    }
    let i = cy * P.crop_w + cx;
    lin_set(0u, i, rgb.x * P.baseline * g);
    lin_set(1u, i, rgb.y * P.baseline * g);
    lin_set(2u, i, rgb.z * P.baseline * g);
}

fn lin_at(c: u32, x: i32, y: i32) -> f32 {
    let xx = u32(clamp(x, 0, i32(P.crop_w) - 1));
    let yy = u32(clamp(y, 0, i32(P.crop_h) - 1));
    return lin_get(c, yy * P.crop_w + xx);
}

// `lens::sample`: bilinear, the edge held.
fn sample(c: u32, x: f32, y: f32) -> f32 {
    let x0 = floor(x);
    let y0 = floor(y);
    let fx = x - x0;
    let fy = y - y0;
    let ix = i32(x0);
    let iy = i32(y0);
    let top = lin_at(c, ix, iy) + (lin_at(c, ix + 1, iy) - lin_at(c, ix, iy)) * fx;
    let bottom = lin_at(c, ix, iy + 1) + (lin_at(c, ix + 1, iy + 1) - lin_at(c, ix, iy + 1)) * fx;
    return top + (bottom - top) * fy;
}

// OPTICS-002: `correct_geometry`, one destination pixel.
@compute @workgroup_size(16, 16)
fn geometry(@builtin(global_invocation_id) id: vec3<u32>) {
    let y = band.y0 + id.y;
    if (id.x >= P.crop_w || y >= band.y1) {
        return;
    }
    let dx = (f32(id.x) + 0.5 - P.centre_x) * P.fit;
    let dy = (f32(y) + 0.5 - P.centre_y) * P.fit;
    let radius = sqrt(dx * dx + dy * dy) / P.half_diag;
    let distortion = 1.0 + lens_at(t_table(1u), radius) / 100.0;
    let scales = vec3<f32>(
        distortion * (1.0 + lens_at(t_table(2u), radius)),
        distortion,
        distortion * (1.0 + lens_at(t_table(3u), radius)),
    );
    let i = y * P.crop_w + id.x;
    for (var c = 0u; c < 3u; c++) {
        let sx = P.centre_x + dx * scales[c] - 0.5;
        let sy = P.centre_y + dy * scales[c] - 0.5;
        bent_set(c, i, sample(c, sx, sy));
    }
}

// The finished frame — after the lens and the false colour — wherever it is.
fn frame(c: u32, i: u32) -> f32 {
    if ((P.flags & FINISHED_BENT) != 0u) {
        return bent_get(c, i);
    }
    return lin_get(c, i);
}

fn pixel(x: u32, y: u32) -> vec3<f32> {
    let i = y * P.crop_w + x;
    return vec3<f32>(frame(0u, i), frame(1u, i), frame(2u, i));
}

// What the false-colour step reads: the frame after the lens, in the other
// buffer from the one it writes.
fn unfinished(c: u32, x: i32, y: i32) -> f32 {
    let xx = u32(clamp(x, 0, i32(P.crop_w) - 1));
    let yy = u32(clamp(y, 0, i32(P.crop_h) - 1));
    let i = yy * P.crop_w + xx;
    if ((P.flags & FINISHED_BENT) != 0u) {
        return lin_get(c, i);
    }
    return bent_get(c, i);
}

fn sort2(w: ptr<function, array<f32, 9>>, a: u32, b: u32) {
    let lo = min((*w)[a], (*w)[b]);
    let hi = max((*w)[a], (*w)[b]);
    (*w)[a] = lo;
    (*w)[b] = hi;
}

// `median_of_nine`: Smith's network, the same nineteen exchanges.
fn median9(v: array<f32, 9>) -> f32 {
    var w = v;
    sort2(&w, 1u, 2u); sort2(&w, 4u, 5u); sort2(&w, 7u, 8u);
    sort2(&w, 0u, 1u); sort2(&w, 3u, 4u); sort2(&w, 6u, 7u);
    sort2(&w, 1u, 2u); sort2(&w, 4u, 5u); sort2(&w, 7u, 8u);
    sort2(&w, 0u, 3u); sort2(&w, 5u, 8u); sort2(&w, 4u, 7u);
    sort2(&w, 3u, 6u); sort2(&w, 1u, 4u); sort2(&w, 2u, 5u);
    sort2(&w, 4u, 7u); sort2(&w, 4u, 2u); sort2(&w, 6u, 4u);
    sort2(&w, 4u, 2u);
    return w[4];
}

// A 16x16 tile's red and blue ratios to green, with the ring around it.
var<workgroup> ratio_r: array<f32, 324>;
var<workgroup> ratio_b: array<f32, 324>;

fn tile_median(blue: bool, x: u32, y: u32) -> f32 {
    var w = array<f32, 9>();
    for (var k = 0u; k < 9u; k++) {
        let at = (y + k / 3u - 1u) * 18u + x + k % 3u - 1u;
        w[k] = select(ratio_r[at], ratio_b[at], blue);
    }
    return median9(w);
}

// FT-022: `suppress_false_colour` — green as it was, red and blue rebuilt as
// green times the median of their 3x3 ratios to it, the border replicated.
// Each ratio is worked out once, into the tile, rather than nine times.
@compute @workgroup_size(16, 16)
fn false_colour(@builtin(local_invocation_id) l: vec3<u32>, @builtin(workgroup_id) g: vec3<u32>) {
    let left = i32(g.x * 16u) - 1;
    let top = i32(band.y0 + g.y * 16u) - 1;
    for (var k = l.y * 16u + l.x; k < 324u; k += 256u) {
        let x = left + i32(k % 18u);
        let y = top + i32(k / 18u);
        let green = unfinished(1u, x, y);
        ratio_r[k] = select(1.0, unfinished(0u, x, y) / green, green > P.fc_black);
        ratio_b[k] = select(1.0, unfinished(2u, x, y) / green, green > P.fc_black);
    }
    workgroupBarrier();
    let x = g.x * 16u + l.x;
    let y = band.y0 + g.y * 16u + l.y;
    if (x >= P.crop_w || y >= band.y1) {
        return;
    }
    let green = unfinished(1u, i32(x), i32(y));
    var red = unfinished(0u, i32(x), i32(y));
    var blue = unfinished(2u, i32(x), i32(y));
    if (green > P.fc_black) {
        red = green * tile_median(false, l.x + 1u, l.y + 1u);
        blue = green * tile_median(true, l.x + 1u, l.y + 1u);
    }
    let i = y * P.crop_w + x;
    if ((P.flags & FINISHED_BENT) != 0u) {
        bent_set(0u, i, red);
        bent_set(1u, i, green);
        bent_set(2u, i, blue);
    } else {
        lin_set(0u, i, red);
        lin_set(1u, i, green);
        lin_set(2u, i, blue);
    }
}

// RENDER-008: every `stride`th pixel, which is all the exposure match reads.
@compute @workgroup_size(256)
fn samples(@builtin(global_invocation_id) id: vec3<u32>) {
    let k = band.y0 + id.x;
    if (k >= band.y1 || k >= P.n_samples) {
        return;
    }
    let at = k * P.stride;
    let v = pixel(at % P.crop_w, at / P.crop_w);
    samp[k * 3u] = v.x;
    samp[k * 3u + 1u] = v.y;
    samp[k * 3u + 2u] = v.z;
}

// TOOL-003: where a pixel of the upright frame comes from — `turned_data`.
fn unturned(ox: u32, oy: u32) -> vec2<u32> {
    var f = vec2<u32>(ox, oy);
    if ((P.flags & TRANSPOSE) != 0u) {
        f = vec2<u32>(oy, ox);
    }
    if ((P.flags & FLIP_X) != 0u) {
        f.x = P.crop_w - 1u - f.x;
    }
    if ((P.flags & FLIP_Y) != 0u) {
        f.y = P.crop_h - 1u - f.y;
    }
    return f;
}

// The whole frame, upright, for 1:1 and export; `outp` holds the rows from
// `band.w0` on, as does the proxy's.
@compute @workgroup_size(16, 16)
fn full(@builtin(global_invocation_id) id: vec3<u32>) {
    let y = band.y0 + id.y;
    if (id.x >= P.out_w || y >= band.y1) {
        return;
    }
    let src = unturned(id.x, y);
    let v = pixel(src.x, src.y) * P.factor;
    let o = ((y - band.w0) * P.out_w + id.x) * 3u;
    outp[o] = v.x;
    outp[o + 1u] = v.y;
    outp[o + 2u] = v.z;
}

// RENDER-001: `LinearImage::downscaled` of the upright frame — a box average
// over the boxes the processor worked out, so every proxy pixel covers the
// same source pixels it would have there.
@compute @workgroup_size(16, 16)
fn proxy(@builtin(global_invocation_id) id: vec3<u32>) {
    let y = band.y0 + id.y;
    if (id.x >= P.out_w || y >= band.y1) {
        return;
    }
    let x0 = table[P.t_box + id.x];
    let x1 = min(table[P.t_box + P.out_w + id.x], x0 + MAX_BOX);
    let y0 = table[P.t_box + 2u * P.out_w + y];
    let y1 = min(table[P.t_box + 2u * P.out_w + P.out_h + y], y0 + MAX_BOX);
    var sum = vec3<f32>(0.0);
    for (var v = y0; v < y1; v++) {
        for (var u = x0; u < x1; u++) {
            let src = unturned(u, v);
            sum += pixel(src.x, src.y);
        }
    }
    let mean = sum / f32((x1 - x0) * (y1 - y0)) * P.factor;
    let o = ((y - band.w0) * P.out_w + id.x) * 3u;
    outp[o] = mean.x;
    outp[o + 1u] = mean.y;
    outp[o + 2u] = mean.z;
}
