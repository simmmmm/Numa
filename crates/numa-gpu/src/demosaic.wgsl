// SPDX-License-Identifier: LGPL-2.1
//
// RENDER-010: the two demosaics, on the card.
//
// PPG is Chuan-kai Lin's Patterned Pixel Grouping and Markesteijn Frank
// Markesteijn's X-Trans interpolation, both as dcraw published them. What
// these passes reproduce is rawler's port of the two (Copyright 2022-2026
// Daniel Vogelbacher, LGPL-2.1), which is what Numa's processor path runs —
// step for step, with its borders and its tile-width quirk, so that the card
// and the processor tell the same picture apart only by rounding. A
// translation that close is rawler's code in another language, so it keeps
// rawler's licence rather than Numa's.
//
// Everything here reads the mosaic through `mos` and writes the cropped frame
// through `store_lin`, both in develop.wgsl.

// ---- PPG (Bayer) -------------------------------------------------------

fn ppg_border_rows(x: i32, y: i32) -> bool {
    return x < 3 || y < 3 || x >= i32(P.roi_w) - 3 || y >= i32(P.roi_h) - 3;
}

// PPG's border: a missing channel is the mean of that colour in the 3x3
// around, clipped to the frame; 0 when there is none.
fn ppg_border(x: i32, y: i32, c: u32) -> f32 {
    var sum = 0.0;
    var count = 0u;
    for (var yy = max(y - 1, 0); yy <= y + 1; yy++) {
        for (var xx = max(x - 1, 0); xx <= x + 1; xx++) {
            if (yy < i32(P.roi_h) && xx < i32(P.roi_w) && colour(xx, yy) == c) {
                sum += mos(xx, yy);
                count++;
            }
        }
    }
    if (count == 0u) {
        return 0.0;
    }
    return sum / f32(count);
}

fn green_at(x: i32, y: i32) -> f32 {
    return work[u32(y) * P.roi_w + u32(x)];
}

// Green everywhere, over the active area.
@compute @workgroup_size(16, 16)
fn ppg_green(@builtin(global_invocation_id) id: vec3<u32>) {
    let row = band.y0 + id.y;
    if (id.x >= P.roi_w || row >= band.y1) {
        return;
    }
    let x = i32(id.x);
    let y = i32(row);
    let c = colour(x, y);
    var g = 0.0;
    if (c == 1u) {
        g = mos(x, y);
    } else if (ppg_border_rows(x, y)) {
        g = ppg_border(x, y, 1u);
    } else {
        let v = mos(x, y);
        let n_1 = mos(x, y - 1);
        let n_2 = mos(x, y - 2);
        let e_1 = mos(x + 1, y);
        let e_2 = mos(x + 2, y);
        let s_1 = mos(x, y + 1);
        let s_2 = mos(x, y + 2);
        let w_1 = mos(x - 1, y);
        let w_2 = mos(x - 2, y);
        let n = abs(v - n_2) * 2.0 + abs(n_1 - s_1);
        let e = abs(v - e_2) * 2.0 + abs(w_1 - e_1);
        let w = abs(v - w_2) * 2.0 + abs(w_1 - e_1);
        let s = abs(v - s_2) * 2.0 + abs(n_1 - s_1);
        var least = n;
        if (e < least) { least = e; }
        if (w < least) { least = w; }
        if (s < least) { least = s; }
        // `a * 3.0` written as `a + a + a`: the same rounding, since `a + a`
        // is exact, and nothing a compiler may fuse with the addition after
        // it. A fused multiply-add rounds once where the processor rounds
        // twice, and one bit of green is enough to tip `ne < nw` below.
        if (least == n) {
            g = (n_1 + n_1 + n_1 + s_1 + v - n_2) / 4.0;
        } else if (least == e) {
            g = (e_1 + e_1 + e_1 + w_1 + v - e_2) / 4.0;
        } else if (least == w) {
            g = (w_1 + w_1 + w_1 + e_1 + v - w_2) / 4.0;
        } else {
            g = (s_1 + s_1 + s_1 + n_1 + v - s_2) / 4.0;
        }
    }
    work[row * P.roi_w + id.x] = g;
}

