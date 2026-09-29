use super::*;
use crate::Model;
use ndarray::{Array, ArrayD};

fn value_info(name: &str, dims: &[i64]) -> Vec<u8> {
    typed_value_info(name, dims, FLOAT)
}

fn typed_value_info(name: &str, dims: &[i64], data_type: i64) -> Vec<u8> {
    let mut shape = Vec::new();
    for dim in dims {
        let mut entry = Vec::new();
        put_int(&mut entry, 1, *dim);
        put_bytes(&mut shape, 1, &entry);
    }
    let mut tensor_type = Vec::new();
    put_int(&mut tensor_type, 1, data_type);
    put_bytes(&mut tensor_type, 2, &shape);
    let mut of_type = Vec::new();
    put_bytes(&mut of_type, 1, &tensor_type);
    let mut out = Vec::new();
    put_bytes(&mut out, 1, name.as_bytes());
    put_bytes(&mut out, 2, &of_type);
    out
}

fn constant(name: &str, dims: &[i64], data_type: i64, raw: &[u8]) -> Vec<u8> {
    new_node("Constant", name, &[], &[name], &[tensor_attribute("value", &tensor("", dims, data_type, raw))])
}

fn int64_raw(values: &[i64]) -> Vec<u8> {
    values.iter().flat_map(|value| value.to_le_bytes()).collect()
}

fn miniature() -> Vec<u8> {
    let outputs: Vec<String> = (0..10).map(|index| format!("o{index}")).collect();
    let outputs: Vec<&str> = outputs.iter().map(String::as_str).collect();
    let nodes = [
        constant("sizes", &[10], INT64, &int64_raw(&[1; 10])),
        new_node("Split", "split", &["x", "sizes"], &outputs, &[int_attribute("axis", 1)]),
        new_node("Sum", "sum", &["o0", "o1", "o2"], &["y"], &[]),
        constant("half", &[1], FLOAT, &0.5f32.to_le_bytes()),
        new_node("Add", "a", &["v", "half"], &["a"], &[]),
        new_node("Floor", "f", &["a"], &["f"], &[]),
        new_node("Cast", "c", &["f"], &["c"], &[int_attribute("to", INT64)]),
        constant("one", &[1], INT64, &int64_raw(&[1])),
        new_node("Add", "p", &["c", "one"], &["p"], &[]),
        constant("lo", &[], INT64, &int64_raw(&[0])),
        constant("hi", &[], INT64, &int64_raw(&[9])),
        new_node("Clip", "q", &["p", "lo", "hi"], &["q"], &[]),
        constant("shape", &[2], INT64, &int64_raw(&[10, 1])),
        new_node("Reshape", "r", &["q", "shape"], &["r"], &[]),
        new_node("GatherND", "z", &["v", "r"], &["z"], &[]),
    ];
    let mut graph = Vec::new();
    for node in &nodes {
        put_bytes(&mut graph, 1, node);
    }
    put_bytes(&mut graph, 2, b"miniature");
    put_bytes(&mut graph, 11, &value_info("x", &[1, 10]));
    put_bytes(&mut graph, 11, &value_info("v", &[10]));
    put_bytes(&mut graph, 12, &value_info("y", &[1, 1]));
    put_bytes(&mut graph, 12, &value_info("z", &[10]));
    let mut opset = Vec::new();
    put_bytes(&mut opset, 1, b"");
    put_int(&mut opset, 2, 17);
    let mut model = Vec::new();
    put_int(&mut model, 1, 8);
    put_bytes(&mut model, 7, &graph);
    put_bytes(&mut model, 8, &opset);
    model
}

fn census(model: &[u8]) -> (HashMap<String, usize>, usize) {
    let top = fields(model).unwrap();
    let graph = fields(top.iter().find(|field| field.number == 7).unwrap().bytes().unwrap()).unwrap();
    let mut counts = HashMap::new();
    let mut widest = 0;
    for field in graph.iter().filter(|field| field.number == 1) {
        let node = Node::parse(field.bytes().unwrap()).unwrap();
        *counts.entry(node.op.to_string()).or_insert(0) += 1;
        if node.op == "Split" {
            widest = widest.max(node.outputs.len());
        }
    }
    (counts, widest)
}

