//! Batch fractal dumper for differential testing against the original machine code.
//!
//! Reads lines `w h level flags seed [smear_w smear_h smear_level smear_flags smear_seed]`
//! from stdin and prints the 129 x 65 height grid as hex, one line per case. When the
//! last five numbers are present, a second fractal is generated with them and passed
//! as the `out` argument (the smear source), as `generateLandmass` does.
//!
//! The driver is `.agents/skills/reverse-engineering-executables/scripts/emu_fractal.py`,
//! which runs `0x5e1b60` from the PE image under Unicorn on the same inputs.

use civ3mapgen::fractal::Fractal;
use std::io::BufRead;

fn main() {
    for line in std::io::stdin().lock().lines() {
        let line = line.expect("stdin");
        let n: Vec<i64> = line.split_whitespace().map(|t| t.parse().expect("number")).collect();
        if n.is_empty() {
            continue;
        }
        let f = if n.len() >= 10 {
            let c = Fractal::generate(n[5] as i32, n[6] as i32, n[7] as i32, n[8] as u32, n[9] as u32);
            Fractal::generate_with(n[0] as i32, n[1] as i32, n[2] as i32, n[3] as u32, n[4] as u32, Some(&c))
        } else {
            Fractal::generate(n[0] as i32, n[1] as i32, n[2] as i32, n[3] as u32, n[4] as u32)
        };
        let hex: String = f.heights().iter().map(|b| format!("{b:02x}")).collect();
        println!("{hex}");
    }
}
