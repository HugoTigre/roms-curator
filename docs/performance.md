# Performance measurements (PERF-01)

Baseline timings and memory for the current export pipeline, before any streaming
or concurrency work. Re-run these when changing parsing, selection, planning, or
copying.

**PERF-01 scope:** Criterion targets for categorize, plan, and copy; sample
baselines below; full-database **categorize** recorded locally. Full plan/copy
rows and peak RSS are optional follow-ups in the tables, not blockers for the RFC.

## What we measure

| Benchmark | Command target | What it includes |
|-----------|----------------|----------------|
| `categorize_roms/{sample\|full}` | `run()` | Read CatVer, parse listxml, build `Catalog` (selection, dependencies, disks). |
| `plan_export/{sample\|full}` | `Catalog::plan` | Copy plan for the sample ROM tree or full metadata; catalog loaded once outside the timed loop. |
| `copy_roms/{sample\|full}/sources=N` | `Catalog::copy_with_plan` | One temp destination per iteration (`BatchSize::PerIteration`); setup rebuilds catalog and plan outside the timed section (with **full** metadata, setup runs `run()` every iteration, so total wall time is much larger than the timed copy). Fixture sources may stay OS-cache-warm across iterations. |

Progress bars are disabled in benchmarks (`progress: false`).

### Metadata fixtures

- **sample** (default): `tests/resources/listxml_0244.xml` and `catver_0244.ini` with `tests/resources/merged_roms/` (12 files). Fast; suitable for CI compile checks and regression timing on a laptop.
- **full**: set `CARGO_RC_RUN_EXPENSIVE_TESTS=true` and provide `tests/resources/prod_lists_0244.zip` (full 0.244 listxml + CatVer). The zip is extracted once into `target/benches/_shared_metadata/` (skipped when files already exist), so running all three bench crates in one `cargo bench` does not triple disk use. After replacing the zip, delete that directory (or your `CARGO_RC_BENCH_METADATA_DIR` tree) so the next run re-extracts; otherwise baselines keep the old files. Point at an existing tree with `CARGO_RC_BENCH_METADATA_DIR` (for example a future CLI metadata cache). Needs several GB RAM for categorize/plan on full listxml.

Copy/plan benchmarks always use the **sample** ROM tree; only listxml/CatVer size changes between sample and full. With **full** metadata, the **copy** bench still parses the full listxml on every iteration in setup (only the timed slice is `copy_with_plan`); expect long total runtime if you run many samples.

## Run Criterion benchmarks

From the repository root:

```bash
cargo bench --locked
```

That runs all three bench binaries in sequence. For **full** metadata, the shared
extract under `target/benches/_shared_metadata/` is reused after the first crate;
you can still run one crate at a time to save time:

```bash
cargo bench --locked --bench categorize_roms_bench
cargo bench --locked --bench plan_roms_bench
cargo bench --locked --bench copy_roms_bench
```

HTML reports are written under `target/criterion/`.

Full-database categorize (local only, after placing `prod_lists_0244.zip`):

```bash
CARGO_RC_RUN_EXPENSIVE_TESTS=true cargo bench --locked --bench categorize_roms_bench -- --sample-size 10 --measurement-time 25
```

Full-database plan (same env; planning against the full catalog can take seconds per iteration):

```bash
CARGO_RC_RUN_EXPENSIVE_TESTS=true cargo bench --locked --bench plan_roms_bench -- --sample-size 10 --measurement-time 25
```

Each full iteration is on the order of **seconds** for categorize and often for
plan. If Criterion warns that it cannot finish 10 samples in 5s, raise
`--measurement-time` (as above) or lower `--sample-size` (for example `5`).

Arguments after `--` go to the Criterion binary (`--sample-size`, `--measurement-time`, `--test`, etc.), not to `cargo bench`. Use `--bench categorize_roms_bench` so Cargo does not run the library test harness (which does not understand those flags).

## Measure peak memory

Criterion times wall clock only. For peak RSS, wrap one bench executable with the
system `time` utility (outside Criterion’s loop):

```bash
cargo bench --locked --bench categorize_roms_bench -- --test
/usr/bin/time -l target/release/deps/categorize_roms_bench-* 2>&1 | tail -5
```

On Linux, use `/usr/bin/time -v` and read `Maximum resident set size`.

Record hardware, OS, Rust toolchain (`rustup show`), metadata mode (sample/full),
and the git revision with each table update.

## Baseline (bench profile)

Recorded with `profile.bench` and `debug = true` in `Cargo.toml`. Times are
Criterion medians unless noted. Re-run on your machine before comparing optimizations.

### Sample metadata (small listxml + fixture ROMs)

| Step | Median time | Notes |
|------|-------------|--------|
| `categorize_roms/sample` | ~0.49 ms | ~40 machines in sample XML |
| `plan_export/sample` | ~0.94 ms | Same catalog, tiny ROM tree |
| `copy_roms/sample/sources=12` | ~10 ms | `copy_with_plan` timed; catalog/plan rebuilt in setup each iteration |

Sample rows measured on Apple Silicon macOS.

### Full metadata (0.244 listxml + CatVer via `prod_lists_0244.zip`)

| Step | Median time | Notes |
|------|-------------|--------|
| `categorize_roms/full` | ~2.3 s | ~45k machines; local run 2026-11-04, `--sample-size 10` (Criterion suggested ~22s measurement window) |

Add peak RSS from `/usr/bin/time -l` on a single `--test` run when you record full
numbers. Run `plan_export/full` and `copy_roms/full` the same way when you need
end-to-end full-metadata timings (copy still uses the small ROM tree).

## How this guides optimization

- If **categorize** dominates on full metadata, listxml parsing or in-memory
  structures are the first place to look (streaming SAX, lazy machine records).
- If **plan** grows with collection size faster than copy, reduce plan work or
  cache hashes incrementally.
- If **copy** dominates with a real merged collection on disk, consider
  concurrency or copy-file range I/O; the sample benchmark stays useful as a
  micro-regression guard.

Do not change selection results or plan semantics for speed without tests and a
before/after row in this document.
