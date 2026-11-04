# Performance measurements (PERF-01 / PERF-02)

Baseline timings and memory for the export pipeline. Re-run these when changing
parsing, selection, planning, or copying.

**PERF-02 (streaming listxml):** `run()` uses `quick-xml` to read one `<machine>` at
a time (`src/core/listxml.rs`); there is no full-file `read_to_string` or
`roxmltree` DOM. The in-memory `Catalog` is unchanged.

## What we measure

| Benchmark | Command target | What it includes |
|-----------|----------------|----------------|
| `categorize_roms/{sample\|full}` | `run()` | Read CatVer, parse listxml, build `Catalog` (selection, dependencies, disks). |
| `plan_export/{sample\|full}` | `Catalog::plan` | Copy plan for the sample ROM tree or full metadata; catalog loaded once outside the timed loop. |
| `copy_roms/{sample\|full}/sources=N` | `Catalog::copy_with_plan` | One temp destination per iteration (`BatchSize::PerIteration`); setup rebuilds catalog and plan outside the timed section (with **full** metadata, setup runs `run()` every iteration, so total wall time is much larger than the timed copy). Fixture sources may stay OS-cache-warm across iterations. |

Progress bars are disabled in benchmarks (`progress: false`).

### Metadata fixtures

- **sample** (default): `tests/resources/listxml_sample.xml` and `catver_sample.ini` with `tests/resources/merged_roms/` (12 files). Fast; suitable for CI compile checks and regression timing on a laptop.
- **full**: set `CARGO_RC_RUN_EXPENSIVE_TESTS=true`. Benchmarks call `ensure_metadata` once into `target/benches/metadata-cache/` (same pattern as integration tests; needs network on first run). Override the version with `CARGO_RC_MAME_VERSION` (default `0.244`). Point at an existing tree with `CARGO_RC_BENCH_METADATA_DIR` (`catver.ini` plus `mame-roms.xml` or `mame*.xml`). Needs several GB RAM for categorize/plan on full listxml.

Copy/plan benchmarks always use the **sample** ROM tree; only listxml/CatVer size changes between sample and full. With **full** metadata, the **copy** bench still parses the full listxml on every iteration in setup (only the timed slice is `copy_with_plan`); expect long total runtime if you run many samples.

## Run Criterion benchmarks

From the repository root:

```bash
cargo bench --locked
```

That runs all three bench binaries in sequence. For **full** metadata, the shared
cache under `target/benches/metadata-cache/` is populated once per process;
you can still run one crate at a time to save time:

```bash
cargo bench --locked --bench categorize_roms_bench
cargo bench --locked --bench plan_roms_bench
cargo bench --locked --bench copy_roms_bench
```

HTML reports are written under `target/criterion/`.

Full-database categorize (local only; downloads metadata on first run unless `CARGO_RC_BENCH_METADATA_DIR` is set):

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
| `categorize_roms/sample` | ~0.64 ms | ~40 machines; streaming parser adds a little overhead vs the old DOM path (~0.49 ms pre–PERF-02) |
| `plan_export/sample` | ~0.94 ms | Same catalog, tiny ROM tree |
| `copy_roms/sample/sources=12` | ~10 ms | `copy_with_plan` timed; catalog/plan rebuilt in setup each iteration |

Sample rows measured on Apple Silicon macOS.

### Full metadata (0.244 listxml + CatVer via `ensure_metadata` or `CARGO_RC_BENCH_METADATA_DIR`)

| Step | Median time | Notes |
|------|-------------|--------|
| `categorize_roms/full` | ~1.35 s | ~45k machines; `--sample-size 10`, `--measurement-time 25`; ~45% faster than pre–PERF-02 (~2.4 s) on same hardware |
| `plan_export/full` | ~252 ms | Pre–PERF-02 row; re-run after metadata cache is warm |
| Peak RSS (`categorize_roms/full`) | TBD | Re-run `/usr/bin/time -l` on `--test` with full metadata; expect lower than pre–PERF-02 (~2.5 GiB) now that listxml is streamed |

Full-metadata categorize re-measured on Apple Silicon macOS, 2026-11-04, with
`CARGO_RC_BENCH_METADATA_DIR` pointing at extracted 0.244 listxml + CatVer.
Re-record plan/full RSS after you refresh local numbers.

### Profiling full categorize (PERF-02)

CPU time moves into **`listxml` machine parsing** and **`categorize_machines` /
dependency resolution**, not a third-party DOM build. Profile with Criterion
`--profile-time` and macOS `sample` as before; follow `roms_curator::run` →
`parse_mame_xml_file` → `listxml::parse`.

**Next optimization (separate change):** on-disk **catalog cache** keyed by
metadata hashes (plans already hash listxml/CatVer) so GUI/CLI replan without a
full reparse; optional lazy fields for machines outside the selection closure.

## How this guides optimization

- **PERF-02** removed the listxml DOM bottleneck; further wins are catalog caching,
  lazy machine records, or parallelizing independent post-parse work—not another
  XML library swap on the same all-machines-in-memory model.
- If **plan** grows with collection size faster than copy, reduce plan work or
  cache hashes incrementally.
- If **copy** dominates with a real merged collection on disk, consider
  concurrency or copy-file range I/O; the sample benchmark stays useful as a
  micro-regression guard.

Do not change selection results or plan semantics for speed without tests and a
before/after row in this document.
