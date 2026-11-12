# PR draft: branch `pr/cached-change-blocks` → loro-dev/loro `main`

**Title:** perf: skip the encoded store for cached change blocks

## Summary

With a local-update subscriber, every local commit exports the change it has just made. `ChangeStore::load_blocks_in_range` looked that change up in the encoded change store although its block was still in `mem_parsed_kv`. After a snapshot export or import the two scans it does read blocks of old history, and once those no longer fit the 4 MiB block cache each commit pays a decompression.

`load_blocks_in_range` now returns early when the cached blocks of the peer cover the whole requested range without a gap (`ChangeStoreInner::caches_range`). A range that is only partly cached, or unbounded, takes the old path unchanged.

Fixes #ISSUE.

## Rationale

It rests on what the store already assumes: blocks of one peer never overlap, and a cached block is the current version of the block with its key (`load_parsed_block` trusts a cached block that holds an id; the scan loop skips cached keys). Details and measurements are in `context/commit-export-after-snapshot.md`.

One behaviour difference, on an error path: for a fully cached range the old code could still load the preceding encoded block of the same peer through `ensure_id_lte`, and `try_iter_changes` failed if that unrelated block was corrupt (the export then wrote nothing). A fully cached range no longer touches it. The note in the context article says so.

## Measurements

One-character insert plus commit with a local-update subscriber, median of 200, release build, Windows 11 x64:

| History | State | `main` (62fb36f9) | This branch |
|---|---|---|---|
| One text insert of 4 MB | loaded from a snapshot | 0.008 ms | 0.006 ms |
| One text insert of 5 MB | loaded from a snapshot | 5.6 ms | 0.006 ms |
| One text insert of 8 MB | loaded from a snapshot | 6.8 ms | 0.006 ms |
| 8 MB in 4 KB commits | loaded, typing peer id above the author's | 1.2 ms | 0.006 ms |
| any | never snapshotted | 0.007 ms | 0.007 ms |

## Validation

All on Windows 11 x64, rustc 1.99.1.

- New tests: a unit test for `caches_range` (cached run, gap, other peer, bounds, a change that exists only in the cache), and `local_updates_after_snapshot_match_explicit_exports`, which compares the subscriber's bytes with explicit exports for typing peers that sort before, at and after the snapshot's author. The ignored `perf_typing_after_snapshot` prints the table above.
- `cargo test --workspace --exclude examples --features=test_utils,jsonpath`: 2,189 passed on this branch merged with two unrelated test-only branches, 2,186 on `main`; the difference is the new tests. Failing on both: two `loro-common` tests that expect `/` in a source path (Windows), and `loro-common`'s `test_long_string_cache*`, which are flaky here. (`cargo nextest` was not available; `examples` is excluded because one example links macOS-only symbols.)
- Loom (`multi_thread_test`): 10 passed.
- Differential run against `main` at 62fb36f9: 1,814 random text histories (1 to 5 peers; pastes up to 40,000 characters, deletes, marks, undo/redo with remote edits, relay of local-update blobs, `import_batch`, snapshots, shallow snapshots, checkout, diff, `fork_at`, `revert_to`), each run in an all-`main` world, an all-branch world and two mixed worlds. Every observable was identical: text, rich text, versions, undo results, diffs, events, and the bytes of every local update, export and snapshot.
- `crates/fuzz` harness on random action lists with debug assertions (targets text, diff_calc, one_doc, all; 4 minutes each on 4 threads, about 344,000 inputs): no failure. Random input, not coverage-guided; `cargo fuzz` and the saved corpus were not run.
- An in-process benchmark of ordinary cases (typing, small undo/redo, 3,000 keystroke imports, concurrent merge, checkout, diff, snapshot export/import), `main` and branch alternating: no systematic difference; rows scatter within about ±10% in both directions on a machine that was not idle. `caches_range` is `#[inline(never)]` because inlining it into `load_blocks_in_range` made unrelated rows of that benchmark 3 to 8% slower (it is not even called in one of them), and the difference went away out of line.
- `cargo clippy --all-features -- -Dwarnings` fails on `main` under rustc 1.99 with 9 errors in untouched code; this branch has the same set and none on the changed lines.
- The WASM package builds from this branch (with the Windows build fix applied) and its vitest suite gives 562 passed, 1 failed (`oom.test.ts`, a Windows path problem unrelated to this change).

Not run: Linux and macOS, coverage-guided fuzzing, the Bun tests of the package.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
