# Upstream preparation (loro-dev/loro)

Written 2026-11-12. Nothing here has been pushed or posted. All branches are
local in the clone `D:\code\projects\loro`; `origin` is the fork
(jibbers42/loro), `upstream` is loro-dev/loro.

## What changed upstream while we worked

`upstream/main` moved from 6ff0efd1 to 62fb36f9 (15 commits). One of them
merged PR #1082, the coalescing fix this fork had cherry-picked; #1236 was
already there. So both large-text fixes are upstream now, and neither needs
a pull request or a comment from us. Undo, diff, checkout and `import_batch`
of large text are linear on `main` (measured: undo of a 5 MB insert 27 ms).

What `main` still lacks is below.

## Branches, one pull request each, independent (no stacking)

Each is one commit on `upstream/main` (62fb36f9). They touch different
files (the two that edit `AGENTS.md` do so in different places) and merge in
any order. They were pushed to the fork on 2026-11-12 and opened as pull
requests the same day, each describing its own problem (no separate issue):

- https://github.com/loro-dev/loro/pull/1282 (`perf/commit-export-cached-blocks`)
- https://github.com/loro-dev/loro/pull/1283 (`fix/wasm-build-on-windows`)
- https://github.com/loro-dev/loro/pull/1284 (`test/large-text-fragments`)

The numbered draft files below are the working drafts; the text actually
posted is shorter and is on the pull requests.

| Branch | Commit | Pull request | Changeset |
|---|---|---|---|
| `perf/commit-export-cached-blocks` | `e3e54166` | [2-pr-cached-change-blocks.md](2-pr-cached-change-blocks.md), after the issue [1-issue-commit-after-snapshot.md](1-issue-commit-after-snapshot.md) | `.changeset/commit-export-cached-blocks.md`, `loro-crdt` patch |
| `fix/wasm-build-on-windows` | `4b3c9337` | [3-pr-windows-wasm-build.md](3-pr-windows-wasm-build.md) | none (published package unchanged) |
| `test/large-text-fragments` | `ff70e9f2` | [4-pr-large-text-tests.md](4-pr-large-text-tests.md) | none (tests and docs) |

Why not stacked: stacking helps when a later change needs an earlier one.
None of these does, and independent pull requests can be reviewed and merged
separately. The tests PR is optional. Its default test takes about 3 s in a
debug build; the existing suite spends about 395 s executing tests on this
machine, with single files at 25 to 83 s.

## Order of actions when the owner says go

1. Push the three branches to `origin` (the fork).
2. Open the issue from draft 1, then the pull request from draft 2 with
   `Fixes #<issue>`.
3. Open the pull requests from drafts 3 and 4.
4. If `upstream/main` has moved again by then: fetch, rebase each branch
   (one commit each), rerun `cargo test -p loro-internal --lib change_store`
   and `cargo test -p loro --test perf_commit_after_snapshot --test
   large_text_time_travel`.

## Not done

- Linux and macOS runs; coverage-guided fuzzing and the saved fuzz corpus;
  the Bun tests of the WASM package.
- `cargo fmt --all` and `clippy -D warnings` are not clean on `main` itself
  with rustc 1.99; only our own lines were checked.
- `tests/oom.test.ts` still fails on Windows (it imports a `D:\` path as a
  module specifier). Not fixed; it is a fourth, tiny candidate.

## The Synchronotes branch

`synchronotes/fix-quadratic-diff` (1.17.1 plus the fixes) is unchanged by
this work and is still what the app would consume today. It does not have
the review loop's additions (a unit test, a clarified note, and
`#[inline(never)]` on `caches_range`); those are test, doc and codegen-hint
changes only.
