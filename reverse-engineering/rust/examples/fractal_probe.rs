//! Batch sampler for differential testing against the original machine code.
//!
//! Reads lines of the form
//!
//! ```text
//! w h level flags seed S x1 y1 x2 y2 ...     -> sample(x, y) for each pair
//! w h level flags seed P p1 p2 ...           -> percentile(p) for each value
//! ```
//!
//! builds the fractal from the first five numbers and prints one line of results.
//! The driver is `.agents/skills/reverse-engineering-executables/scripts/emu/diff_sample.py`,
//! which runs `sampleHeight` (`0x5e2180`) and `percentileLookup` (`0x5e2280`) on the
//! same fractals under Unicorn.

use civ3mapgen::fractal::Fractal;
use std::io::BufRead;

fn main() {
    for line in std::io::stdin().lock().lines() {
        let line = line.expect("stdin");
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() < 6 {
            continue;
        }
        let n: Vec<i64> = t[..5].iter().map(|v| v.parse().expect("number")).collect();
        let f = Fractal::generate(n[0] as i32, n[1] as i32, n[2] as i32, n[3] as u32, n[4] as u32);
        let args: Vec<i32> = t[6..].iter().map(|v| v.parse().expect("number")).collect();
        let out: Vec<String> = match t[5] {
            "S" => args.chunks(2).map(|p| f.sample(p[0], p[1]).to_string()).collect(),
            "P" => args.iter().map(|&p| f.percentile(p).to_string()).collect(),
            other => panic!("unknown mode {other}"),
        };
        println!("{}", out.join(" "));
    }
}
