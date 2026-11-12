# Loro fork for Synchronotes: large text in linear time

Date: 2026-11-11. Clone: `D:\code\projects\loro` (fork
https://github.com/jibbers42/loro of loro-dev/loro). Branch:
`synchronotes/fix-quadratic-diff`, local only, nothing pushed, nothing posted
upstream. Machine: Windows 11, 12 logical cores, rustc 1.99.1, Node 24.22.1.

## Result

Undo, redo, version diff, checkout and batch import of large text are now
linear in the text size, and the separate slow keystroke after a snapshot is
gone. Results are identical to stock Loro 1.17.1 in every comparison made,
including exported bytes, and no ordinary case got slower.

| Case (Rust, release, idle machine) | Stock 1.17.1 | Fork |
|---|---|---|
| Undo one 1 MB insert | 18.7 s | 4.9 ms |
| Undo one 5 MB insert | not run (est. minutes) | 26 ms |
| `import_batch` of 8 × 100 KB | 5.4 s | 1.6 ms |
| Diff across a 1 MB insert | 19 to 23 s | 3 to 5 ms |
| One keystroke, 5 MB doc loaded from a snapshot | 5.5 ms | 0.007 ms |

## Base

The branch starts at tag `loro-crdt@1.17.1` (6114e10e), not `loro-v1.17.1`
(ef83c083). The first is one commit after the second and that commit only
bumps the WASM package version and changelogs, so 6114e10e is both the
Rust crates 1.17.1 and the `loro-crdt` 1.17.1 package. The sources of the
crates.io crates (`loro` 1.17.1, `loro-internal` 1.17.1, `loro-common`
1.17.0, `loro-kv-store` 1.17.0, `loro-delta` 1.13.0, `loro-rle` 1.6.0) are
file-for-file the same as the tag's.

`upstream/main` is 27 commits ahead of the tag (crates at 1.18.0, not
released). It is not a few safe commits: it has features, format-adjacent
fixes and several performance changes. See "Upstream" below.

## Cause

One cause explains diff, checkout, undo, redo and `import_batch`.

The text tracker (`crates/loro-internal/src/container/richtext/tracker.rs`)
keeps one inserted text as one rope leaf (`tracker/crdt_rope.rs`), but maps op
ids to leaves in fragments of at most 256 ids (`tracker/id_to_cursor.rs`,
`MAX_FRAGMENT_LEN`). `Tracker::_checkout` emits one `LeafUpdate` per
fragment. For a 1 MB insert that is 3,906 updates, all for the same leaf and
with the same effect. Then:

1. `CrdtRope::update` split the leaf at every fragment boundary and merged
   the pieces straight back, returning the same leaf index once per fragment.
2. `Tracker::update_insert_by_split` took each returned index and rewrote
   every fragment of that leaf in the id-to-cursor map.

That is (size / 256)² fragment rewrites for each checkout across the op.
Undo and redo do two checkouts; `diff`, `checkout` and `fork_at` use the same
call; `import_batch` imports detached and then checks out, so it always pays
it, while a plain import of one peer's updates applies them directly, which
is why "one by one" was fast. Undo of a small edit made before a large insert
was slow for the same reason: the checkout crosses the large insert.

Evidence: reading the code; the growth (100 KB → 300 KB → 1 MB: about 9.5× and
11× per step, table below); and removing each of the two steps removes the
corresponding part of the cost (step 2 alone: undo of 1 MB 66 s → 75 ms under
load, still quadratic, 1.7 s at 5 MB; both: 6 ms and 29 ms). No profiler was
used.

The slow keystroke after a snapshot is a second, unrelated cause; see
"Second fix".

## Upstream

- loro-dev/loro#1236, merged on `main` after 1.17.1: updates each reported
  leaf once (step 2).
- loro-dev/loro#1082, open since 2026-09-01, not merged, no review comments:
  coalesces the per-fragment updates (step 1). Its description gives the same
  cause.
- loro-dev/loro#1263 (open issue): every text undo replays the text's whole
  history. Different problem, linear per undo. `main` has a fix (#1217,
  "keep the full richtext tracker across undo and checkout") that is not in
  1.17.1 and not on this branch.
- Open PR #956 ("improve text checkout scalability") is a large rewrite; not
  used.
- No upstream issue or change was found for the snapshot path.
  `ChangeStore::load_blocks_in_range` is unchanged on `main`.

## Fix

Two commits, both taken from upstream authors with `git cherry-pick -x`:

- `895364c9`: upstream's merged #1236 (6 lines in `tracker.rs`).
- `28cddbf7`: the commit of upstream PR #1082 (25 lines in
  `tracker/crdt_rope.rs`): adjacent updates for the same leaf with the same
  `set_future` and `delete_times_diff` and contiguous id spans become one.

