
// RENDER-027: the Bayer demosaic on the card, RCD — Luis Sanz Rodríguez's
// Ratio Corrected Demosaicing (v2.3) — as `numa-io`'s `raw::rcd` does it: the
// same steps, operation for operation, and the same rule at the active area's
// edge, so the card and the processor differ only by how the card rounds.
// Numa's own, written from the algorithm's steps.
//
// The sites arrive white-balanced as shot (`scale`, through the levels'
// table). `rcd_direction` leaves vertical against horizontal in `drv`,
// `rcd_green` the green in `work`, `rcd_colour` blue at red and red at blue
// in `homo`; `rcd_out` finishes red and blue at green and takes the balance
// out again on the way to `lin`.

const RCD_EPS: f32 = 1e-5;
const RCD_EPSSQ: f32 = 1e-10;

// How far a site of the active area is from its nearest edge.
fn rcd_edge(x: i32, y: i32) -> i32 {
    return min(min(x, y), min(i32(P.roi_w) - 1 - x, i32(P.roi_h) - 1 - y));
}

// PPG's border, which RCD keeps near the edge: the mean of colour `c` in the
// 3x3 around, within the area; 0 when there is none.
fn rcd_border(x: i32, y: i32, c: u32) -> f32 {
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

// The square of a colour-difference high pass through (x, y) along (dx, dy).
fn rcd_high(x: i32, y: i32, dx: i32, dy: i32) -> f32 {
    let v = (mos(x - 3 * dx, y - 3 * dy) - mos(x - dx, y - dy) - mos(x + dx, y + dy) + mos(x + 3 * dx, y + 3 * dy))
        - 3.0 * (mos(x - 2 * dx, y - 2 * dy) + mos(x + 2 * dx, y + 2 * dy)) + 6.0 * mos(x, y);
    return v * v;
}

fn rcd_index(x: i32, y: i32) -> u32 {
    return u32(y) * P.roi_w + u32(x);
}

fn rcd_g(x: i32, y: i32) -> f32 {
    return work[rcd_index(x, y)];
}

fn rcd_dir(x: i32, y: i32) -> f32 {
    return drv[rcd_index(x, y)];
}

fn rcd_other(x: i32, y: i32) -> f32 {
    return bitcast<f32>(homo[rcd_index(x, y)]);
}

// A direction, or the mean of the four diagonal neighbours' where those are
// surer of theirs.
fn rcd_refine(central: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    let around = 0.25 * (a + b + c + d);
    return select(central, around, abs(0.5 - central) < abs(0.5 - around));
}

// A low pass of all three colours, at red and blue.
fn rcd_lpf(x: i32, y: i32) -> f32 {
    return mos(x, y) + 0.5 * (mos(x, y - 1) + mos(x, y + 1) + mos(x - 1, y) + mos(x + 1, y))
        + 0.25 * (mos(x - 1, y - 1) + mos(x + 1, y - 1) + mos(x - 1, y + 1) + mos(x + 1, y + 1));
}

// A neighbour's value, corrected by the ratio of the low pass here and there.
fn rcd_ratio(value: f32, here: f32, there: f32) -> f32 {
    return value * (1.0 + (here - there) / (RCD_EPS + here + there));
}

// Diagonal against anti-diagonal at a red or blue site, as the direction is.
fn rcd_pq(x: i32, y: i32) -> f32 {
    if (rcd_edge(x, y) < 4) {
        return 0.5;
    }
    let p = max(rcd_high(x - 1, y - 1, 1, 1) + rcd_high(x, y, 1, 1) + rcd_high(x + 1, y + 1, 1, 1), RCD_EPSSQ);
    let q = max(rcd_high(x + 1, y - 1, -1, 1) + rcd_high(x, y, -1, 1) + rcd_high(x - 1, y + 1, -1, 1), RCD_EPSSQ);
    return p / (p + q);
}

// 1. Vertical against horizontal, over the active area.
@compute @workgroup_size(16, 16)
fn rcd_direction(@builtin(global_invocation_id) id: vec3<u32>) {
    let row = band.y0 + id.y;
    if (id.x >= P.roi_w || row >= band.y1) {
        return;
    }
    let x = i32(id.x);
    let y = i32(row);
    var dir = 0.5;
    if (rcd_edge(x, y) >= 4) {
        let v = max(rcd_high(x, y - 1, 0, 1) + rcd_high(x, y, 0, 1) + rcd_high(x, y + 1, 0, 1), RCD_EPSSQ);
        let h = max(rcd_high(x - 1, y, 1, 0) + rcd_high(x, y, 1, 0) + rcd_high(x + 1, y, 1, 0), RCD_EPSSQ);
        dir = v / (v + h);
    }
    drv[rcd_index(x, y)] = dir;
}

// 2–3. Green at red and blue, each neighbour corrected by the low pass.
@compute @workgroup_size(16, 16)
fn rcd_green(@builtin(global_invocation_id) id: vec3<u32>) {
    let row = band.y0 + id.y;
    if (id.x >= P.roi_w || row >= band.y1) {
        return;
    }
    let x = i32(id.x);
    let y = i32(row);
    var g = 0.0;
    if (colour(x, y) == 1u) {
        g = mos(x, y);
    } else if (rcd_edge(x, y) < 4) {
        g = rcd_border(x, y, 1u);
    } else {
        let disc = rcd_refine(rcd_dir(x, y), rcd_dir(x - 1, y - 1), rcd_dir(x + 1, y - 1), rcd_dir(x - 1, y + 1), rcd_dir(x + 1, y + 1));
        let v = mos(x, y);
        let n_grad = RCD_EPS + abs(mos(x, y - 1) - mos(x, y + 1)) + abs(v - mos(x, y - 2)) + abs(mos(x, y - 1) - mos(x, y - 3)) + abs(mos(x, y - 2) - mos(x, y - 4));
        let s_grad = RCD_EPS + abs(mos(x, y + 1) - mos(x, y - 1)) + abs(v - mos(x, y + 2)) + abs(mos(x, y + 1) - mos(x, y + 3)) + abs(mos(x, y + 2) - mos(x, y + 4));
        let w_grad = RCD_EPS + abs(mos(x - 1, y) - mos(x + 1, y)) + abs(v - mos(x - 2, y)) + abs(mos(x - 1, y) - mos(x - 3, y)) + abs(mos(x - 2, y) - mos(x - 4, y));
        let e_grad = RCD_EPS + abs(mos(x + 1, y) - mos(x - 1, y)) + abs(v - mos(x + 2, y)) + abs(mos(x + 1, y) - mos(x + 3, y)) + abs(mos(x + 2, y) - mos(x + 4, y));
        let l = rcd_lpf(x, y);
        let n_est = rcd_ratio(mos(x, y - 1), l, rcd_lpf(x, y - 2));
        let s_est = rcd_ratio(mos(x, y + 1), l, rcd_lpf(x, y + 2));
        let w_est = rcd_ratio(mos(x - 1, y), l, rcd_lpf(x - 2, y));
        let e_est = rcd_ratio(mos(x + 1, y), l, rcd_lpf(x + 2, y));
        let v_est = (s_grad * n_est + n_grad * s_est) / (n_grad + s_grad);
        let h_est = (e_grad * w_est + w_grad * e_est) / (e_grad + w_grad);
        g = max(disc * h_est + (1.0 - disc) * v_est, 0.0);
    }
    work[rcd_index(x, y)] = g;
}

// 4. Blue at red and red at blue, along the diagonals.
@compute @workgroup_size(16, 16)
fn rcd_colour(@builtin(global_invocation_id) id: vec3<u32>) {
    let row = band.y0 + id.y;
    if (id.x >= P.roi_w || row >= band.y1) {
        return;
    }
    let x = i32(id.x);
    let y = i32(row);
    let c = colour(x, y);
    var o = 0.0;
    if (c == 1u) {
        o = 0.0;
    } else if (rcd_edge(x, y) < 3) {
        o = rcd_border(x, y, 2u - c);
    } else {
        let disc = rcd_refine(rcd_pq(x, y), rcd_pq(x - 1, y - 1), rcd_pq(x + 1, y - 1), rcd_pq(x - 1, y + 1), rcd_pq(x + 1, y + 1));
        let g = rcd_g(x, y);
        let nw_grad = RCD_EPS + abs(mos(x - 1, y - 1) - mos(x + 1, y + 1)) + abs(mos(x - 1, y - 1) - mos(x - 3, y - 3)) + abs(g - rcd_g(x - 2, y - 2));
        let ne_grad = RCD_EPS + abs(mos(x + 1, y - 1) - mos(x - 1, y + 1)) + abs(mos(x + 1, y - 1) - mos(x + 3, y - 3)) + abs(g - rcd_g(x + 2, y - 2));
        let sw_grad = RCD_EPS + abs(mos(x - 1, y + 1) - mos(x + 1, y - 1)) + abs(mos(x - 1, y + 1) - mos(x - 3, y + 3)) + abs(g - rcd_g(x - 2, y + 2));
        let se_grad = RCD_EPS + abs(mos(x + 1, y + 1) - mos(x - 1, y - 1)) + abs(mos(x + 1, y + 1) - mos(x + 3, y + 3)) + abs(g - rcd_g(x + 2, y + 2));
        let nw = mos(x - 1, y - 1) - rcd_g(x - 1, y - 1);
        let ne = mos(x + 1, y - 1) - rcd_g(x + 1, y - 1);
        let sw = mos(x - 1, y + 1) - rcd_g(x - 1, y + 1);
        let se = mos(x + 1, y + 1) - rcd_g(x + 1, y + 1);
        let p_est = (nw_grad * se + se_grad * nw) / (nw_grad + se_grad);
        let q_est = (ne_grad * sw + sw_grad * ne) / (ne_grad + sw_grad);
        o = max(g + (1.0 - disc) * p_est + disc * q_est, 0.0);
    }
    homo[rcd_index(x, y)] = bitcast<u32>(o);
}

// Colour `c` at a red or blue site.
fn rcd_known(x: i32, y: i32, c: u32) -> f32 {
    if (colour(x, y) == c) {
        return mos(x, y);
    }
    return rcd_other(x, y);
}

// 5. Red and blue at green, along the axes; the crop out, on the camera's
// own scale.
@compute @workgroup_size(16, 16)
fn rcd_out(@builtin(global_invocation_id) id: vec3<u32>) {
    let row = band.y0 + id.y;
    if (id.x >= P.crop_w || row >= band.y1) {
        return;
    }
    let x = i32(id.x + P.crop_x);
    let y = i32(row + P.crop_y);
    let c = colour(x, y);
    var rgb = vec3<f32>(0.0);
    let g = rcd_g(x, y);
    rgb.y = g;
    if (c == 1u) {
        for (var k = 0u; k < 2u; k++) {
            let cc = k * 2u;
            if (rcd_edge(x, y) < 3) {
                rgb[cc] = rcd_border(x, y, cc);
            } else {
                let disc = rcd_refine(rcd_dir(x, y), rcd_dir(x - 1, y - 1), rcd_dir(x + 1, y - 1), rcd_dir(x - 1, y + 1), rcd_dir(x + 1, y + 1));
                let n_grad = RCD_EPS + abs(g - rcd_g(x, y - 2)) + abs(rcd_known(x, y - 1, cc) - rcd_known(x, y + 1, cc)) + abs(rcd_known(x, y - 1, cc) - rcd_known(x, y - 3, cc));
                let s_grad = RCD_EPS + abs(g - rcd_g(x, y + 2)) + abs(rcd_known(x, y + 1, cc) - rcd_known(x, y - 1, cc)) + abs(rcd_known(x, y + 1, cc) - rcd_known(x, y + 3, cc));
                let w_grad = RCD_EPS + abs(g - rcd_g(x - 2, y)) + abs(rcd_known(x - 1, y, cc) - rcd_known(x + 1, y, cc)) + abs(rcd_known(x - 1, y, cc) - rcd_known(x - 3, y, cc));
                let e_grad = RCD_EPS + abs(g - rcd_g(x + 2, y)) + abs(rcd_known(x + 1, y, cc) - rcd_known(x - 1, y, cc)) + abs(rcd_known(x + 1, y, cc) - rcd_known(x + 3, y, cc));
                let n = rcd_known(x, y - 1, cc) - rcd_g(x, y - 1);
                let so = rcd_known(x, y + 1, cc) - rcd_g(x, y + 1);
                let we = rcd_known(x - 1, y, cc) - rcd_g(x - 1, y);
                let e = rcd_known(x + 1, y, cc) - rcd_g(x + 1, y);
                let v_est = (n_grad * so + s_grad * n) / (n_grad + s_grad);
                let h_est = (e_grad * we + w_grad * e) / (e_grad + w_grad);
                rgb[cc] = max(g + (1.0 - disc) * v_est + disc * h_est, 0.0);
            }
        }
    } else {
        rgb[c] = mos(x, y);
        rgb[2u - c] = rcd_other(x, y);
    }
    // `balance` is red and blue as shot, then one over each.
    rgb.x = rgb.x * P.balance[2];
    rgb.z = rgb.z * P.balance[3];
    store_lin(id.x, row, rgb);
}
