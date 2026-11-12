# PR draft: branch `pr/large-text-tests` → loro-dev/loro `main`

**Title:** test: cover text ops longer than one tracker fragment

## Summary

#1236 and #1082 made a checkout across one big text op linear. Their tests are timing checks (`big_insert_import.rs`, `perf_giant_op_checkout.rs`); as #1082's description notes, the fuzz targets only insert short strings, so nothing checks the coalesced leaf updates for correctness on ops that span several id-to-cursor fragments. This adds that coverage, test-only.

- `crates/loro/tests/large_text_time_travel.rs` (runs by default, about 3 s in a debug build): random three-peer histories whose inserts, deletes, backspace runs and marks span several 256-id fragments. An observer records the text and rich text at every version; checkout, checkout between two old versions, `fork_at`, `diff` + `apply_diff`, and undo/redo (also with a remote edit in between) are checked against the recorded states.
- `crates/loro/tests/perf_large_text.rs` (ignored): prints timings by text size for undo, redo, diff, checkout, `import_batch`, a select-all delete, an insert inside an insert, and an import next to a concurrent edit.
- `context/large-text-linear-time.md`: why those paths were quadratic and what keeps them linear, linked from both `AGENTS.md` files.

## Validation

- Both default tests pass on `main` at 62fb36f9.
- Mutation check on `main`: making the coalescing in `CrdtRope::update` drop the later update instead of extending the span (`if mergeable && false`) makes both tests fail at once; so does an off-by-one in the merged span's end.
- Timings on `main` (release, Windows 11 x64): undo of one 1 MB insert 5 ms, 5 MB 27 ms; `import_batch` of 50 × 100 KB 12 ms. On 1.17.1 the same test printed 18.7 s for the 1 MB undo and 5.4 s for `import_batch` of 8 × 100 KB.

No changeset: tests and docs only.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