Why it is safe: merging `[a, b)` and `[b, c)` only removes a split point
inside one leaf; every id still gets the same status change exactly once.
Overlapping spans (two deletes of the same text) are not contiguous and never
merge. Nothing that is encoded, exported or returned changes: the tracker is
a scratch structure for computing diffs. No public API, no format, no merge
rule is touched.

### Second fix (snapshot path)

`2b1626b9`. With a local-update subscriber every local commit exports its own
change. `ChangeStore::load_blocks_in_range`
(`crates/loro-internal/src/oplog/change_store.rs`) asked the encoded history
store for the change's block although the block was in memory. After a
snapshot export or import that store holds all history, and each of the two
scans decodes the blocks it starts and ends in: megabytes for a big text.
The change returns early when in-memory blocks cover the whole requested
range without a gap (`ChangeStoreInner::caches_range`, 40 lines). It rests on
what the store already assumes elsewhere (`load_parsed_block` trusts a cached
block; the scan loop skips cached keys). A range that is not fully cached
takes the old path unchanged.

Why the 4 MB block cache does not absorb the cost on stock was not
established.

## Commits on the branch

Base `6114e10e` (tag `loro-crdt@1.17.1`), then:

| Commit | What |
|---|---|
| `9400897f` | test: timings for undo, diff, checkout, batch import of big text (ignored tests) |
| `895364c9` | fix: upstream #1236, cherry-picked (author williamjakfar) |
| `28cddbf7` | perf: upstream PR #1082's commit, cherry-picked (author Magnus Rex) |
| `5f70b432` | test: time travel and undo over multi-fragment text, plus `context/large-text-linear-time.md` |
| `514011d4` | build: let the wasm package build on Windows (two script fixes) |
| `2b1626b9` | perf: skip the encoded store for cached change blocks, test, `context/commit-export-after-snapshot.md` |
| `182fcd7c` | test: timings for big deletes, nested and concurrent big inserts |
| `43f11ffd` | docs: idle-machine timings in the large-text note |
| tip | this folder (`synchronotes/`): report and tools. Not for upstream. |

The cherry-picked commits carry my `Co-Authored-By` trailer as the task
required, in addition to their original authors. `895364c9` keeps upstream's
subject line unchanged, which is 81 characters, over the 72 limit; it was
left so the commit can be matched to upstream's by its subject.

## Numbers

All Rust numbers: release build, this machine, idle unless noted. "Stock" is
a git worktree at the tag, same test file.

### Large text (`crates/loro/tests/perf_large_text.rs`)

Milliseconds.

| Case | Size | Stock 1.17.1 | Fork |
|---|---|---|---|
| Undo one insert | 100 KB | 176 | 0.6 |
| | 300 KB | 1,678 | 1.3 |
| | 1 MB | 18,661 | 4.9 |
| | 5 MB | not run | 26.2 |
| Redo it | 100 KB | 203 | 0.7 |
| | 300 KB | 1,933 | 1.8 |
| | 1 MB | 22,539 | 6.1 |
| | 5 MB | not run | 34.1 |
| Diff forward / backward | 100 KB | 182 / 217 | 5.8 / 0.6 |
| | 300 KB | 1,701 / 2,365 | 1.2 / 1.1 |
| | 1 MB | 18,964 / 23,436 | 4.7 / 3.4 |
| | 5 MB | not run | 19.6 / 19.3 |
| Checkout before / back to latest | 100 KB | 73 / 143 | 0.3 / 0.2 |
| | 300 KB | 774 / 1,390 | 0.6 / 0.5 |
| | 1 MB | 7,833 / 15,068 | 2.0 / 1.6 |
| | 5 MB | not run | 9.7 / 9.0 |
| Undo a small edit made before the insert | 100 KB | 138 | 0.3 |
| | 300 KB | 1,328 | 0.8 |
| | 1 MB | 15,026 | 3.0 |
| | 5 MB | not run | 12.0 |
| Undo a small edit made after the insert | any | 0.1 to 0.3 | 0.1 to 0.6 |
| `import_batch`, N × 100 KB | 2 | 282 | 1.0 |
| | 4 | 1,157 | 1.0 |
| | 8 | 5,425 | 1.6 |
| | 50 | not run | 11.6 |
| Same updates one by one | 8 | 1.1 | 1.1 |
| | 50 | not run | 6.7 |
| Undo a select-all delete of two nested inserts | 2 × 1 MB | 67,230 | 11.0 |
| Undo the inner of two nested inserts | 2 × 1 MB | 117,809 | 16.0 |
| Import both into a doc with a concurrent edit | 2 × 1 MB | 33,738 | 11.9 |
| | 2 × 5 MB | not run | 60.5 |