fn hue_transit(l1: f32, l2: f32, l3: f32, v1: f32, v3: f32) -> f32 {
    if ((l1 < l2 && l2 < l3) || (l1 > l2 && l2 > l3)) {
        return v1 + (v3 - v1) * (l2 - l1) / (l3 - l1);
    }
    return (v1 + v3) / 2.0 + (l2 * 2.0 - l1 - l3) / 4.0;
}

// Red and blue, over the crop, and out.
@compute @workgroup_size(16, 16)
fn ppg_rb(@builtin(global_invocation_id) id: vec3<u32>) {
    let row = band.y0 + id.y;
    if (id.x >= P.crop_w || row >= band.y1) {
        return;
    }
    let x = i32(id.x + P.crop_x);
    let y = i32(row + P.crop_y);
    let c = colour(x, y);
    var rgb = vec3<f32>(0.0);
    rgb[c] = mos(x, y);
    rgb.y = green_at(x, y);
    if (ppg_border_rows(x, y)) {
        if (c != 0u) { rgb.x = ppg_border(x, y, 0u); }
        if (c != 2u) { rgb.z = ppg_border(x, y, 2u); }
    } else if (c == 1u) {
        let h_ch = colour(x + 1, y);
        let v_ch = colour(x, y + 1);
        let g = rgb.y;
        rgb[h_ch] = hue_transit(green_at(x - 1, y), g, green_at(x + 1, y), mos(x - 1, y), mos(x + 1, y));
        rgb[v_ch] = hue_transit(green_at(x, y - 1), g, green_at(x, y + 1), mos(x, y - 1), mos(x, y + 1));
    } else {
        let y_ch = select(0u, 2u, c == 0u);
        let x_center = rgb[c];
        let g_center = rgb.y;
        let y_ne_1 = mos(x + 1, y - 1);
        let y_sw_1 = mos(x - 1, y + 1);
        let x_ne_2 = mos(x + 2, y - 2);
        let x_sw_2 = mos(x - 2, y + 2);
        let g_ne_1 = green_at(x + 1, y - 1);
        let g_sw_1 = green_at(x - 1, y + 1);
        let y_nw_1 = mos(x - 1, y - 1);
        let y_se_1 = mos(x + 1, y + 1);
        let x_nw_2 = mos(x - 2, y - 2);
        let x_se_2 = mos(x + 2, y + 2);
        let g_nw_1 = green_at(x - 1, y - 1);
        let g_se_1 = green_at(x + 1, y + 1);
        let ne = abs(y_ne_1 - y_sw_1) + abs(x_ne_2 - x_center) + abs(x_center - x_sw_2)
            + abs(g_ne_1 - g_center) + abs(g_center - g_sw_1);
        let nw = abs(y_nw_1 - y_se_1) + abs(x_nw_2 - x_center) + abs(x_center - x_se_2)
            + abs(g_nw_1 - g_center) + abs(g_center - g_se_1);
        if (ne < nw) {
            rgb[y_ch] = hue_transit(g_ne_1, g_center, g_sw_1, y_ne_1, y_sw_1);
        } else {
            rgb[y_ch] = hue_transit(g_nw_1, g_center, g_se_1, y_nw_1, y_se_1);
        }
    }
    store_lin(id.x, row, rgb);
}

// ---- Markesteijn, one pass (X-Trans) -----------------------------------
//
// Four directions, twelve planes of the band: plane (d * 3 + c).

fn mk_index(d: u32, c: u32, x: i32, ly: i32) -> u32 {
    let xx = u32(clamp(x, 0, i32(P.roi_w) - 1));
    let yy = u32(clamp(ly, 0, i32(band.rows) - 1));
    return (d * 3u + c) * P.band_n + yy * P.roi_w + xx;
}

fn rgbd(d: u32, c: u32, x: i32, ly: i32) -> f32 {
    return work[mk_index(d, c, x, ly)];
}

