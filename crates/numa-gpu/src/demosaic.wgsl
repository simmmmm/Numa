// SPDX-License-Identifier: LGPL-2.1
// Derived from rawler, Copyright 2022-2026 Daniel Vogelbacher, LGPL-2.1

fn mk_index(d: u32, c: u32, x: i32, ly: i32) -> u32 {
    let xx = u32(clamp(x, 0, i32(P.roi_w) - 1));
    let yy = u32(clamp(ly, 0, i32(band.rows) - 1));
    return (d * 3u + c) * P.band_n + yy * P.roi_w + xx;
}

fn rgbd(d: u32, c: u32, x: i32, ly: i32) -> f32 {
    return work[mk_index(d, c, x, ly)];
}

fn hex(x: i32, y: i32, slot: u32) -> vec2<i32> {
    let at = T_HEX + (((u32(y % 3) * 3u + u32(x % 3)) * 8u + slot) * 2u);
    return vec2<i32>(bitcast<i32>(table[at]), bitcast<i32>(table[at + 1u]));
}

fn modulo3(v: i32) -> i32 {
    return ((v % 3) + 3) % 3;
}

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