Stock at 5 MB was not run: at 25 times the 1 MB cost each operation would
take several minutes. The first diff in a process includes warm-up (5.8 ms at
100 KB).

### Keystroke after a snapshot (`crates/loro/tests/perf_commit_after_snapshot.rs`)

One-character insert plus commit, with a local-update subscriber; median,
milliseconds.

| Doc | State | Stock 1.17.1 | Fork |
|---|---|---|---|
| 1 MB, one insert | never snapshotted | 0.008 | 0.008 |
| | after exporting a snapshot | 1.67 | 0.007 |
| | loaded from a snapshot | 1.86 | 0.007 |
| 2 MB, one insert | loaded from a snapshot | 2.60 | 0.006 |
| 5 MB, one insert | after exporting a snapshot | 5.57 | 0.006 |
| | loaded from a snapshot | 5.39 | 0.006 |
| 5 MB, 4 KB commits | loaded, typing peer id above the author's | 2.66 | 0.008 |
| | loaded, typing peer id below the author's | 0.008 | 0.007 |
| any | loaded, no subscriber | 0.001 | 0.001 |

### Ordinary cases (`synchronotes/differential`, `bench` binary)

Stock and fork linked into one process, 9 rounds each, alternating, median.
Milliseconds unless marked.

| Scenario | Metric | Stock 1.17.1 | Fork | Fork / stock |
|---|---|---|---|---|
| typing in a 1 KB doc | typing commit, median | 0.0066 | 0.0064 | 0.97 |
|  | typing commit, p95 | 0.0075 | 0.0109 | 1.45 |
| typing in a 1 MB doc (one insert) | typing commit, median | 0.0066 | 0.0065 | 0.98 |
|  | typing commit, p95 | 0.0077 | 0.0083 | 1.08 |
| typing in a 1 MB doc (4 KB commits) | typing commit, median | 0.0068 | 0.0067 | 0.99 |
|  | typing commit, p95 | 0.0087 | 0.0073 | 0.84 |
| undo/redo in a 1 KB doc | undo 300 small edits, total | 520.489 | 517.359 | 0.99 |
|  | redo 300 small edits, total | 814.943 | 805.166 | 0.99 |
|  | peak heap during undo (MiB) | 0.551 | 0.551 | 1.00 |
| undo/redo of small edits made after a 100 KB insert | undo 100 small edits, total | 80.1562 | 79.2163 | 0.99 |
|  | redo 100 small edits, total | 98.5464 | 99.7329 | 1.01 |
|  | peak heap during undo (MiB) | 0.2486 | 0.2485 | 1.00 |
| import of 3000 keystroke updates | one by one, total | 25.9614 | 25.5713 | 0.98 |
|  | import_batch, total | 28.5368 | 28.2567 | 0.99 |
|  | peak heap during import_batch (MiB) | 1.47 | 1.47 | 1.00 |
|  | one by one into a doc with a concurrent edit | 1677.51 | 1664.05 | 0.99 |
| two peers, 3000 concurrent edits each on 10 KB | merge the other peer's edits (import) | 14.4011 | 14.3133 | 0.99 |
|  | peak heap during merge (MiB) | 4.7129 | 4.7129 | 1.00 |
|  | checkout to the common base | 26.468 | 26.7808 | 1.01 |
|  | checkout to latest | 20.6143 | 20.1772 | 0.98 |
|  | diff base to latest | 58.4598 | 57.8948 | 0.99 |
|  | fork_at base | 50.1019 | 49.723 | 0.99 |
| snapshot of that doc | export snapshot | 2.3194 | 2.3368 | 1.01 |
|  | import snapshot and read text | 0.314 | 0.3102 | 0.99 |
|  | peak heap during snapshot import (MiB) | 0.3957 | 0.3958 | 1.00 |
|  | export all updates | 1.6754 | 1.5988 | 0.95 |
|  | import all updates | 21.5325 | 21.6499 | 1.01 |
|  | export shallow snapshot | 53.1095 | 54.1372 | 1.02 |
| one peer, 6000 scattered small edits | apply the edits locally, total | 11.3291 | 12.745 | 1.12 |
|  | checkout to the middle | 14.5238 | 14.7337 | 1.01 |
|  | peak heap during checkout (MiB) | 5.0297 | 5.0297 | 1.00 |
|  | checkout to latest | 11.3063 | 11.6073 | 1.03 |
|  | diff middle to latest | 27.805 | 27.7025 | 1.00 |
|  | diff latest to middle | 26.6935 | 26.9446 | 1.01 |
| one 100 KB insert | insert and commit | 0.2845 | 0.2582 | 0.91 |
|  | undo | 123.814 | 0.564 | 0.00 |
|  | peak heap during undo (MiB) | 4.6775 | 0.0822 | 0.02 |
|  | redo | 145.032 | 0.6505 | 0.00 |