// The hexagon of neighbours around a site: (dy, dx) of one of eight slots.
fn hex(x: i32, y: i32, slot: u32) -> vec2<i32> {
    let at = T_HEX + (((u32(y % 3) * 3u + u32(x % 3)) * 8u + slot) * 2u);
    return vec2<i32>(bitcast<i32>(table[at]), bitcast<i32>(table[at + 1u]));
}

fn modulo3(v: i32) -> i32 {
    return ((v % 3) + 3) % 3;
}

// Green in four directions, and each site's own colour in all of them.
@compute @workgroup_size(16, 16)
fn mk_green(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= P.roi_w || id.y >= band.rows) {
        return;
    }
    let x = i32(id.x);
    let ly = i32(id.y);
    let y = ly + i32(band.w0);
    let f = colour(x, y);
    let v = mos(x, y);
    var green = array<f32, 4>(v, v, v, v);
    let w = i32(P.roi_w);
    let h = i32(P.roi_h);
    if (f != 1u && y >= 3 && y + 3 < h && x >= 3 && x + 3 < w) {
        var lo = 3.4028235e38;
        var hi = -3.4028235e38;
        for (var s = 0u; s < 6u; s++) {
            let o = hex(x, y, s);
            if (o.x == 0 && o.y == 0) {
                continue;
            }
            let m = mos(x + o.y, y + o.x);
            lo = min(lo, m);
            hi = max(hi, m);
        }
        let h0 = hex(x, y, 0u);
        let h1 = hex(x, y, 1u);
        let h2 = hex(x, y, 2u);
        let h3 = hex(x, y, 3u);
        let h4 = hex(x, y, 4u);
        let h5 = hex(x, y, 5u);
        let c0 = (174.0 * (mos(x + h1.y, y + h1.x) + mos(x + h0.y, y + h0.x))
            - 46.0 * (mos(x + 2 * h1.y, y + 2 * h1.x) + mos(x + 2 * h0.y, y + 2 * h0.x))) * (1.0 / 256.0);
        let c1 = (223.0 * mos(x + h3.y, y + h3.x) + 33.0 * mos(x + h2.y, y + h2.x)
            + 92.0 * (v - mos(x - h2.y, y - h2.x))) * (1.0 / 256.0);
        let c2 = (164.0 * mos(x + h4.y, y + h4.x) + 92.0 * mos(x - 2 * h4.y, y - 2 * h4.x)
            + 33.0 * (2.0 * v - mos(x + 3 * h4.y, y + 3 * h4.x) - mos(x - 3 * h4.y, y - 3 * h4.x))) * (1.0 / 256.0);
        let c3 = (164.0 * mos(x + h5.y, y + h5.x) + 92.0 * mos(x - 2 * h5.y, y - 2 * h5.x)
            + 33.0 * (2.0 * v - mos(x + 3 * h5.y, y + 3 * h5.x) - mos(x - 3 * h5.y, y - 3 * h5.x))) * (1.0 / 256.0);
        let flip = select(0u, 1u, modulo3(y - i32(P.sgrow)) == 0);
        var estimates = array<f32, 4>(c0, c1, c2, c3);
        for (var c = 0u; c < 4u; c++) {
            green[c ^ flip] = min(max(estimates[c], lo), hi);
        }
    }
    for (var d = 0u; d < 4u; d++) {
        var rgb = vec3<f32>(0.0);
        rgb[f] = v;
        rgb.y = green[d];
        work[mk_index(d, 0u, x, ly)] = rgb.x;
        work[mk_index(d, 1u, x, ly)] = rgb.y;
        work[mk_index(d, 2u, x, ly)] = rgb.z;
    }
}

