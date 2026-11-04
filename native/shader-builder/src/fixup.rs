// Makes rust-gpu's SPIR-V acceptable to naga, which turns it into WGSL for
// the browser:
//
// 1. Structs outside buffers lose their member offsets. rust-gpu lays out
//    every struct as Rust does (a Vec3 is 12 bytes, aligned to 4) and says
//    so; naga holds a struct's offsets to WGSL's layout rules (a vec3 is
//    aligned to 16), so it rejects them as overlapping. Only structs in
//    buffers need fixed offsets, and keep theirs.
// 2. Non-finite float constants (infinity, NaN) become calls to functions
//    returning them (a bitcast of a u32 another function returns), as
//    Futhark's WGSL does: WGSL rejects constant expressions that are not
//    finite, and folds a bitcast of a constant into one. Composite
//    constants holding them are rebuilt where they are used.

use std::collections::{HashMap, HashSet};

const OP_DECORATE: u32 = 71;
const OP_MEMBER_DECORATE: u32 = 72;
const OP_TYPE_INT: u32 = 21;
const OP_TYPE_FLOAT: u32 = 22;
const OP_TYPE_ARRAY: u32 = 28;
const OP_TYPE_RUNTIME_ARRAY: u32 = 29;
const OP_TYPE_STRUCT: u32 = 30;
const OP_TYPE_FUNCTION: u32 = 33;
const OP_CONSTANT: u32 = 43;
const OP_CONSTANT_COMPOSITE: u32 = 44;
const OP_FUNCTION: u32 = 54;
const OP_FUNCTION_END: u32 = 56;
const OP_FUNCTION_CALL: u32 = 57;
const OP_VARIABLE: u32 = 59;
const OP_COMPOSITE_CONSTRUCT: u32 = 80;
const OP_BITCAST: u32 = 124;
const OP_LABEL: u32 = 248;
const OP_RETURN_VALUE: u32 = 254;
const BLOCK: u32 = 2;
const BUFFER_BLOCK: u32 = 3;
const OFFSET: u32 = 35;

type Inst = Vec<u32>;

fn op(x: &Inst) -> u32 {
    x[0] & 0xffff
}

fn inst(opcode: u32, operands: &[u32]) -> Inst {
    let mut v = vec![((operands.len() as u32 + 1) << 16) | opcode];
    v.extend_from_slice(operands);
    v
}