Peak heap is the growth in allocated bytes during the operation, counted by
a wrapping allocator; it is equal everywhere except the large-insert undo,
where the fork needs far less.

Two rows looked worse in that run and were repeated three times with 15
rounds each: "apply the edits locally" gave 0.98, 1.02 and 1.01, and the
95th percentile of a 7 µs typing commit gave 1.14, 0.85 and 1.20, so both are
noise. No ordinary case is slower.

Two stock behaviours stand out in that table and are not changed by the
fork: 300 small undos in a 1 KB doc take 0.5 s (upstream issue #1263), and
importing 3000 keystroke updates one by one into a doc that has a concurrent
edit takes 1.7 s, about 0.55 ms each.

### Loro's own benchmark

`cargo bench -p loro-internal --features test_utils --bench text_r` (the B4
editing trace, 260,000 edits), criterion medians in milliseconds, one run per
build. Two cases of that file (`DecodeUpdates B4Parallel`,
`DecodeSnapshot B4Parallel`) were not run: their setup replays the whole
trace with two imports per edit and did not finish in 16 minutes on stock.

| Benchmark | Stock 1.17.1 | Fork | Fork / stock |
|---|---|---|---|
| B4 (apply the trace locally) | 114.8 | 138.4 | 1.21 |
| B4 with 100K actors history | 117.9 | 142.8 | 1.21 |
| B4 Obs | 158.7 | 159.2 | 1.00 |
| B4 encode snapshot | 0.248 | 0.286 | 1.15 |
| B4 encode updates | 3.755 | 3.621 | 0.96 |
| B4 decode snapshot | 0.164 | 0.158 | 0.96 |
| B4 import updates | 14.11 | 13.42 | 0.95 |
| B4_Per100_Txn | 129.8 | 126.2 | 0.97 |
| B4 One Op One Txn | 381.5 | 390.4 | 1.02 |
| B4 One Op One Txn Obs | 969.8 | 881.4 | 0.91 |
| B4DirectSync | 5,658 | 6,002 | 1.06 |
| B4Parallel | 498.0 | 528.3 | 1.06 |

Three rows of that single run looked worse (1.21, 1.21, 1.15), so those three
were run three more times on each build, alternating:

| Benchmark | Stock, three runs | Fork, three runs |
|---|---|---|
| B4 | 122.1, 115.9, 114.4 | 115.9, 118.2, 123.7 |
| B4 with 100K actors history | 114.5, 114.8, 120.4 | 121.2, 116.4, 122.4 |
| B4 encode snapshot (µs) | 201, 204, 194 | 191, 197, 223 |

The ranges overlap; the first reading was a one-off. Averaged, the fork is
1% to 3% higher on these three, which is inside the run-to-run spread of
either build (stock alone spans 114 to 122). B4DirectSync and B4Parallel
(1.06 in the single run) were not repeated; they exercise import with
concurrency, which the in-process benchmark above measures at 0.98 to 1.01.
I cannot claim a difference in either direction below about 5% from these
runs.

### JavaScript (`loro-crdt` WASM build of the fork, Node 24)

My copies of the research scripts (`diff.mjs`, `undo-group.mjs`,
`import-batch.mjs`, `paste-undo-by-delete.mjs`), run against the published
`loro-crdt` 1.17.1 and against the package built from this branch.
Milliseconds, single runs, idle machine.

| Case | Size | Published 1.17.1 | Fork build |
|---|---|---|---|
| Undo one paste (`undo-group.mjs`) | 100 KB | 371 | 13.9 |
| | 300 KB | 3,139 | 15.8 |
| | 1 MB | 38,300 | 23.2 |
| | 5 MB | not run | 57.4 |
| Undo a 1 MB paste made of 100 KB commits | 1 MB | not run | 19.6 |
| Diff forward / backward (`diff.mjs`) | 100 KB | 356 / 274 | 14.0 / 1.7 |
| | 300 KB | 2,124 / 2,542 | 3.6 / 2.2 |
| | 1 MB | 26,749 / 28,181 | 12.2 / 5.0 |
| | 5 MB | not run | 84.0 / 24.2 |
| Checkout old / latest | 1 MB | 9,431 / 18,814 | 2.8 / 2.1 |
| | 5 MB | not run | 13.6 / 9.5 |
| `importBatch`, N × 100 KB (`import-batch.mjs`) | 2 | 818 | 6.1 |
| | 4 | 3,325 | 6.6 |
| | 8 | 14,509 | 8.6 |
| | 50 | not run (research note: over 120 s) | 31.7 |
| Same blobs one by one | 8 | 9.9 | 10.4 |
| Keystroke on a doc loaded from a snapshot, median | 1 MB | 0.066 | 0.057 |
| | 2 MB | 2.83 | 0.054 |
| | 5 MB | 12.5 | 0.041 |
| `paste-undo-by-delete.mjs`, 1 MB: undo earlier typing | 1 MB | not run (research note: 29 s) | 9.3 |

The first call in a Node process includes WASM warm-up (the 14 ms forward
diff at 100 KB).

### Package size

| File | Published 1.17.1 | Fork build |
|---|---|---|
| `bundler/loro_wasm_bg.wasm` | 3,596,988 B (gzip 1,198,806, brotli 841,462) | 3,583,269 B (gzip 1,198,800, brotli 841,174) |
| `nodejs/loro_wasm_bg.wasm` | 3,597,806 B | 3,584,087 B |
| `web/loro_wasm_bg.wasm` | 3,595,346 B | 3,581,627 B |
| `base64/index.js` | 5,129,988 B | 5,111,696 B |
| `npm pack` tarball | 6,908,488 B | 6,905,737 B |

Same 41 files and the same `index.js` sizes in every target. The raw size
difference comes from the compiler, not from the changes (the first build of
this branch, before the snapshot-path commit, was 3,581,947 B).

## What was tested

- **Existing test suite** (`cargo test --workspace --exclude examples
  --features=test_utils,jsonpath`; `cargo nextest` is not installed). Before
  any change: 1962 passed, 2 failed, 45 ignored, 9 min 06 s including the
  build. At the final code: 1966 passed (the 4 new tests), the same 2 failed.
  The 2 failures are `loro-common` tests that expect `/` in a source path and
  fail on Windows on the unmodified tag too. A third `loro-common` test,
  `test_long_string_cache_drop`, fails in about 2 runs out of 5 on Windows
  (it checks a process-wide table while other tests run); `loro-common` is
  not touched by this branch. The `examples` crate is excluded because one
  example links macOS-only symbols.
- **Small-fragment stress**: the whole suite again with the fragment size
  set to 4 instead of 256 in a separate worktree, so almost every text run
  spans many fragments and the new code runs constantly: 1965 passed, the
  same 2 Windows failures.
- **New tests in the repository**: `large_text_time_travel.rs` (random
  three-peer histories with multi-fragment inserts, deletes, backspace runs
  and marks; checkout, diff, fork and undo/redo against recorded states).
  Deliberately dropping a coalesced update makes both tests fail.
  `perf_commit_after_snapshot.rs` has a test that the subscriber's bytes
  equal explicit exports after a snapshot load.
- **Differential against stock 1.17.1** (`synchronotes/differential`): the
  same random history is run in four worlds (all stock, all fork, and two
  mixed assignments where neighbouring peers use different libraries), and
  every trace line must be identical: text, rich text, version vectors,
  frontiers, undo and redo results and stack sizes, diffs both ways,
  checkouts, `fork_at` results and their snapshot bytes, `revert_to`, change
  events, and the bytes of every local-update blob, update export, snapshot
  and shallow snapshot. Peers sync by relayed blobs, `import_batch`, update
  exports and snapshots, and each final export is read back by both
  libraries. 1 to 5 peers, pastes up to 40,000 characters. Long runs: 922
  histories in 5 × 25 minutes with the first fix, 355 more in 5 × 15 minutes
  with both fixes. No difference. (About 200 further histories ran before
  the harness compared events; also identical.)
- **Cross-version sync**: covered by the mixed worlds above: fork and stock
  peers import each other's updates, batches and snapshots at every step and
  converge.
- **Fuzzing**: `cargo fuzz` is not usable here (no nightly toolchain, and
  libFuzzer on Windows/MSVC). Instead Loro's fuzz harness (`crates/fuzz`,
  with its own convergence, checkout, undo and event checks) was driven with
  random action lists of 8 to 150 actions, optimized build with debug
  assertions on. Targets `text`, `diff_calc`, `one_doc`, `all`: 4 × 5 minutes
  on 4 threads with the first fix (about 418,000 inputs, 23.6 million
  actions), 4 × 4 minutes with both fixes (293,000 inputs), and 4 × 4 minutes
  on the small-fragment build (209,000 inputs). No failure. This is random
  input, not coverage-guided.
