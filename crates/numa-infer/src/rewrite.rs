use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

const MARK: (&str, &str) = ("numa", "webgpu 1");

const FLOAT: i64 = 1;
const INT64: i64 = 7;
const FLOAT16: i64 = 10;

pub fn prepare(path: &Path) -> std::io::Result<bool> {
    if rewritten(path)? {
        return Ok(false);
    }
    let model = std::fs::read(path)?;
    let pieces = for_webgpu(&model).map_err(std::io::Error::other)?;
    let partial = path.with_extension("rewriting");
    let mut out = std::io::BufWriter::new(std::fs::File::create(&partial)?);
    for piece in &pieces {
        out.write_all(piece)?;
    }
    out.into_inner().map_err(|err| err.into_error())?.sync_all()?;
    std::fs::rename(&partial, path)?;
    Ok(true)
}

pub fn rewritten(path: &Path) -> std::io::Result<bool> {
    let mark = mark();
    let mut file = std::fs::File::open(path)?;
    if file.metadata()?.len() < mark.len() as u64 {
        return Ok(false);
    }
    file.seek(SeekFrom::End(-(mark.len() as i64)))?;
    let mut tail = vec![0; mark.len()];
    file.read_exact(&mut tail)?;
    Ok(tail == mark)
}

fn mark() -> Vec<u8> {
    let mut entry = Vec::new();
    put_bytes(&mut entry, 1, MARK.0.as_bytes());
    put_bytes(&mut entry, 2, MARK.1.as_bytes());
    let mut field = Vec::new();
    put_bytes(&mut field, 14, &entry);
    field
}

pub fn for_webgpu(model: &[u8]) -> Result<Vec<Cow<'_, [u8]>>, String> {
    let top = fields(model)?;
    let graph = top.iter().find(|field| field.number == 7).ok_or("no graph")?.bytes()?;
    let mut graph = rewrite_graph(graph)?;
    let length: usize = graph.iter().map(|piece| piece.len()).sum();

    let mut out = Vec::new();
    for field in &top {
        if field.number == 7 {
            let mut head = Vec::new();
            put_varint(&mut head, 7 << 3 | 2);
            put_varint(&mut head, length as u64);
            out.push(Cow::Owned(head));
            out.append(&mut graph);
        } else {
            out.push(Cow::Borrowed(field.raw));
        }
    }
    out.push(Cow::Owned(mark()));
    Ok(out)
}

