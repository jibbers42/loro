//! `cargo run --release --bin bench -- [rounds] [filter]`
//!
//! Ordinary-case timings and peak heap use, stock `loro` 1.17.1 against the
//! fork, in one process. Each scenario runs `rounds` times per library, the
//! two libraries alternating, and the median is reported. These are the
//! cases that must not get slower; the large-text cases are timed by
//! `crates/loro/tests/perf_large_text.rs`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            let now = CURRENT.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = System.realloc(ptr, layout, new_size);
        if !p.is_null() {
            if new_size >= layout.size() {
                let d = new_size - layout.size();
                let now = CURRENT.fetch_add(d, Ordering::Relaxed) + d;
                PEAK.fetch_max(now, Ordering::Relaxed);
            } else {
                CURRENT.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        p
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

/// (scenario, metric, value). Times are in milliseconds, memory in MiB.
type Rows = Vec<(&'static str, String, f64)>;

fn gen_text(n: usize) -> String {
    let line = "loro note sync the a markdown edit peer text of and **bold** `code`\n";
    line.repeat(n / line.len() + 1)[..n].to_string()
}

macro_rules! scenarios {
    ($m:ident, $k:ident) => {
        mod $m {
            use super::{gen_text, Rows};
            use loro_fork_differential::Rng;
            use std::sync::{Arc, Mutex};
            use std::time::Instant;
            use $k::{ExportMode, LoroDoc, UndoManager};

            fn ms(s: Instant) -> f64 {
                s.elapsed().as_secs_f64() * 1000.0
            }

            fn median_p95(mut v: Vec<f64>) -> (f64, f64) {
                v.sort_by(|a, b| a.partial_cmp(b).unwrap());
                (v[v.len() / 2], v[(v.len() * 95 / 100).min(v.len() - 1)])
            }

            fn doc(peer: u64) -> LoroDoc {
                let d = LoroDoc::new();
                d.set_peer_id(peer).unwrap();
                d
            }

            /// One-character insert plus commit, with a local-update
            /// subscriber, `n` times in the middle of the text.
            fn typing(rows: &mut Rows, name: &'static str, d: &LoroDoc, n: usize) {
                let _sub = d.subscribe_local_update(Box::new(|_| true));
                let t = d.get_text("note");
                let mut pos = t.len_unicode() / 2;
                let mut times = Vec::with_capacity(n);
                for _ in 0..n {
                    let s = Instant::now();
                    t.insert(pos, "x").unwrap();
                    d.commit();
                    times.push(ms(s));
                    pos += 1;
                }
                let (med, p95) = median_p95(times);
                rows.push((name, "typing commit, median".into(), med));
                rows.push((name, "typing commit, p95".into(), p95));
            }

            pub fn type_small(rows: &mut Rows) {
                let d = doc(1);
                d.get_text("note").insert(0, &gen_text(1000)).unwrap();
                d.commit();
                typing(rows, "typing in a 1 KB doc", &d, 3000);
            }

            pub fn type_1mb(rows: &mut Rows) {
                let d = doc(1);
                d.get_text("note").insert(0, &gen_text(1_000_000)).unwrap();
                d.commit();
                typing(rows, "typing in a 1 MB doc (one insert)", &d, 3000);
            }

            pub fn type_1mb_pieces(rows: &mut Rows) {
                let d = doc(1);
                let t = d.get_text("note");
                let piece = gen_text(4096);
                for _ in 0..245 {
                    t.insert(t.len_unicode(), &piece).unwrap();
                    d.commit();
                }
                typing(rows, "typing in a 1 MB doc (4 KB commits)", &d, 3000);
            }

            fn undo_redo(rows: &mut Rows, name: &'static str, d: &LoroDoc, edits: usize) {
                let mut undo = UndoManager::new(d);
                undo.set_merge_interval(0);
                undo.set_max_undo_steps(edits + 10);
                let t = d.get_text("note");
                let mut rng = Rng::new(7);
                for i in 0..edits {
                    let len = t.len_unicode();
                    if i % 4 == 3 && len > 4 {
                        t.delete(rng.below(len - 3), 3).unwrap();
                    } else {
                        t.insert(rng.below(len + 1), "word ").unwrap();
                    }
                    d.commit();
                }
                let expected = t.to_string();
                let (undo_ms, mem) = super::peak_of(|| {
                    let s = Instant::now();
                    for _ in 0..edits {
                        assert!(undo.undo().unwrap());
                    }
                    ms(s)
                });
                let s = Instant::now();
                for _ in 0..edits {
                    assert!(undo.redo().unwrap());
                }
                let redo_ms = ms(s);
                assert_eq!(t.to_string(), expected);
                rows.push((name, format!("undo {edits} small edits, total"), undo_ms));
                rows.push((name, format!("redo {edits} small edits, total"), redo_ms));
                rows.push((name, "peak heap during undo (MiB)".into(), mem));
            }

            pub fn undo_small(rows: &mut Rows) {
                let d = doc(1);
                d.get_text("note").insert(0, &gen_text(1000)).unwrap();
                d.commit();
                undo_redo(rows, "undo/redo in a 1 KB doc", &d, 300);
            }

            pub fn undo_after_big(rows: &mut Rows) {
                let d = doc(1);
                d.get_text("note").insert(0, &gen_text(100_000)).unwrap();
                d.commit();
                undo_redo(
                    rows,
                    "undo/redo of small edits made after a 100 KB insert",
                    &d,
                    100,
                );
            }

            /// `n` keystroke-sized updates of one peer, as local-update blobs.
            fn keystroke_updates(n: usize) -> (LoroDoc, Vec<Vec<u8>>) {
                let d = doc(1);
                let blobs = Arc::new(Mutex::new(Vec::new()));
                let sink = blobs.clone();
                let sub = d.subscribe_local_update(Box::new(move |b| {
                    sink.lock().unwrap().push(b.to_vec());
                    true
                }));
                let t = d.get_text("note");
                let mut rng = Rng::new(11);
                let mut pos = 0;
                for i in 0..n {
                    if i % 50 == 0 {
                        pos = rng.below(t.len_unicode() + 1);
                    }
                    t.insert(pos, "k").unwrap();
                    pos += 1;
                    d.commit();
                }
                drop(sub);
                let b = blobs.lock().unwrap().clone();
                (d, b)
            }

            pub fn import_small(rows: &mut Rows) {
                let name = "import of 3000 keystroke updates";
                let (src, blobs) = keystroke_updates(3000);
                let expected = src.get_text("note").to_string();
                let one = doc(2);
                let s = Instant::now();
                for b in &blobs {
                    one.import(b).unwrap();
                }
                rows.push((name, "one by one, total".into(), ms(s)));
                assert_eq!(one.get_text("note").to_string(), expected);
                let batch = doc(3);
                let (t, mem) = super::peak_of(|| {
                    let s = Instant::now();
                    batch.import_batch(&blobs).unwrap();
                    ms(s)
                });
                rows.push((name, "import_batch, total".into(), t));
                rows.push((name, "peak heap during import_batch (MiB)".into(), mem));
                assert_eq!(batch.get_text("note").to_string(), expected);
                // Into a doc with a concurrent edit, so the tracker replays.
                let other = doc(4);
                other.get_text("note").insert(0, "concurrent").unwrap();
                other.commit();
                let s = Instant::now();
                for b in &blobs {
                    other.import(b).unwrap();
                }
                rows.push((
                    name,
                    "one by one into a doc with a concurrent edit".into(),
                    ms(s),
                ));
            }

            /// Two peers edit the same 10 KB text concurrently.
            fn concurrent_pair() -> (LoroDoc, LoroDoc, $k::Frontiers) {
                let a = doc(1);
                a.get_text("note").insert(0, &gen_text(10_000)).unwrap();
                a.commit();
                let b = doc(2);
                b.import(&a.export(ExportMode::all_updates()).unwrap())
                    .unwrap();
                let base = a.oplog_frontiers();
                for (d, seed) in [(&a, 21u64), (&b, 22)] {
                    let t = d.get_text("note");
                    let mut rng = Rng::new(seed);
                    for _ in 0..150 {
                        for _ in 0..20 {
                            let len = t.len_unicode();
                            match rng.below(10) {
                                0..=6 => t.insert(rng.below(len + 1), "ab").unwrap(),
                                7 => t
                                    .mark(
                                        rng.below(len / 2)..len / 2 + rng.below(len / 2),
                                        "bold",
                                        true,
                                    )
                                    .unwrap(),
                                _ => t.delete(rng.below(len - 2), 2).unwrap(),
                            }
                        }
                        d.commit();
                    }
                }
                (a, b, base)
            }

            pub fn concurrent(rows: &mut Rows) {
                let name = "two peers, 3000 concurrent edits each on 10 KB";
                let (a, b, base) = concurrent_pair();
                let updates = b.export(ExportMode::updates(&a.oplog_vv())).unwrap();
                let (t, mem) = super::peak_of(|| {
                    let s = Instant::now();
                    a.import(&updates).unwrap();
                    ms(s)
                });
                rows.push((name, "merge the other peer's edits (import)".into(), t));
                rows.push((name, "peak heap during merge (MiB)".into(), mem));
                let latest = a.oplog_frontiers();
                let s = Instant::now();
                a.checkout(&base).unwrap();
                rows.push((name, "checkout to the common base".into(), ms(s)));
                let s = Instant::now();
                a.checkout_to_latest();
                rows.push((name, "checkout to latest".into(), ms(s)));
                let s = Instant::now();
                let _ = a.diff(&base, &latest).unwrap();
                rows.push((name, "diff base to latest".into(), ms(s)));
                let s = Instant::now();
                let _ = a.fork_at(&base).unwrap();
                rows.push((name, "fork_at base".into(), ms(s)));

                let name = "snapshot of that doc";
                let s = Instant::now();
                let snap = a.export(ExportMode::Snapshot).unwrap();
                rows.push((name, "export snapshot".into(), ms(s)));
                let (t, mem) = super::peak_of(|| {
                    let s = Instant::now();
                    let d = doc(9);
                    d.import(&snap).unwrap();
                    let _ = d.get_text("note").to_string();
                    ms(s)
                });
                rows.push((name, "import snapshot and read text".into(), t));
                rows.push((name, "peak heap during snapshot import (MiB)".into(), mem));
                let s = Instant::now();
                let all = a.export(ExportMode::all_updates()).unwrap();
                rows.push((name, "export all updates".into(), ms(s)));
                let s = Instant::now();
                let d = doc(9);
                d.import(&all).unwrap();
                rows.push((name, "import all updates".into(), ms(s)));
                let s = Instant::now();
                let _ = a.export(ExportMode::shallow_snapshot(&latest)).unwrap();
                rows.push((name, "export shallow snapshot".into(), ms(s)));
            }

            pub fn scattered_history(rows: &mut Rows) {
                let name = "one peer, 6000 scattered small edits";
                let d = doc(1);
                let t = d.get_text("note");
                let mut rng = Rng::new(31);
                let s = Instant::now();
                let mut mid = d.oplog_frontiers();
                for i in 0..6000 {
                    let len = t.len_unicode();
                    if i % 5 == 4 && len > 3 {
                        t.delete(rng.below(len - 2), 2).unwrap();
                    } else {
                        t.insert(rng.below(len + 1), "edit ").unwrap();
                    }
                    d.commit();
                    if i == 3000 {
                        mid = d.oplog_frontiers();
                    }
                }
                rows.push((name, "apply the edits locally, total".into(), ms(s)));
                let latest = d.oplog_frontiers();
                let (tm, mem) = super::peak_of(|| {
                    let s = Instant::now();
                    d.checkout(&mid).unwrap();
                    ms(s)
                });
                rows.push((name, "checkout to the middle".into(), tm));
                rows.push((name, "peak heap during checkout (MiB)".into(), mem));
                let s = Instant::now();
                d.checkout_to_latest();
                rows.push((name, "checkout to latest".into(), ms(s)));
                let s = Instant::now();
                let _ = d.diff(&mid, &latest).unwrap();
                rows.push((name, "diff middle to latest".into(), ms(s)));
                let s = Instant::now();
                let _ = d.diff(&latest, &mid).unwrap();
                rows.push((name, "diff latest to middle".into(), ms(s)));
            }

            /// The fixed path at a size stock can still do quickly.
            pub fn big_insert(rows: &mut Rows) {
                let name = "one 100 KB insert";
                let d = doc(1);
                let mut undo = UndoManager::new(&d);
                let t = d.get_text("note");
                t.insert(0, "before\n").unwrap();
                d.commit();
                let s = Instant::now();
                t.insert(7, &gen_text(100_000)).unwrap();
                d.commit();
                rows.push((name, "insert and commit".into(), ms(s)));
                let (tm, mem) = super::peak_of(|| {
                    let s = Instant::now();
                    assert!(undo.undo().unwrap());
                    ms(s)
                });
                rows.push((name, "undo".into(), tm));
                rows.push((name, "peak heap during undo (MiB)".into(), mem));
                let s = Instant::now();
                assert!(undo.redo().unwrap());
                rows.push((name, "redo".into(), ms(s)));
            }

            pub fn all(filter: &str) -> Rows {
                let mut rows = Rows::new();
                let list: [(&str, fn(&mut Rows)); 9] = [
                    ("type_small", type_small),
                    ("type_1mb", type_1mb),
                    ("type_1mb_pieces", type_1mb_pieces),
                    ("undo_small", undo_small),
                    ("undo_after_big", undo_after_big),
                    ("import_small", import_small),
                    ("concurrent", concurrent),
                    ("scattered_history", scattered_history),
                    ("big_insert", big_insert),
                ];
                for (name, f) in list {
                    if name.contains(filter) {
                        f(&mut rows);
                    }
                }
                rows
            }
        }
    };
}

/// Peak heap growth in MiB while `f` runs.
fn peak_of<T>(f: impl FnOnce() -> T) -> (T, f64) {
    let base = CURRENT.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let out = f();
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
    (out, peak as f64 / (1024.0 * 1024.0))
}

scenarios!(stock, loro_stock);
scenarios!(fork, loro_fork);

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rounds: usize = args.first().map(|s| s.parse().unwrap()).unwrap_or(7);
    let filter = args.get(1).cloned().unwrap_or_default();
    let mut stock_runs: Vec<Rows> = Vec::new();
    let mut fork_runs: Vec<Rows> = Vec::new();
    for round in 0..rounds {
        // Alternate which library goes first.
        if round % 2 == 0 {
            stock_runs.push(stock::all(&filter));
            fork_runs.push(fork::all(&filter));
        } else {
            fork_runs.push(fork::all(&filter));
            stock_runs.push(stock::all(&filter));
        }
        eprintln!("round {} done", round + 1);
    }
    println!("scenario\tmetric\tstock 1.17.1\tfork\tfork/stock");
    for i in 0..stock_runs[0].len() {
        let (scenario, metric, _) = &stock_runs[0][i];
        let s = median(stock_runs.iter().map(|r| r[i].2).collect());
        let f = median(fork_runs.iter().map(|r| r[i].2).collect());
        println!("{scenario}\t{metric}\t{s:.4}\t{f:.4}\t{:.2}", f / s);
    }
}