- **WASM package tests** (vitest, from the fork build): 522 passed, 1 failed,
  2 skipped. The failure is `oom.test.ts`, which imports a `D:\...` path as a
  module specifier in a child process; that cannot work on Windows and does
  not involve the changed code. The Deno and Bun test suites of the package
  were not run (Bun is not installed).

## Not verified

- Anything inside the app, a browser or WebView2. Only Node was used for the
  WASM build.
- Coverage-guided fuzzing, and the repository's saved fuzz corpus
  (`pnpm run-fuzz-corpus`).
- macOS and Linux. Everything ran on Windows.
- `pnpm check` (clippy with warnings as errors): rustc 1.99 reports lints in
  code this branch does not touch, on the tag too. No lint is reported on
  the changed lines.
- The loom tests, the MoonBit codec, the bundler smoke tests, `loro.js`.
- Lists and movable lists use the same tracker. They are covered by the test
  suite and the fuzz runs, not by the stock-versus-fork comparison, which is
  text only.
- Why stock's block cache does not hide the snapshot cost.
- Stock timings at 5 MB.

## Risks of running the fork in production

- The tracker change alters how checkout updates are grouped. A mistake
  there would show as a wrong diff: wrong undo result, wrong text after
  checkout, or a wrong event on import with concurrency. It cannot change
  stored history, because the tracker never writes it. The evidence against
  such a mistake is listed above; it is strong for text and thinner for
  lists.