fn rewrite_graph(graph: &[u8]) -> Result<Vec<Cow<'_, [u8]>>, String> {
    let graph_fields = fields(graph)?;
    let nodes: Vec<Node> =
        graph_fields.iter().filter(|field| field.number == 1).map(|field| Node::parse(field.bytes()?)).collect::<Result<_, _>>()?;
    let producer: HashMap<&str, usize> =
        nodes.iter().enumerate().flat_map(|(at, node)| node.outputs.iter().map(move |name| (*name, at))).collect();
    let initializer_type: HashMap<&str, i64> = graph_fields
        .iter()
        .filter(|field| field.number == 5)
        .filter_map(|field| {
            let tensor = fields(field.bytes().ok()?).ok()?;
            Some((string(&tensor, 8)?, int(&tensor, 2)? as i64))
        })
        .collect();
    let op = |name: &str| producer.get(name).map(|at| nodes[*at].op);
    let constant_type = |name: &str| match producer.get(name) {
        Some(at) if nodes[*at].op == "Constant" => nodes[*at].tensor("value").and_then(data_type),
        Some(_) => None,
        None => initializer_type.get(name).copied(),
    };

    let mut floatish: HashMap<&str, i64> = HashMap::new();
    let mut changed: HashMap<usize, Vec<u8>> = HashMap::new();
    for (at, node) in nodes.iter().enumerate() {
        if node.op == "Cast" && node.int("to") == Some(INT64) && op(node.inputs[0]) == Some("Floor") {
            let floor = &nodes[producer[node.inputs[0]]];
            let Some(add) = producer.get(floor.inputs[0]).map(|at| &nodes[*at]) else { continue };
            let Some(to) = add.inputs.iter().find_map(|name| constant_type(name)) else { continue };
            changed.insert(at, node.with_attribute("to", &int_attribute("to", to)));
            floatish.insert(node.outputs[0], to);
            continue;
        }
        let from = match node.op {
            "Slice" | "Reshape" | "Clip" => floatish.get(node.inputs[0]),
            "Add" | "Concat" => node.inputs.iter().find_map(|name| floatish.get(name)),
            _ => None,
        };
        let Some(&to) = from else { continue };
        for (index, name) in node.inputs.iter().enumerate() {

            if floatish.contains_key(name) || (matches!(node.op, "Slice" | "Reshape") && index > 0) {
                continue;
            }
            let Some(&constant) = producer.get(name) else { continue };
            let Some(value) = nodes[constant].tensor("value") else { continue };
            if nodes[constant].op != "Constant" || data_type(value) != Some(INT64) {
                continue;
            }
            let retyped = tensor_attribute("value", &retype(value, to)?);
            changed.insert(constant, nodes[constant].with_attribute("value", &retyped));
        }
        for name in &node.outputs {
            floatish.insert(name, to);
        }
    }

    let attentions = attentions(&graph_fields, &nodes, &producer);
    let folded: std::collections::HashSet<usize> =
        attentions.values().flat_map(|chain| chain[..chain.len() - 1].to_vec()).collect();

    let mut out: Vec<Cow<[u8]>> = Vec::new();
    let mut initializers: Vec<Vec<u8>> = Vec::new();
    if !attentions.is_empty() {
        for (name, value) in [("one", 1), ("end", i64::MAX), ("zero", 0), ("any", -1)] {
            initializers.push(tensor(&format!("numa_heads_{name}"), &[1], INT64, &value.to_le_bytes()));
        }
    }
    let mut nodes_seen = 0;
    for field in &graph_fields {
        match field.number {
            1 => {
                let at = nodes_seen;
                nodes_seen += 1;
                let node = &nodes[at];
                if folded.contains(&at) {

                } else if let Some(chain) = attentions.get(&at) {
                    for bytes in by_head(&nodes, chain) {
                        emit(&mut out, 1, bytes);
                    }
                } else if node.op == "Split" && node.outputs.len() > 8 {
                    let axis = node.int("axis").unwrap_or(0);
                    let sizes = node
                        .inputs
                        .get(1)
                        .and_then(|name| producer.get(name))
                        .and_then(|at| nodes[*at].tensor("value"))
                        .map(int64s)
                        .ok_or_else(|| format!("{}: a Split without its sizes", node.name))?;
                    let mut start = 0;
                    for (index, (output, size)) in node.outputs.iter().zip(sizes).enumerate() {
                        let bounds = ["st", "en", "ax"].map(|what| format!("{}_s{index}_{what}", node.name));
                        for (name, value) in bounds.iter().zip([start, start + size, axis]) {
                            initializers.push(tensor(name, &[1], INT64, &value.to_le_bytes()));
                        }
                        let inputs = [node.inputs[0], &bounds[0], &bounds[1], &bounds[2]];
                        emit(&mut out, 1, new_node("Slice", &format!("{}_slice{index}", node.name), &inputs, &[*output], &[]));
                        start += size;
                    }
                } else if node.op == "Sum" && node.inputs.len() > 1 {
                    let mut sum = node.inputs[0].to_string();
                    for (index, name) in node.inputs[1..].iter().enumerate() {
                        let last = index == node.inputs.len() - 2;
                        let to = if last { node.outputs[0].to_string() } else { format!("{}_acc{index}", node.name) };
                        emit(&mut out, 1, new_node("Add", &format!("{}_add{index}", node.name), &[&sum, *name], &[&to], &[]));
                        sum = to;
                    }
                } else if node.op == "GatherND" && floatish.contains_key(node.inputs[1]) {
                    let index = format!("{}_i64", node.inputs[1]);
                    emit(&mut out, 1, new_node("Cast", &index, &[node.inputs[1]], &[&index], &[int_attribute("to", INT64)]));
                    emit(&mut out, 1, node.with_input(1, &index));
                } else if let Some(bytes) = changed.remove(&at) {
                    emit(&mut out, 1, bytes);
                } else {
                    out.push(Cow::Borrowed(field.raw));
                }
            }

            13 if string(&fields(field.bytes()?)?, 1).is_some_and(|name| floatish.contains_key(name)) => {}
            _ => out.push(Cow::Borrowed(field.raw)),
        }
    }
    for initializer in initializers {
        emit(&mut out, 5, initializer);
    }
    Ok(out)
}

