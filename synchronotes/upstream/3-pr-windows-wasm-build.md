# PR draft: branch `pr/windows-wasm-build` → loro-dev/loro `main`

**Title:** build: let the wasm package build on Windows

## Summary

`pnpm -C crates/loro-wasm build-release` fails on Windows in two places. Neither change affects the package built on Linux or macOS.

- `loro-wasm-tools sourcemap`: `wasm2map` 0.1 joins the source paths into the JSON without escaping them, so Windows paths (`crates\loro-internal\src\...`) make the map invalid JSON and `embed-wasm-sourcemap.mjs` stops with `Bad escaped character in JSON`. The map is now written with forward slashes. (`map_v3` emits only `sources`, `sourcesContent: null` and VLQ `mappings`, so a backslash can only come from a path.)
- `scripts/post-rollup.ts`: the path checks split on `/` while `walk` yields `\` on Windows (`bundler\index.j/loro_wasm_bg.js` not found), and `new Deno.Command("rollup")` cannot start the `.cmd` shim. Paths are normalised before the checks, and on Windows rollup is started through `cmd /c`.

## Validation

- Windows 11 x64, rustc 1.99.1, Deno 2.9.6, pnpm 9.15.9, wasm-bindgen 0.2.100: `deno run -A ./scripts/build.ts release && rollup -c && deno run -A ./scripts/post-rollup.ts` now completes and produces all five targets with the same file lists and `index.js` sizes as the published 1.17.1 package.
- On `main` at 62fb36f9 with this change: all five targets build; vitest in `crates/loro-wasm` gives 562 passed, 1 failed (`oom.test.ts`, see below); the package's Deno tests pass (4); `deno check scripts/post-rollup.ts` passes.
- Not run: the same build on Linux or macOS (the changed lines are behind a backslash replacement that is a no-op there and an `os === "windows"` branch).

No changeset: the published package does not change.

Not included: `tests/oom.test.ts` still fails on Windows because it imports an absolute `D:\...` path as a module specifier in a child process.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
