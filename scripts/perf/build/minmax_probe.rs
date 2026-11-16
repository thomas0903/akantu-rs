// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs project tooling (not ported from Akantu): does the result of
// f64::min / f64::max / f64::clamp / NaN-producing operations depend on the
// target CPU? Every pair of special values (signed zeros, infinities, NaNs
// of both signs and payloads, subnormals, ordinary values) in both operand
// orders, through scalar calls (black_box) and through loops over arrays
// that LLVM vectorises; prints one line per operation with the FNV-1a hash
// of the result bits, and the bits of the first few results. Compile with
//   rustc -C opt-level=3 [-C target-cpu=x86-64-v4] minmax_probe.rs
// and compare the outputs of the builds (identical lines = identical bits).
use std::hint::black_box;

fn vals() -> Vec<f64> {
    let mut v = vec![
        0.0, -0.0, 1.0, -1.0, 2.5, -2.5, f64::INFINITY, f64::NEG_INFINITY, f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE, 5e-324, -5e-324, f64::MAX, f64::MIN,
    ];
    for bits in [0x7ff8_0000_0000_0000_u64, 0xfff8_0000_0000_0000, 0x7ff8_0000_0000_0123, 0xfff4_0000_0000_0001, 0x7ff0_0000_0000_0001] {
        v.push(f64::from_bits(bits));
    }
    v
}

fn fnv(h: &mut u64, x: f64) {
    for b in x.to_bits().to_le_bytes() {
        *h ^= u64::from(b);
        *h = h.wrapping_mul(0x0100_0000_01b3);
    }
}

#[inline(never)]
fn vec_op(a: &[f64], b: &[f64], out: &mut [f64], op: u8) {
    match op {
        0 => out.iter_mut().zip(a.iter().zip(b)).for_each(|(o, (x, y))| *o = x.min(*y)),
        1 => out.iter_mut().zip(a.iter().zip(b)).for_each(|(o, (x, y))| *o = x.max(*y)),
        2 => out.iter_mut().zip(a.iter().zip(b)).for_each(|(o, (x, y))| *o = x + y),
        3 => out.iter_mut().zip(a.iter().zip(b)).for_each(|(o, (x, y))| *o = x * y),
        4 => out.iter_mut().zip(a.iter().zip(b)).for_each(|(o, (x, y))| *o = x / y),
        5 => out.iter_mut().zip(a.iter().zip(b)).for_each(|(o, (x, y))| *o = x - y),
        6 => out.iter_mut().zip(a.iter().zip(b)).for_each(|(o, (x, y))| *o = if x < y { *x } else { *y }),
        7 => out.iter_mut().zip(a.iter().zip(b)).for_each(|(o, (x, y))| *o = x.abs().max(y.abs())),
        8 => out.iter_mut().zip(a.iter()).for_each(|(o, x)| *o = x.sqrt()),
        _ => out.iter_mut().zip(a.iter().zip(b)).for_each(|(o, (x, y))| *o = x.clamp(-1.0, 1.0) + 0.0 * y),
    }
}

fn main() {
    let v = vals();
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for x in &v {
        for y in &v {
            a.push(*x);
            b.push(*y);
        }
    }
    // reductions as the kernels write them (fold with max of |x|)
    let names = ["min", "max", "add", "mul", "div", "sub", "lt-select", "absmax", "sqrt", "clamp"];
    for (op, name) in names.iter().enumerate() {
        let op = op as u8;
        let mut out = vec![0.0; a.len()];
        vec_op(black_box(&a), black_box(&b), &mut out, black_box(op));
        let mut hv = 0xcbf2_9ce4_8422_2325_u64;
        out.iter().for_each(|x| fnv(&mut hv, *x));
        let mut hs = 0xcbf2_9ce4_8422_2325_u64;
        for (x, y) in a.iter().zip(&b) {
            let (x, y) = (black_box(*x), black_box(*y));
            let r = match op {
                0 => x.min(y),
                1 => x.max(y),
                2 => x + y,
                3 => x * y,
                4 => x / y,
                5 => x - y,
                6 => if x < y { x } else { y },
                7 => x.abs().max(y.abs()),
                8 => x.sqrt(),
                _ => x.clamp(-1.0, 1.0) + 0.0 * y,
            };
            fnv(&mut hs, black_box(r));
        }
        println!("{name:10} vector {hv:016x} scalar {hs:016x} min(+0,-0)={:016x} min(-0,+0)={:016x}",
            black_box(0.0_f64).min(black_box(-0.0)).to_bits(), black_box(-0.0_f64).min(black_box(0.0)).to_bits());
    }
    let fold = a.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    let fold_min = b.iter().fold(f64::INFINITY, |m, x| m.min(*x));
    let nan = black_box(0.0_f64) / black_box(0.0);
    let inf_sub = black_box(f64::INFINITY) - black_box(f64::INFINITY);
    println!("fold absmax {:016x} fold min {:016x} 0/0 {:016x} inf-inf {:016x}", fold.to_bits(), fold_min.to_bits(), nan.to_bits(), inf_sub.to_bits());
}