const HEADS: usize = 12;

fn attentions<'a>(graph: &[Field<'a>], nodes: &[Node<'a>], producer: &HashMap<&'a str, usize>) -> HashMap<usize, Vec<usize>> {
    let mut reads: HashMap<&str, usize> = HashMap::new();
    for name in nodes.iter().flat_map(|node| node.inputs.iter().copied()) {
        *reads.entry(name).or_default() += 1;
    }
    for field in graph.iter().filter(|field| field.number == 12) {
        if let Some(name) = field.bytes().ok().and_then(|bytes| fields(bytes).ok()).and_then(|output| string(&output, 1)) {
            *reads.entry(name).or_default() += 2;
        }
    }
    let mut found = HashMap::new();
    'chains: for (at, node) in nodes.iter().enumerate().filter(|(_, node)| node.op == "MatMul") {
        let mut chain = vec![at];
        let mut from = node.inputs[0];
        while let Some(&cast) = producer.get(from).filter(|&&p| nodes[p].op == "Cast" && nodes[p].int("to") == Some(FLOAT)) {
            chain.push(cast);
            from = nodes[cast].inputs[0];
        }
        for op in ["Softmax", "Reshape", "Add", "Add", "Reshape", "MatMul"] {
            match producer.get(from) {
                Some(&p) if nodes[p].op == op && nodes[p].inputs.len() == if op == "Softmax" { 1 } else { 2 } => {
                    chain.push(p);
                    from = nodes[p].inputs[0];
                }
                _ => continue 'chains,
            }
        }
        chain.reverse();
        if chain[..chain.len() - 1].iter().all(|&p| nodes[p].outputs.iter().all(|name| reads.get(name) == Some(&1))) {
            found.insert(at, chain);
        }
    }
    found
}

fn by_head(nodes: &[Node], chain: &[usize]) -> Vec<Vec<u8>> {
    let node = |index: usize| &nodes[chain[index]];
    let (query_key, rows, columns, softmax, value) = (node(0), node(2), node(3), node(5), &nodes[chain[chain.len() - 1]]);
    let base = value.name;
    let mut out = Vec::new();
    let mut shapes = Vec::new();
    for reshape in [node(1), node(4)] {
        let tail = format!("{}_tail", reshape.name);
        let shape = format!("{}_by_head", reshape.name);
        out.push(new_node("Slice", &tail, &[reshape.inputs[1], "numa_heads_one", "numa_heads_end", "numa_heads_zero"], &[&tail], &[]));
        out.push(new_node("Concat", &shape, &["numa_heads_any", &tail], &[&shape], &[int_attribute("axis", 0)]));
        shapes.push(shape);
    }
    let whole = [query_key.inputs[0], query_key.inputs[1], rows.inputs[1], columns.inputs[1], value.inputs[1]];
    let pieces: Vec<Vec<String>> =
        (0..whole.len()).map(|k| (0..HEADS).map(|head| format!("{base}_in{k}_head{head}")).collect()).collect();
    for (k, name) in whole.iter().enumerate() {
        let outputs: Vec<&str> = pieces[k].iter().map(String::as_str).collect();
        out.push(new_node("Split", &format!("{base}_split{k}"), &[name], &outputs, &[int_attribute("axis", 0)]));
    }
    let axis = softmax.int("axis").unwrap_or(-1);
    let mut heads = Vec::new();
    for head in 0..HEADS {
        let piece = |k: usize| pieces[k][head].as_str();
        let [scores, grid, by_row, by_column, flat, weights, answer] =
            ["scores", "grid", "rows", "columns", "flat", "weights", "answer"].map(|what| format!("{base}_head{head}_{what}"));
        out.push(new_node("MatMul", &scores, &[piece(0), piece(1)], &[&scores], &[]));
        out.push(new_node("Reshape", &grid, &[&scores, &shapes[0]], &[&grid], &[]));
        out.push(new_node("Add", &by_row, &[&grid, piece(2)], &[&by_row], &[]));
        out.push(new_node("Add", &by_column, &[&by_row, piece(3)], &[&by_column], &[]));
        out.push(new_node("Reshape", &flat, &[&by_column, &shapes[1]], &[&flat], &[]));
        out.push(new_node("Softmax", &weights, &[&flat], &[&weights], &[int_attribute("axis", axis)]));
        out.push(new_node("MatMul", &answer, &[&weights, piece(4)], &[&answer], &[]));
        heads.push(answer);
    }
    let heads: Vec<&str> = heads.iter().map(String::as_str).collect();
    out.push(new_node("Concat", &format!("{base}_heads"), &heads, &[value.outputs[0]], &[int_attribute("axis", 0)]));
    out
}