pub fn fixup(words: &[u32]) -> Result<Vec<u32>, String> {
    if words.len() < 5 || words[0] != 0x0723_0203 {
        return Err("not a SPIR-V module".into());
    }
    let mut header = words[..5].to_vec();
    let mut insts: Vec<Inst> = vec![];
    let mut i = 5;
    while i < words.len() {
        let n = (words[i] >> 16) as usize;
        if n == 0 || i + n > words.len() {
            return Err("malformed SPIR-V".into());
        }
        insts.push(words[i..i + n].to_vec());
        i += n;
    }

    // 1. offsets only where buffers need them
    let blocks: Vec<u32> = insts.iter().filter(|x| op(x) == OP_DECORATE && (x[2] == BLOCK || x[2] == BUFFER_BLOCK)).map(|x| x[1]).collect();
    let mut members: HashMap<u32, Vec<u32>> = HashMap::new();
    for x in &insts {
        match op(x) {
            OP_TYPE_STRUCT => {
                members.insert(x[1], x[2..].to_vec());
            }
            OP_TYPE_ARRAY | OP_TYPE_RUNTIME_ARRAY => {
                members.insert(x[1], vec![x[2]]);
            }
            _ => {}
        }
    }
    let mut keep = HashSet::new();
    let mut stack = blocks;
    while let Some(t) = stack.pop() {
        if keep.insert(t) {
            stack.extend(members.get(&t).into_iter().flatten());
        }
    }
    insts.retain(|x| !(op(x) == OP_MEMBER_DECORATE && x[3] == OFFSET && !keep.contains(&x[1])));

    // 2. non-finite constants
    let floats: HashSet<u32> = insts.iter().filter(|x| op(x) == OP_TYPE_FLOAT && x[2] == 32).map(|x| x[1]).collect();
    let mut bad: HashMap<u32, u32> = HashMap::new(); // constant id -> bits
    for x in &insts {
        if op(x) == OP_CONSTANT && floats.contains(&x[1]) && !f32::from_bits(x[3]).is_finite() {
            bad.insert(x[2], x[3]);
        }
    }
    if bad.is_empty() {
        return Ok(flatten(header, insts));
    }
    // composites holding them, transitively
    let mut comps: HashMap<u32, (u32, Vec<u32>)> = HashMap::new();
    loop {
        let mut changed = false;
        for x in &insts {
            if op(x) == OP_CONSTANT_COMPOSITE && !comps.contains_key(&x[2]) && x[3..].iter().any(|a| bad.contains_key(a) || comps.contains_key(a)) {
                comps.insert(x[2], (x[1], x[3..].to_vec()));
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let fty = *floats.iter().next().unwrap();
    let uty = insts
        .iter()
        .find(|x| op(x) == OP_TYPE_INT && x[2] == 32 && x[3] == 0)
        .map(|x| x[1])
        .ok_or("no u32 type in the module")?;
    let mut bound = header[3];
    let mut new = || {
        bound += 1;
        bound - 1
    };
    let (tf_u, tf_f) = (new(), new());
    let types = vec![inst(OP_TYPE_FUNCTION, &[tf_u, uty]), inst(OP_TYPE_FUNCTION, &[tf_f, fty])];
    let mut consts = vec![];
    let mut fns = vec![];
    let mut makers: HashMap<u32, u32> = HashMap::new(); // bits -> function id
    let mut bits_all: Vec<u32> = bad.values().copied().collect();
    bits_all.sort();
    bits_all.dedup();
    for bits in bits_all {
        let cu = new();
        consts.push(inst(OP_CONSTANT, &[uty, cu, bits]));
        let (h, hl, f, fl, u, r) = (new(), new(), new(), new(), new(), new());
        fns.extend([
            inst(OP_FUNCTION, &[uty, h, 0, tf_u]),
            inst(OP_LABEL, &[hl]),
            inst(OP_RETURN_VALUE, &[cu]),
            inst(OP_FUNCTION_END, &[]),
            inst(OP_FUNCTION, &[fty, f, 0, tf_f]),
            inst(OP_LABEL, &[fl]),
            inst(OP_FUNCTION_CALL, &[uty, u, h]),
            inst(OP_BITCAST, &[fty, r, u]),
            inst(OP_RETURN_VALUE, &[r]),
            inst(OP_FUNCTION_END, &[]),
        ]);
        makers.insert(bits, f);
    }
    let dead = |x: &Inst| (op(x) == OP_CONSTANT || op(x) == OP_CONSTANT_COMPOSITE) && (bad.contains_key(&x[2]) || comps.contains_key(&x[2]));

    let mut out: Vec<Inst> = vec![];
    let mut first_fn = None;
    let mut k = 0;
    while k < insts.len() {
        if op(&insts[k]) != OP_FUNCTION {
            if !dead(&insts[k]) {
                out.push(insts[k].clone());
            }
            k += 1;
            continue;
        }
        first_fn.get_or_insert(out.len());
        let mut j = k;
        while op(&insts[j]) != OP_FUNCTION_END {
            j += 1;
        }
        let mut f = insts[k..=j].to_vec();
        let mut used: Vec<u32> = f.iter().flat_map(|y| y[1..].iter().copied()).filter(|a| bad.contains_key(a) || comps.contains_key(a)).collect();
        used.sort();
        used.dedup();
        if !used.is_empty() {
            // made at the start of the function's first block
            let mut rep: HashMap<u32, u32> = HashMap::new();
            let mut made: Vec<Inst> = vec![];
            fn make(c: u32, rep: &mut HashMap<u32, u32>, made: &mut Vec<Inst>, bad: &HashMap<u32, u32>, comps: &HashMap<u32, (u32, Vec<u32>)>, makers: &HashMap<u32, u32>, fty: u32, new: &mut dyn FnMut() -> u32) -> u32 {
                if let Some(&r) = rep.get(&c) {
                    return r;
                }
                let r = if let Some(bits) = bad.get(&c) {
                    let r = new();
                    made.push(inst(OP_FUNCTION_CALL, &[fty, r, makers[bits]]));
                    r
                } else if let Some((ty, parts)) = comps.get(&c) {
                    let ids: Vec<u32> = parts.iter().map(|&p| make(p, rep, made, bad, comps, makers, fty, new)).collect();
                    let r = new();
                    let mut ops = vec![*ty, r];
                    ops.extend(ids);
                    made.push(inst(OP_COMPOSITE_CONSTRUCT, &ops));
                    r
                } else {
                    return c;
                };
                rep.insert(c, r);
                r
            }
            for &c in &used {
                make(c, &mut rep, &mut made, &bad, &comps, &makers, fty, &mut new);
            }
            let label = f.iter().position(|y| op(y) == OP_LABEL).ok_or("function without a block")?;
            let mut at = label + 1;
            while at < f.len() && op(&f[at]) == OP_VARIABLE {
                at += 1;
            }
            for y in f.iter_mut() {
                for a in y[1..].iter_mut() {
                    if let Some(&r) = rep.get(a) {
                        *a = r;
                    }
                }
            }
            f.splice(at..at, made);
        }
        out.extend(f);
        k = j + 1;
    }
    let first_fn = first_fn.unwrap_or(out.len());
    let mut all = out[..first_fn].to_vec();
    all.extend(types);
    all.extend(consts);
    all.extend(out[first_fn..].iter().cloned());
    all.extend(fns);
    header[3] = bound;
    Ok(flatten(header, all))
}

fn flatten(header: Vec<u32>, insts: Vec<Inst>) -> Vec<u32> {
    let mut w = header;
    for x in insts {
        w.extend(x);
    }
    w
}
