//! `cargo run --release --bin fuzz-driver -- <target> <seconds> [threads] [first_seed]`
//!
//! Runs Loro's own fuzz harness (`crates/fuzz`, with its convergence,
//! checkout, undo and event oracles) on random inputs for a fixed time.
//! It stands in for `cargo fuzz run`, which needs a nightly toolchain and
//! libFuzzer; inputs here are random, not coverage-guided.
//!
//! Targets: `text`, `all`, `one_doc`, `diff_calc` (the same entry points as
//! the fuzz targets of those names in `crates/fuzz/fuzz/fuzz_targets`).

use arbitrary::{Arbitrary, Unstructured};
use fuzz::{
    test_multi_sites, test_multi_sites_on_one_doc,
    test_multi_sites_on_one_doc_with_peer_seed_and_targets, Action, FuzzTarget,
};
use loro_fork::ContainerType;
use loro_fork_differential::Rng;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The action mix of the `diff_calc` fuzz target: every action is followed by
/// a sync, diff, checkout, fork or shallow round trip.
fn biased(actions: Vec<Action>) -> Vec<Action> {
    let mut out = Vec::with_capacity(actions.len() * 2);
    for (i, action) in actions.into_iter().take(48).enumerate() {
        out.push(action);
        let site = ((i * 37) % 251) as u8;
        let other = site.wrapping_add(1);
        let version = (i as u32).wrapping_mul(97);
        out.push(match i % 8 {
            0 => Action::Sync {
                from: site,
                to: other,
            },
            1 => Action::DiffApply {
                from: site,
                to: other,
            },
            2 => Action::Checkout { site, to: version },
            3 => Action::ForkAt { site, to: version },
            4 => Action::ImportShallow { site, from: other },
            5 => Action::ExportShallow { site },
            6 => Action::StateOnlyRoundTrip { site },
            _ => Action::Commit { site },
        });
    }
    out
}

fn run(target: &str, seed: u64, mut actions: Vec<Action>) {
    let odd = seed % 2 == 1;
    match target {
        "text" => test_multi_sites(5, vec![FuzzTarget::Text], &mut actions),
        "all" if odd => test_multi_sites_on_one_doc(5, &mut actions),
        "all" => test_multi_sites(5, vec![FuzzTarget::All], &mut actions),
        "one_doc" => test_multi_sites_on_one_doc(5, &mut actions),
        "diff_calc" if odd => test_multi_sites_on_one_doc_with_peer_seed_and_targets(
            5,
            seed,
            vec![ContainerType::Text],
            &mut biased(actions),
        ),
        "diff_calc" => test_multi_sites(5, vec![FuzzTarget::Text], &mut biased(actions)),
        other => panic!("unknown target {other}"),
    }
}

fn input(seed: u64) -> Vec<Action> {
    let mut rng = Rng::new(seed);
    // `Vec::<Action>::arbitrary` on random bytes stops after about one
    // element, so the actions are decoded one at a time.
    let count = [8, 20, 50, 150][rng.below(4)];
    let bytes: Vec<u8> = (0..count * 64).map(|_| rng.next() as u8).collect();
    let mut u = Unstructured::new(&bytes);
    let mut actions = Vec::with_capacity(count);
    while actions.len() < count && !u.is_empty() {
        match Action::arbitrary(&mut u) {
            Ok(a) => actions.push(a),
            Err(_) => break,
        }
    }
    actions
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let target = args.first().cloned().unwrap_or_else(|| "text".into());
    let seconds: u64 = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(60);
    let threads: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(8);
    let first: u64 = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(0);

    let next = Arc::new(AtomicU64::new(first));
    let failed = Arc::new(AtomicBool::new(false));
    let actions_run = Arc::new(AtomicU64::new(0));
    let start = Instant::now();
    // The harness panics on purpose inside `catch_unwind` for some inputs;
    // only failures that reach this driver are reported.
    std::panic::set_hook(Box::new(|_| {}));
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let (next, failed, actions_run, target) = (
                next.clone(),
                failed.clone(),
                actions_run.clone(),
                target.clone(),
            );
            std::thread::Builder::new()
                .stack_size(64 << 20)
                .spawn(move || {
                    while start.elapsed() < Duration::from_secs(seconds)
                        && !failed.load(Ordering::Relaxed)
                    {
                        let seed = next.fetch_add(1, Ordering::Relaxed);
                        let actions = input(seed);
                        if actions.is_empty() {
                            continue;
                        }
                        actions_run.fetch_add(actions.len() as u64, Ordering::Relaxed);
                        let t = target.clone();
                        if let Err(e) = std::panic::catch_unwind(move || run(&t, seed, actions)) {
                            let msg = e
                                .downcast_ref::<String>()
                                .cloned()
                                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                                .unwrap_or_default();
                            eprintln!("FAILED target {target} seed {seed}: {msg}");
                            failed.store(true, Ordering::Relaxed);
                        }
                    }
                })
                .unwrap()
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let inputs = next.load(Ordering::Relaxed) - first;
    println!(
        "target {target}: {inputs} inputs from seed {first}, {} actions, {threads} threads, {:.0}s, {}",
        actions_run.load(Ordering::Relaxed),
        start.elapsed().as_secs_f64(),
        if failed.load(Ordering::Relaxed) { "FAILED" } else { "no failures" }
    );
    if failed.load(Ordering::Relaxed) {
        std::process::exit(1);
    }
}