// Red and blue at the solitary greens.
@compute @workgroup_size(16, 16)
fn mk_solitary(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = i32(id.x);
    let ly = i32(id.y);
    let y = ly + i32(band.w0);
    if (x < 2 || x + 2 >= i32(P.roi_w) || ly < 2 || ly + 2 >= i32(band.rows)) {
        return;
    }
    if (modulo3(y - i32(P.sgrow)) != 0 || modulo3(x - i32(P.sgcol)) != 0 || colour(x, y) != 1u) {
        return;
    }
    var h = colour(x + 1, y);
    if (h == 1u) {
        return;
    }
    var red = array<f32, 6>(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    var blue = array<f32, 6>(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    var diff = array<f32, 6>(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    var across = true;
    var buf = 0u;
    for (var d = 0u; d < 6u; d++) {
        for (var c = 0; c < 2; c++) {
            let s = 1 << u32(c);
            let o = select(vec2<i32>(0, s), vec2<i32>(s, 0), across);
            let centre = rgbd(buf, 1u, x, ly);
            let plus_g = rgbd(buf, 1u, x + o.x, ly + o.y);
            let minus_g = rgbd(buf, 1u, x - o.x, ly - o.y);
            let plus_h = rgbd(buf, h, x + o.x, ly + o.y);
            let minus_h = rgbd(buf, h, x - o.x, ly - o.y);
            let g = 2.0 * centre - plus_g - minus_g;
            let estimate = g + plus_h + minus_h;
            if (h == 0u) { red[d] = estimate; } else { blue[d] = estimate; }
            if (d > 1u) {
                let t = plus_g - minus_g - plus_h + minus_h;
                diff[d] += t * t + g * g;
            }
            h ^= 2u;
        }
        if (d > 1u && (d & 1u) != 0u && diff[d - 1u] < diff[d]) {
            red[d] = red[d - 1u];
            blue[d] = blue[d - 1u];
        }
        if (d < 2u || (d & 1u) != 0u) {
            work[mk_index(buf, 0u, x, ly)] = max(red[d] * 0.5, 0.0);
            work[mk_index(buf, 2u, x, ly)] = max(blue[d] * 0.5, 0.0);
            buf += 1u;
        }
        across = !across;
        h ^= 2u;
    }
}

// Red at blue sites and blue at red ones.
@compute @workgroup_size(16, 16)
fn mk_rb(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = i32(id.x);
    let ly = i32(id.y);
    let y = ly + i32(band.w0);
    if (x < 3 || x + 3 >= i32(P.roi_w) || ly < 3 || ly + 3 >= i32(band.rows)) {
        return;
    }
    let fc = colour(x, y);
    if (fc == 1u) {
        return;
    }
    let f = select(0u, 2u, fc == 0u);
    let vertical = modulo3(y - i32(P.sgrow)) != 0;
    let c = select(vec2<i32>(1, 0), vec2<i32>(0, 1), vertical);
    let h = select(vec2<i32>(0, 3), vec2<i32>(3, 0), vertical);
    // rawler writes `(d ^ c) & 1` with c the tile's width when vertical: its
    // parity is the tile's, and the last column of tiles can be odd.
    let c_odd = select(1u, select(0u, P.tw_odd, u32(x) >= P.tw_from), vertical);
    for (var d = 0u; d < 4u; d++) {
        let g0 = rgbd(d, 1u, x, ly);
        var dir = c;
        let forced = d > 1u || ((d & 1u) ^ c_odd) != 0u;
        if (!forced) {
            let along = abs(g0 - rgbd(d, 1u, x + c.x, ly + c.y)) + abs(g0 - rgbd(d, 1u, x - c.x, ly - c.y));
            let other = abs(g0 - rgbd(d, 1u, x + h.x, ly + h.y)) + abs(g0 - rgbd(d, 1u, x - h.x, ly - h.y));
            if (!(along < 2.0 * other)) {
                dir = h;
            }
        }
        let value = (rgbd(d, f, x + dir.x, ly + dir.y) + rgbd(d, f, x - dir.x, ly - dir.y) + 2.0 * g0
            - rgbd(d, 1u, x + dir.x, ly + dir.y) - rgbd(d, 1u, x - dir.x, ly - dir.y)) * 0.5;
        work[mk_index(d, f, x, ly)] = max(value, 0.0);
    }
}

// Red and blue for the 2x2 blocks of green.
@compute @workgroup_size(16, 16)
fn mk_blocks(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = i32(id.x);
    let ly = i32(id.y);
    let y = ly + i32(band.w0);
    if (x < 2 || x + 2 >= i32(P.roi_w) || ly < 2 || ly + 2 >= i32(band.rows)) {
        return;
    }
    if (modulo3(y - i32(P.sgrow)) == 0 || modulo3(x - i32(P.sgcol)) == 0) {
        return;
    }
    for (var d = 0u; d < 4u; d++) {
        let a = hex(x, y, d * 2u);
        let b = hex(x, y, d * 2u + 1u);
        let g0 = rgbd(d, 1u, x, ly);
        let ga = rgbd(d, 1u, x + a.y, ly + a.x);
        let gb = rgbd(d, 1u, x + b.y, ly + b.x);
        if (a.x + b.x != 0 || a.y + b.y != 0) {
            let g = 3.0 * g0 - 2.0 * ga - gb;
            for (var ch = 0u; ch < 3u; ch += 2u) {
                let value = (g + 2.0 * rgbd(d, ch, x + a.y, ly + a.x) + rgbd(d, ch, x + b.y, ly + b.x)) / 3.0;
                work[mk_index(d, ch, x, ly)] = max(value, 0.0);
            }
        } else {
            let g = 2.0 * g0 - ga - gb;
            for (var ch = 0u; ch < 3u; ch += 2u) {
                let value = (g + rgbd(d, ch, x + a.y, ly + a.x) + rgbd(d, ch, x + b.y, ly + b.x)) * 0.5;
                work[mk_index(d, ch, x, ly)] = max(value, 0.0);
            }
        }
    }
}

// rawler's cube root: a bit trick and two Newton steps.
fn fast_cbrt(x: f32) -> f32 {
    if (x == 0.0) {
        return 0.0;
    }
    let a = abs(x);
    var r = bitcast<f32>(bitcast<u32>(a) / 3u + 0x2a510554u);
    r -= (r - a / (r * r)) * (1.0 / 3.0);
    r -= (r - a / (r * r)) * (1.0 / 3.0);
    return select(r, -r, x < 0.0);
}

fn lab_f(t: f32) -> f32 {
    if (t > 216.0 / 24389.0) {
        return fast_cbrt(t);
    }
    return ((24389.0 / 27.0) * t + 16.0) / 116.0;
}

// sRGB (D65) to CIELab, as rawler measures homogeneity in.
fn lab(d: u32, x: i32, ly: i32) -> vec3<f32> {
    let r = rgbd(d, 0u, x, ly);
    let g = rgbd(d, 1u, x, ly);
    let b = rgbd(d, 2u, x, ly);
    let cx = 0.4124564 * r + 0.3575761 * g + 0.1804375 * b;
    let cy = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let cz = 0.0193339 * r + 0.1191920 * g + 0.9503041 * b;
    let fx = lab_f(cx / 0.95047);
    let fy = lab_f(cy / 1.0);
    let fz = lab_f(cz / 1.08883);
    return vec3<f32>(116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz));
}

// How much each direction's picture changes along that direction, in Lab.
@compute @workgroup_size(16, 16)
fn mk_drv(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= P.roi_w || id.y >= band.rows) {
        return;
    }
    let x = i32(id.x);
    let ly = i32(id.y);
    var steps = array<vec2<i32>, 4>(vec2(1, 0), vec2(0, 1), vec2(1, 1), vec2(-1, 1));
    for (var d = 0u; d < 4u; d++) {
        let o = steps[d];
        let centre = lab(d, x, ly);
        let plus = lab(d, x + o.x, ly + o.y);
        let minus = lab(d, x - o.x, ly - o.y);
        let g = 2.0 * centre.x - plus.x - minus.x;
        let a = 2.0 * centre.y - plus.y - minus.y + g * (500.0 / 232.0);
        let b = 2.0 * centre.z - plus.z - minus.z - g * (500.0 / 580.0);
        drv[d * P.band_n + id.y * P.roi_w + id.x] = g * g + a * a + b * b;
    }
}

fn drv_at(d: u32, x: i32, ly: i32) -> f32 {
    let xx = u32(clamp(x, 0, i32(P.roi_w) - 1));
    let yy = u32(clamp(ly, 0, i32(band.rows) - 1));
    return drv[d * P.band_n + yy * P.roi_w + xx];
}

// Each direction's votes: how many of the 3x3 are within eight times the
// smoothest direction here. Four counts in one word.
@compute @workgroup_size(16, 16)
fn mk_homo(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= P.roi_w || id.y >= band.rows) {
        return;
    }
    let x = i32(id.x);
    let ly = i32(id.y);
    var tr = 3.4028235e38;
    for (var d = 0u; d < 4u; d++) {
        tr = min(tr, drv_at(d, x, ly));
    }
    tr *= 8.0;
    var packed = 0u;
    for (var d = 0u; d < 4u; d++) {
        var votes = 0u;
        for (var v = -1; v <= 1; v++) {
            for (var h = -1; h <= 1; h++) {
                if (drv_at(d, x + h, ly + v) <= tr) {
                    votes++;
                }
            }
        }
        packed |= votes << (8u * d);
    }
    homo[id.y * P.roi_w + id.x] = packed;
}