#[test]
fn the_rewrite_changes_the_graph_and_not_the_answer() {
    let dir = std::env::temp_dir().join("numa-rewrite-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (before, after) = (dir.join("before.onnx"), dir.join("after.onnx"));
    std::fs::write(&before, miniature()).unwrap();
    std::fs::write(&after, miniature()).unwrap();

    assert!(!rewritten(&after).unwrap());
    assert!(prepare(&after).unwrap());
    assert!(rewritten(&after).unwrap());
    let once = std::fs::read(&after).unwrap();
    assert!(!prepare(&after).unwrap(), "a rewritten model is left alone");
    assert_eq!(std::fs::read(&after).unwrap(), once);

    assert_eq!(for_webgpu(&miniature()).unwrap().concat(), once);
    assert!(!dir.join("after.rewriting").exists());

    let (counts, widest) = census(&once);
    assert_eq!(widest, 0);
    assert_eq!(counts.get("Sum"), None);
    assert_eq!(counts["Slice"], 10);
    assert_eq!(counts["Add"], 2 + 2);
    assert_eq!(counts["Cast"], 2, "the Cast after Floor, and one before the GatherND");

    let x: ArrayD<f32> = Array::from_shape_vec(vec![1, 10], (1..=10).map(|v| v as f32).collect()).unwrap();
    let v: ArrayD<f32> =
        Array::from_shape_vec(vec![10], vec![0.2, 1.4, 2.6, 3.5, -1.0, 8.9, 9.7, 4.49, 0.0, 7.1]).unwrap();
    let run = |path: &Path| Model::load(path).unwrap().run(vec![x.clone().into(), v.clone().into()]).unwrap();
    let (was, is) = (run(&before), run(&after));
    assert_eq!(was[0].iter().copied().collect::<Vec<_>>(), [6.0]);
    assert_eq!(was, is);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn attention_by_head_is_the_same_attention() {
    let nodes = [
        constant("grid", &[5], INT64, &int64_raw(&[12, 2, 2, 2, 2])),
        constant("flat", &[3], INT64, &int64_raw(&[12, 4, 4])),
        new_node("MatMul", "qk", &["q", "k"], &["qk"], &[]),
        new_node("Reshape", "r5", &["qk", "grid"], &["r5"], &[]),
        new_node("Add", "a2", &["r5", "rows"], &["a2"], &[]),
        new_node("Add", "a3", &["a2", "columns"], &["a3"], &[]),
        new_node("Reshape", "r6", &["a3", "flat"], &["r6"], &[]),
        new_node("Softmax", "sm", &["r6"], &["sm"], &[int_attribute("axis", -1)]),
        new_node("Cast", "c", &["sm"], &["c"], &[int_attribute("to", FLOAT)]),
        new_node("MatMul", "pv", &["c", "v"], &["y"], &[]),
    ];
    let mut graph = Vec::new();
    for node in &nodes {
        put_bytes(&mut graph, 1, node);
    }
    put_bytes(&mut graph, 2, b"attention");
    let inputs: [(&str, &[i64]); 5] =
        [("q", &[12, 4, 3]), ("k", &[12, 3, 4]), ("rows", &[12, 2, 2, 2, 1]), ("columns", &[12, 2, 2, 1, 2]), ("v", &[12, 4, 3])];
    for (name, dims) in inputs {
        put_bytes(&mut graph, 11, &value_info(name, dims));
    }
    put_bytes(&mut graph, 12, &value_info("y", &[12, 4, 3]));
    let mut opset = Vec::new();
    put_bytes(&mut opset, 1, b"");
    put_int(&mut opset, 2, 13);
    let mut model = Vec::new();
    put_int(&mut model, 1, 8);
    put_bytes(&mut model, 7, &graph);
    put_bytes(&mut model, 8, &opset);

    let rewritten = for_webgpu(&model).unwrap().concat();
    let (counts, _) = census(&rewritten);
    assert_eq!((counts["Softmax"], counts["MatMul"], counts["Split"]), (12, 24, 5));

    let dir = std::env::temp_dir().join("numa-rewrite-attention");
    std::fs::create_dir_all(&dir).unwrap();
    let (before, after) = (dir.join("before.onnx"), dir.join("after.onnx"));
    std::fs::write(&before, &model).unwrap();
    std::fs::write(&after, &rewritten).unwrap();
    let mut seed = 1u32;
    let values: Vec<ArrayD<f32>> = inputs
        .iter()
        .map(|(_, dims)| {
            let shape: Vec<usize> = dims.iter().map(|d| *d as usize).collect();
            Array::from_shape_fn(shape, |_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 8) as f32 / (1u32 << 24) as f32 * 4.0 - 2.0
            })
            .into_dyn()
        })
        .collect();
    let run = |path: &Path| Model::load(path).unwrap().run(values.iter().cloned().map(Into::into).collect()).unwrap();
    assert_eq!(run(&before), run(&after));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[cfg(not(target_vendor = "apple"))]
fn a_deformable_convolution_by_row_is_the_same_convolution() {
    let (c, k, h, w, out) = (2i64, 3i64, 4i64, 4i64, 3i64);
    let taps = k * k * h * w;
    let shape = |name: &str, dims: &[i64]| constant(name, &[dims.len() as i64], INT64, &int64_raw(dims));
    let weight: Vec<u8> = (0..out * c * k * k).flat_map(|i| ((i % 7) as f32 * 0.25 - 0.7).to_le_bytes()).collect();
    let mut nodes = vec![
        shape("by_tap", &[1, 1, c, k * k, h, w]),
        shape("grid", &[1, c, k, k, h, w]),
        shape("wide", &[1, c, h * k, w * k]),
        constant("weight", &[out, c, k, k], FLOAT, &weight),
    ];
    for corner in 0..4 {
        let at = |what: &str| format!("{what}{corner}");
        nodes.push(new_node("GatherND", &at("g"), &["image", &at("i")], &[&at("g")], &[int_attribute("batch_dims", 2)]));
        nodes.push(new_node("Transpose", &at("t"), &[&at("g")], &[&at("t")], &[ints_attribute("perm", &[0, 1, 3, 2])]));
        nodes.push(new_node("Reshape", &at("r"), &[&at("t"), "by_tap"], &[&at("r")], &[]));
        nodes.push(new_node("Mul", &at("m"), &[&at("w"), &at("r")], &[&at("m")], &[]));
    }
    nodes.extend([
        new_node("Sum", "sum", &["m0", "m1", "m2", "m3"], &["sum"], &[]),
        new_node("Mul", "modulated", &["sum", "mod"], &["modulated"], &[]),
        new_node("Reshape", "cells", &["modulated", "grid"], &["cells"], &[]),
        new_node("Transpose", "spread", &["cells"], &["spread"], &[ints_attribute("perm", &[0, 1, 4, 2, 5, 3])]),
        new_node("Reshape", "flat", &["spread", "wide"], &["flat"], &[]),
        new_node("Conv", "conv", &["flat", "weight"], &["y"], &[ints_attribute("kernel_shape", &[k, k]), ints_attribute("strides", &[k, k])]),
    ]);
    let mut graph = Vec::new();
    for node in &nodes {
        put_bytes(&mut graph, 1, node);
    }
    put_bytes(&mut graph, 2, b"deformable");
    put_bytes(&mut graph, 11, &value_info("image", &[1, 1, h + 2, w + 2, c]));
    for corner in 0..4 {
        put_bytes(&mut graph, 11, &typed_value_info(&format!("i{corner}"), &[1, 1, taps, 2], INT64));
        put_bytes(&mut graph, 11, &value_info(&format!("w{corner}"), &[1, 1, 1, k * k, h, w]));
    }
    put_bytes(&mut graph, 11, &value_info("mod", &[1, 1, 1, k * k, h, w]));
    put_bytes(&mut graph, 12, &value_info("y", &[1, out, h, w]));
    let mut opset = Vec::new();
    put_bytes(&mut opset, 1, b"");
    put_int(&mut opset, 2, 17);
    let mut model = Vec::new();
    put_int(&mut model, 1, 8);
    put_bytes(&mut model, 7, &graph);
    put_bytes(&mut model, 8, &opset);

    let rewritten = for_webgpu(&model).unwrap().concat();
    let (counts, _) = census(&rewritten);
    assert_eq!((counts.get("Sum"), counts.get("Conv")), (None, None));
    assert_eq!((counts["GatherND"], counts["MatMul"]), (4 * k as usize, k as usize));

    let dir = std::env::temp_dir().join("numa-rewrite-deformable-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (before, after) = (dir.join("before.onnx"), dir.join("after.onnx"));
    std::fs::write(&before, &model).unwrap();
    std::fs::write(&after, &rewritten).unwrap();
    let noise = |n: i64, seed: i64| (0..n).map(|i| ((i * 7919 + seed * 104729) % 97) as f32 / 97.0 - 0.5).collect::<Vec<_>>();
    let inputs = || {
        let mut inputs: Vec<crate::Input> =
            vec![Array::from_shape_vec(vec![1, 1, 6, 6, 2], noise((h + 2) * (w + 2) * c, 1)).unwrap().into()];
        for corner in 0..4 {
            let indices: Vec<i64> = (0..taps * 2).map(|i| (i * 31 + corner * 17 + 7) % (h + 2)).collect();
            inputs.push(Array::from_shape_vec(vec![1, 1, taps as usize, 2], indices).unwrap().into());
            inputs.push(Array::from_shape_vec(vec![1, 1, 1, 9, 4, 4], noise(k * k * h * w, corner + 2)).unwrap().into());
        }
        inputs.push(Array::from_shape_vec(vec![1, 1, 1, 9, 4, 4], noise(k * k * h * w, 9)).unwrap().into());
        inputs
    };
    let run = |path: &Path| Model::load(path).unwrap().run(inputs()).unwrap();
    let (was, is) = (run(&before), run(&after));
    assert_eq!(was[0].shape(), [1, 3, 4, 4]);
    let apart = was[0].iter().zip(is[0].iter()).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(apart < 1e-5, "{apart}");
    assert!(was[0].iter().any(|v| v.abs() > 0.1), "a miniature that answers zero proves nothing");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn float16_holds_the_whole_numbers_it_can() {
    assert_eq!(half(0), Some(0));
    assert_eq!(half(1), Some(0x3c00));
    assert_eq!(half(-2), Some(0xc000));
    assert_eq!(half(33), Some(0x5020));
    assert_eq!(half(1024), Some(0x6400));
    assert_eq!(half(2048), Some(0x6800));
    assert_eq!(half(2049), None);
    assert_eq!(half(65504), Some(0x7bff));
    assert_eq!(half(65536), None);
}

#[test]
#[ignore]
fn birefnet_rewritten() {
    let (Ok(model), Ok(reference), Ok(photos), Ok(work)) =
        (std::env::var("BIREFNET"), std::env::var("REFERENCE"), std::env::var("PHOTOS"), std::env::var("WORK"))
    else {
        println!("set BIREFNET, REFERENCE, PHOTOS and WORK");
        return;
    };
    let on_card = std::env::var("GPU_MODELS").ok();
    if let Some(dir) = &on_card {
        crate::enable_gpu(Path::new(dir), &Path::new(&work).join("numa-rewrite-guard"));
    }

    let path = Path::new(&work).join("birefnet.onnx");
    std::fs::copy(&model, &path).unwrap();
    let started = std::time::Instant::now();
    assert!(prepare(&path).unwrap());
    println!("rewritten in {:.1?}", started.elapsed());

    let (counts, widest) = census(&std::fs::read(&path).unwrap());
    assert!(widest <= 8, "a Split of {widest}");
    assert_eq!(counts.get("Sum"), None);

    let plan = Model::load(&path).unwrap();
    assert_eq!(plan.on_card(), on_card.is_some());
    let count: usize = std::env::var("COUNT").ok().and_then(|n| n.parse().ok()).unwrap_or(3);
    const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
    const STD: [f32; 3] = [0.229, 0.224, 0.225];
    let mut ious = Vec::new();
    for n in 1..=count {
        let square = image::open(format!("{photos}/q{n}.png")).unwrap().to_rgb8();
        assert_eq!(square.dimensions(), (1024, 1024));
        let mut input = ndarray::Array4::<f32>::zeros((1, 3, 1024, 1024));
        for (x, y, pixel) in square.enumerate_pixels() {
            for c in 0..3 {
                input[[0, c, y as usize, x as usize]] = (pixel[c] as f32 / 255.0 - MEAN[c]) / STD[c];
            }
        }
        let started = std::time::Instant::now();
        let out = plan.run(vec![input.into_dyn().into()]).unwrap();
        let took = started.elapsed();
        let want = image::open(format!("{reference}/q{n}.png")).unwrap().to_luma8();
        let (mut both, mut either) = (0usize, 0usize);
        for (value, reference) in out[0].iter().zip(want.as_raw()) {
            let (a, b) = (*value > 0.0, *reference > 127);
            both += (a && b) as usize;
            either += (a || b) as usize;
        }
        let iou = if either == 0 { 1.0 } else { both as f64 / either as f64 };
        println!("q{n}: IoU {iou:.4} in {took:.0?}");
        ious.push(iou);
    }
    ious.sort_by(f64::total_cmp);

    let judged = if on_card.is_some() { ious[ious.len() / 2] } else { ious[0] };
    assert!(judged > 0.99, "IoU {ious:?}");
}