- The snapshot-path change skips a lookup when the cache already has the
  range. If the cache could ever hold a stale block for a key while the
  encoded store holds a newer one, the fork would serve the stale one where
  stock, in some cases, would not. The existing code already assumes this
  cannot happen (it never replaces a cached block from the store), and no
  test or comparison found a difference, but this change is mine alone and
  has had no outside review.
- The fork is 1.17.1 plus these commits. It gets no upstream fixes until it
  is rebased. `main` already contains the first fix and a different cure for
  slow repeated undo; when 1.18 is released, moving the app to it and
  re-applying only what upstream still lacks (the coalescing commit, the
  snapshot-path change) is the lower-maintenance route.
- The built `loro-crdt` package is compiled with rustc 1.99.1 and this
  machine's toolchain, not upstream's CI. It is about 15 KB smaller than the
  published one, which shows the compilers differ.

## Suitable for upstream?

- The coalescing commit is already upstream PR #1082, by someone else. The
  useful contribution is evidence: the differential result and the
  observation that #1236 alone leaves the checkout quadratic (1.7 s to undo
  5 MB).
- The snapshot-path change is small, self-contained and has a test and a
  context article in the repository's own format. It is a candidate for a
  pull request after the owner's review.
- The Windows build fixes are also candidates.
- The timing tests follow the repository's `perf_*` convention.
- The `synchronotes/` folder is not for upstream.
- Upstream asks for a changeset for published behaviour; the cherry-picked
  #1236 brings its own, the other commits have none yet.

