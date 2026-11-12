# Synchronotes fork notes

This folder is not part of Loro. It holds the report on the changes this fork
makes on top of Loro 1.17.1 and the tools used to check them. It lives on its
own commit at the tip of `synchronotes/fix-quadratic-diff`; leave that commit
out of anything offered upstream.

- [REPORT.md](REPORT.md): cause, fix, evidence, risks, how to consume the fork.
- [differential/](differential/): a Cargo project outside the Loro workspace
  that links stock `loro` 1.17.1 from crates.io and the fork side by side.

## differential

Run from `synchronotes/differential`.

Compare stock and fork on random histories (several peers, pastes of many
kilobytes, deletes, backspace runs, marks, undo/redo, sync by relay blobs,
`import_batch`, snapshots, checkout, diff, fork, revert). Everything
observable is compared, including change events and exported bytes, and peers
on different libraries sync with each other:

```bash
cargo run --release --bin loro-fork-differential -- <first_seed> <seed_count> <peers, 0 = vary> <steps> [seconds]
```

Print one history instead of comparing (`DUMP_TRACE=1` with the same
arguments).

Loro's own fuzz harness on random inputs, with debug assertions on
(`cargo fuzz` needs nightly and libFuzzer, which this Windows machine lacks):

```bash
cargo run --profile checked --bin fuzz-driver -- <text|diff_calc|one_doc|all> <seconds> [threads] [first_seed]
```

Ordinary-case timings and peak heap, stock against fork in one process:

```bash
cargo run --release --bin bench -- [rounds] [scenario filter]
```