fn homo_at(x: i32, ly: i32) -> u32 {
    let xx = u32(clamp(x, 0, i32(P.roi_w) - 1));
    let yy = u32(clamp(ly, 0, i32(band.rows) - 1));
    return homo[yy * P.roi_w + xx];
}

// Markesteijn's border: every channel the mean of that colour in the 3x3.
fn mk_border(x: i32, y: i32) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    var count = vec3<f32>(0.0);
    for (var yy = max(y - 1, 0); yy <= min(y + 1, i32(P.roi_h) - 1); yy++) {
        for (var xx = max(x - 1, 0); xx <= min(x + 1, i32(P.roi_w) - 1); xx++) {
            let c = colour(xx, yy);
            sum[c] += mos(xx, yy);
            count[c] += 1.0;
        }
    }
    return select(vec3<f32>(0.0), sum / max(count, vec3<f32>(1.0)), count > vec3<f32>(0.0));
}

// The average of the most homogeneous directions over a 5x5, and out.
@compute @workgroup_size(16, 16)
fn mk_final(@builtin(global_invocation_id) id: vec3<u32>) {
    let y = band.y0 + id.y;
    if (id.x >= P.crop_w || y >= band.y1 || y < P.crop_y || y >= P.crop_y + P.crop_h) {
        return;
    }
    let x = i32(id.x + P.crop_x);
    let iy = i32(y);
    var rgb = vec3<f32>(0.0);
    if (x < 12 || x >= i32(P.roi_w) - 12 || iy < 12 || iy >= i32(P.roi_h) - 12) {
        rgb = mk_border(x, iy);
    } else {
        let ly = iy - i32(band.w0);
        var hm = vec4<u32>(0u);
        for (var v = -2; v <= 2; v++) {
            for (var h = -2; h <= 2; h++) {
                let packed = homo_at(x + h, ly + v);
                hm += vec4<u32>(packed & 0xffu, (packed >> 8u) & 0xffu, (packed >> 16u) & 0xffu, packed >> 24u);
            }
        }
        let most = max(max(hm.x, hm.y), max(hm.z, hm.w));
        let threshold = most - (most >> 3u);
        var sum = vec3<f32>(0.0);
        var count = 0u;
        for (var d = 0u; d < 4u; d++) {
            if (hm[d] >= threshold) {
                sum += vec3<f32>(rgbd(d, 0u, x, ly), rgbd(d, 1u, x, ly), rgbd(d, 2u, x, ly));
                count++;
            }
        }
        rgb = sum * (1.0 / f32(count));
    }
    store_lin(id.x, y - P.crop_y, rgb);
}