## How Synchronotes would use it

Nothing was changed under `D:\code\projects\synchronotes`.

### Rust core

The fork's crates keep version 1.17.1, so a patch section replaces the
crates.io ones without touching the dependency line. With the fork as a Git
submodule at, say, `vendor/loro`:

```toml
[patch.crates-io]
loro = { path = "vendor/loro/crates/loro" }
loro-internal = { path = "vendor/loro/crates/loro-internal" }
```

(or `git = "https://github.com/jibbers42/loro", rev = "<commit>"` in place
of `path` once the branch is pushed). `loro-internal` is listed so nothing
else in the graph pulls the registry copy; the other Loro crates
(`loro-common`, `loro-rle`, `loro-delta`, `loro-kv-store`,
`loro_fractional_index`) are unchanged and can stay on crates.io, or be
patched the same way to be certain of one source. Outside the Loro
workspace, `generic-btree` comes from crates.io (0.10.7), exactly as with
stock `loro`; the repository's own modified copy only applies inside its
workspace. The differential harness is built this way, so that combination
is the one that was compared with stock.

### Webview

Build the package on this machine (Git Bash), from the clone:

Tools that had to be present (none were installed system-wide by me; see
"Tools, worktrees and leftovers"): the `wasm32-unknown-unknown` Rust target
(already installed), `wasm-bindgen` 0.2.100 exactly (it must match
`Cargo.lock`), Deno 2 (2.9.6 used), pnpm 9 (9.15.9 used), Node.

```bash
cargo install wasm-bindgen-cli --version 0.2.100 --locked
```

```bash
cd /d/code/projects/loro/crates/loro-wasm
```

```bash
pnpm install --frozen-lockfile --filter loro-crdt
```

```bash
export PATH="$PWD/node_modules/.bin:$PATH"
```

```bash
deno run -A ./scripts/build.ts release && rollup -c && deno run -A ./scripts/post-rollup.ts
```

```bash
npm pack
```

The third-to-last line is the package's `build-release` script without its
test step (which also needs Bun). The cargo part takes about 7 minutes cold.
Read before running, as the task required: `scripts/build.ts`,
`scripts/post-rollup.ts`, `scripts/embed-wasm-sourcemap.mjs` (head),
`rollup.config.mjs`, `rollup.base64.config.mjs`. They are plain build
scripts: they run cargo, wasm-bindgen and rollup, fetch Deno standard-library
modules from deno.land and packages from npm, and post a size comment to
GitHub only when `CI=true` with a token. Nothing obfuscated, nothing outside
the project, no instruction aimed at AI agents.

`npm pack` writes `loro-crdt-1.17.1.tgz`, which the app can depend on as
`"loro-crdt": "file:<path>/loro-crdt-1.17.1.tgz"`, or the app can point at the
`crates/loro-wasm` folder of the submodule after building there. The package
keeps the name and version of the published one; giving it a distinct
version (for example `1.17.1-synchronotes.1` in
`crates/loro-wasm/package.json`) would make it impossible to confuse the two
in a lockfile. That choice is the owner's and was not made.

Both sides of the app should move together only for tidiness: fork and stock
interoperate, so a mixed state is safe.

## Tools, worktrees and leftovers

- Build tools were installed only into this session's scratch folder
  (`wasm-bindgen-cli` 0.2.100 with `cargo install --root`, `deno` and `pnpm`
  with `npm install` in a private folder). Nothing was added to the system
  PATH; the build commands above assume the owner installs them.
- Two temporary git worktrees were created under the scratch folder (one
  detached at `loro-crdt@1.17.1` for baseline timings, one detached at
  `5f70b432` with the fragment size set to 4) and removed again with
  `git worktree remove`.
- A remote-tracking ref `refs/remotes/upstream/pr-1082` was fetched (upstream
  PR 1082's head, read only) and left in place.
- Left in the clone, all ignored by Git: `target/`, `node_modules/` (from
  `pnpm install`), and the built package folders in `crates/loro-wasm`
  (`bundler/`, `browser/`, `nodejs/`, `web/`, `base64/`), plus
  `synchronotes/differential/target/`.
- One harness edit (adding event comparison) was made with a small script
  instead of the editor tool, against the standing rule; the result was
  type-checked and the file's line endings verified. Everything else used
  the editor tools.

## Delegation

No subagents were used; all work was done in this session.