fn retype(constant: &[u8], to: i64) -> Result<Vec<u8>, String> {
    let dims = repeated(&fields(constant)?, 1);
    let mut raw = Vec::new();
    for value in int64s(constant) {
        match to {
            FLOAT => raw.extend((value as f32).to_le_bytes()),
            FLOAT16 => raw.extend(half(value).ok_or_else(|| format!("{value} is not a whole float16"))?.to_le_bytes()),
            other => return Err(format!("index arithmetic in element type {other}")),
        }
    }
    Ok(tensor("", &dims, to, &raw))
}

fn half(value: i64) -> Option<u16> {
    if value == 0 {
        return Some(0);
    }
    let sign = if value < 0 { 0x8000u16 } else { 0 };
    let magnitude = value.unsigned_abs();
    let exponent = 63 - magnitude.leading_zeros();

    if exponent > 15 || magnitude & ((1 << exponent.saturating_sub(10)) - 1) != 0 {
        return None;
    }
    let mantissa = ((magnitude << 10) >> exponent) as u16 & 0x3ff;
    Some(sign | ((exponent as u16 + 15) << 10) | mantissa)
}

fn data_type(tensor: &[u8]) -> Option<i64> {
    int(&fields(tensor).ok()?, 2).map(|value| value as i64)
}

fn int64s(tensor: &[u8]) -> Vec<i64> {
    let Ok(fields) = fields(tensor) else { return Vec::new() };
    match fields.iter().find(|field| field.number == 9).and_then(|field| field.bytes().ok()) {
        Some(raw) => raw.as_chunks::<8>().0.iter().map(|bytes| i64::from_le_bytes(*bytes)).collect(),
        None => repeated(&fields, 7),
    }
}

struct Node<'a> {
    fields: Vec<Field<'a>>,
    inputs: Vec<&'a str>,
    outputs: Vec<&'a str>,
    name: &'a str,
    op: &'a str,
}

impl<'a> Node<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        let fields = fields(bytes)?;
        let strings = |number| fields.iter().filter(|f| f.number == number).filter_map(|f| f.text()).collect::<Vec<_>>();
        let (inputs, outputs) = (strings(1), strings(2));
        let name = string(&fields, 3).unwrap_or("");
        let op = string(&fields, 4).unwrap_or("");
        Ok(Self { fields, inputs, outputs, name, op })
    }

    fn attribute(&self, name: &str) -> Option<Vec<Field<'a>>> {
        self.fields
            .iter()
            .filter(|field| field.number == 5)
            .filter_map(|field| fields(field.bytes().ok()?).ok())
            .find(|attribute| string(attribute, 1) == Some(name))
    }

    fn int(&self, name: &str) -> Option<i64> {
        int(&self.attribute(name)?, 3).map(|value| value as i64)
    }

    fn tensor(&self, name: &str) -> Option<&'a [u8]> {
        self.attribute(name)?.iter().find(|field| field.number == 5)?.bytes().ok()
    }

    fn with_attribute(&self, name: &str, with: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for field in &self.fields {
            let named = field.number == 5
                && field.bytes().ok().and_then(|bytes| fields(bytes).ok()).is_some_and(|a| string(&a, 1) == Some(name));
            match named {
                true => put_bytes(&mut out, 5, with),
                false => out.extend_from_slice(field.raw),
            }
        }
        out
    }

    fn with_input(&self, index: usize, name: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let mut seen = 0;
        for field in &self.fields {
            if field.number == 1 {
                seen += 1;
                if seen == index + 1 {
                    put_bytes(&mut out, 1, name.as_bytes());
                    continue;
                }
            }
            out.extend_from_slice(field.raw);
        }
        out
    }
}

