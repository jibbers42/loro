//! `cargo run --release -- [first_seed] [seed_count] [peers] [steps] [seconds]`
//!
//! Runs `seed_count` random histories (or as many as fit in `seconds`, when
//! given) and compares stock `loro` 1.17.1 with the fork on each. Exits with
//! status 1 at the first difference.

use loro_fork_differential::{compare_seed, Config};
use std::time::Instant;

fn main() {
    let args: Vec<u64> = std::env::args()
        .skip(1)
        .map(|a| a.parse().expect("numeric arguments"))
        .collect();
    let first = args.first().copied().unwrap_or(0);
    let count = args.get(1).copied().unwrap_or(50);
    let peers = args.get(2).copied().unwrap_or(3) as usize;
    let steps = args.get(3).copied().unwrap_or(120) as usize;
    let seconds = args.get(4).copied();
    if std::env::var("DUMP_TRACE").is_ok() {
        // Print one world's trace, to see what a history contains.
        let cfg = Config {
            peers: peers.max(1),
            steps,
            ..Config::default()
        };
        let world = vec![loro_fork_differential::Lib::Fork; cfg.peers];
        for line in loro_fork_differential::run_world(first, cfg, &world) {
            println!("{line}");
        }
        return;
    }
    let start = Instant::now();
    let mut lines = 0;
    let mut done = 0;
    for seed in first.. {
        match seconds {
            Some(s) if start.elapsed().as_secs() >= s => break,
            None if seed >= first + count => break,
            _ => {}
        }
        // Vary the shape with the seed: more peers, longer or shorter runs,
        // bigger pastes.
        let cfg = Config {
            peers: if peers == 0 {
                1 + (seed % 4) as usize
            } else {
                peers
            },
            steps,
            large_insert: [600, 3000, 9000][(seed % 3) as usize],
            // Stock 1.17.1 needs seconds per checkout across a paste much
            // larger than this, which would leave time for few histories.
            huge_percent: [0, 4, 8][((seed / 3) % 3) as usize],
            huge_insert: [10_000, 20_000, 40_000][((seed / 9) % 3) as usize],
        };
        match compare_seed(seed, cfg) {
            Ok(n) => lines += n,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        done += 1;
        if done % 20 == 0 {
            eprintln!("{done} seeds, {:.0}s", start.elapsed().as_secs_f64());
        }
    }
    println!(
        "{done} seeds from {first} identical ({lines} compared trace lines per world, {:.0}s)",
        start.elapsed().as_secs_f64()
    );
}
