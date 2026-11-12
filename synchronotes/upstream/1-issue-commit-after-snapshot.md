# Issue draft (loro-dev/loro)

**Title:** Local commits take milliseconds each on a snapshot-loaded doc with `subscribeLocalUpdates` once history exceeds the block cache

**Versions:** `main` at 62fb36f9 (also `loro` / `loro-crdt` 1.17.1, where it starts at about 1 MB). Windows 11 x64, rustc 1.99.1, release build.

With a local-update subscriber, every local commit exports the change it has just made. On a document that has exported a snapshot or was loaded from one, that export looks the change up in the encoded change store, although the change's block is still in memory. Each lookup reads history blocks from the SSTable, and once those no longer fit the 4 MiB block cache they are decompressed again on every commit.

One-character insert plus commit, 200 commits, median (ms), `main` at 62fb36f9:

| History | Never snapshotted | After exporting a snapshot | Loaded from a snapshot |
|---|---|---|---|
| One text insert of 4 MB | 0.007 | 0.007 | 0.008 |
| One text insert of 5 MB | 0.007 | 5.4 | 5.6 |
| One text insert of 8 MB | 0.007 | 6.8 | 6.8 |
| 8 MB in 4 KB commits, typing peer id above the author's | 0.007 | 1.5 | 1.2 |
| 8 MB in 4 KB commits, typing peer id below the author's | 0.007 | n/a | 0.007 |
| Any of the above, no subscriber | 0.001 | 0.001 | 0.001 |

On 1.17.1 the same path cost about 2 ms per commit at 1 MB and 5.5 ms at 5 MB (12.5 ms in the WASM build under Node); #1260 (block caches sized to the table) is the likely reason the smaller sizes are fast on `main`.

**Where the time goes** (read from the code, confirmed by the fix removing it; no profiler):
the commit hook in `txn.rs` calls `export_fast_updates_in_range` → `ChangeStore::export_blocks_in_range` → `iter_blocks_inner` → `load_blocks_in_range`, which does

1. `external_kv.scan(start, end)`: `SsTableIter::new_scan` reads the block the start key falls in and the block the end key falls in;
2. `ensure_id_lte(start)`: `scan(Unbounded, Included(id)).rfind(..)`, which reads the first block of the table and the block `id` falls in,

even when `mem_parsed_kv` already holds the whole requested range.

**Reproduction**

```rust
use loro::{ExportMode, LoroDoc};
use std::time::Instant;

fn main() {
    let src = LoroDoc::new();
    src.set_peer_id(1).unwrap();
    src.get_text("note").insert(0, &"lorem ipsum ".repeat(5_000_000 / 12)).unwrap();
    src.commit();

    let doc = LoroDoc::new();
    doc.set_peer_id(2).unwrap();
    doc.import(&src.export(ExportMode::Snapshot).unwrap()).unwrap();
    let _sub = doc.subscribe_local_update(Box::new(|_| true));

    let text = doc.get_text("note");
    let start = Instant::now();
    for i in 0..200 {
        text.insert(1000 + i, "x").unwrap();
        doc.commit();
    }
    println!("{:.3} ms per commit", start.elapsed().as_secs_f64() * 1000.0 / 200.0);
}
```

Prints about 10 ms per commit on `main`. Importing `ExportMode::all_updates()` instead of the snapshot gives 0.008 ms, and dropping the subscriber gives 0.06 ms (which is what the fix brings the first case to).

I have a small fix (return early from `load_blocks_in_range` when cached blocks cover the range) with a test and will open a pull request.