fn emit(out: &mut Vec<Cow<[u8]>>, number: u32, bytes: Vec<u8>) {
    let mut field = Vec::new();
    put_bytes(&mut field, number, &bytes);
    out.push(Cow::Owned(field));
}

fn new_node(op: &str, name: &str, inputs: &[&str], outputs: &[&str], attributes: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for input in inputs {
        put_bytes(&mut out, 1, input.as_bytes());
    }
    for output in outputs {
        put_bytes(&mut out, 2, output.as_bytes());
    }
    put_bytes(&mut out, 3, name.as_bytes());
    put_bytes(&mut out, 4, op.as_bytes());
    for attribute in attributes {
        put_bytes(&mut out, 5, attribute);
    }
    out
}

fn int_attribute(name: &str, value: i64) -> Vec<u8> {
    let mut out = Vec::new();
    put_bytes(&mut out, 1, name.as_bytes());
    put_int(&mut out, 3, value);
    put_int(&mut out, 20, 2);
    out
}

fn tensor_attribute(name: &str, tensor: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    put_bytes(&mut out, 1, name.as_bytes());
    put_bytes(&mut out, 5, tensor);
    put_int(&mut out, 20, 4);
    out
}

fn tensor(name: &str, dims: &[i64], data_type: i64, raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for dim in dims {
        put_int(&mut out, 1, *dim);
    }
    put_int(&mut out, 2, data_type);
    if !name.is_empty() {
        put_bytes(&mut out, 8, name.as_bytes());
    }
    put_bytes(&mut out, 9, raw);
    out
}

struct Field<'a> {
    number: u32,
    value: Value<'a>,

    raw: &'a [u8],
}

enum Value<'a> {
    Int(u64),
    Bytes(&'a [u8]),
    Fixed,
}

impl<'a> Field<'a> {
    fn bytes(&self) -> Result<&'a [u8], String> {
        match self.value {
            Value::Bytes(bytes) => Ok(bytes),
            _ => Err(format!("field {} is not bytes", self.number)),
        }
    }

    fn text(&self) -> Option<&'a str> {
        std::str::from_utf8(self.bytes().ok()?).ok()
    }
}

fn fields(data: &[u8]) -> Result<Vec<Field<'_>>, String> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let start = at;
        let key = varint(data, &mut at)?;
        let value = match key & 7 {
            0 => Value::Int(varint(data, &mut at)?),
            1 | 5 => {
                at += if key & 7 == 1 { 8 } else { 4 };
                Value::Fixed
            }
            2 => {
                let length = varint(data, &mut at)? as usize;
                let bytes = data.get(at..at + length).ok_or("a field runs past its message")?;
                at += length;
                Value::Bytes(bytes)
            }
            kind => return Err(format!("wire type {kind}")),
        };
        let raw = data.get(start..at).ok_or("a field runs past its message")?;
        out.push(Field { number: (key >> 3) as u32, value, raw });
    }
    Ok(out)
}

fn varint(data: &[u8], at: &mut usize) -> Result<u64, String> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *data.get(*at).ok_or("a varint runs past its message")?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err("a varint longer than ten bytes".into())
}

fn string<'a>(fields: &[Field<'a>], number: u32) -> Option<&'a str> {
    fields.iter().find(|field| field.number == number)?.text()
}

fn int(fields: &[Field], number: u32) -> Option<u64> {
    fields.iter().find_map(|field| match field.value {
        Value::Int(value) if field.number == number => Some(value),
        _ => None,
    })
}

fn repeated(fields: &[Field], number: u32) -> Vec<i64> {
    let mut out = Vec::new();
    for field in fields.iter().filter(|field| field.number == number) {
        match field.value {
            Value::Int(value) => out.push(value as i64),
            Value::Bytes(packed) => {
                let mut at = 0;
                while let Ok(value) = varint(packed, &mut at) {
                    out.push(value as i64);
                }
            }
            Value::Fixed => {}
        }
    }
    out
}

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push(value as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn put_int(out: &mut Vec<u8>, number: u32, value: i64) {
    put_varint(out, u64::from(number) << 3);
    put_varint(out, value as u64);
}

fn put_bytes(out: &mut Vec<u8>, number: u32, bytes: &[u8]) {
    put_varint(out, u64::from(number) << 3 | 2);
    put_varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests;
