# Local Commits After a Snapshot: No Lookup in the Encoded Store

Verified against code 2026-11-11.

## What was slow

With a local-update subscriber, every local commit exports the change it has
just made: the commit hook in `crates/loro-internal/src/txn.rs` calls
`export_fast_updates_in_range`, which reaches
`ChangeStore::iter_blocks_inner` (`crates/loro-internal/src/oplog/change_store.rs`)
through `export_blocks_in_range`. That first calls `load_blocks_in_range` to
make sure the blocks of the span are in `mem_parsed_kv`, the parsed cache.

`load_blocks_in_range` asked the encoded store (`external_kv`, a
`MemKvStore`) even when the cache already held the span:

1. `kv.scan(start, end)`: `SsTableIter::new_scan`
   (`crates/kv-store/src/sstable.rs`) decodes the SSTable block the start key
   falls in and the block the end key falls in.
2. Unless a block starts exactly at `start`, `ensure_id_lte`:
   `kv.scan(Unbounded, Included(id)).rfind(..)`, which decodes the first block
   of the table and the block `id` falls in.

A doc that never exported or imported a snapshot has an empty encoded store,
so this cost nothing. After a snapshot export (history is flushed into the
store) or import (the doc starts from it) the store holds all history, and a
new change's key falls in or next to the last block of some peer. One big text
insert is one change block of the same size, and the cost of a commit grew
with the size of the blocks the two scans decode. (Why the SSTable block
cache, `BLOCK_CACHE_MAX_BYTES`, does not absorb this was not established.)
Measured on 1.17.1, release build, idle machine: about 2 ms per one-character
commit at 1 MB and 5.5 ms at 5 MB, against 0.007 ms on a doc that never saw a
snapshot (and 0.007 ms with the change below). Which peer id types matters only because it decides which block the
new key lands next to.

## What it does now

`load_blocks_in_range` returns at once when `ChangeStoreInner::caches_range`
finds a gapless run of cached blocks of the peer over the whole range. That
rests on two facts the store already relies on (`load_parsed_block` trusts a
cached block that holds an id; the scan loop skips keys that are cached):
blocks of one peer never overlap, and a cached block is the current version of
the block with its key. A range that is only partly cached, or unbounded,
takes the old path unchanged.

Nothing encoded changes: the export reads the same parsed blocks as before.

## Tests

- `crates/loro/tests/perf_commit_after_snapshot.rs`:
  `local_updates_after_snapshot_match_explicit_exports` compares the
  subscriber's bytes with explicit exports for peers that sort before, at and
  after the snapshot's author; the ignored `perf_typing_after_snapshot` prints
  the timings.

## Not covered

Other callers still scan when the cache does not hold the range, for example
the first read of old history after loading a snapshot. That is a one-time
load per block, not a per-commit cost.
