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
| `categorize_roms/sample` | ~0.49 ms | ~40 machines in sample XML |
| `plan_export/sample` | ~0.94 ms | Same catalog, tiny ROM tree |
| `copy_roms/sample/sources=12` | ~10 ms | `copy_with_plan` timed; catalog/plan rebuilt in setup each iteration |

Sample rows measured on Apple Silicon macOS.

### Full metadata (0.244 listxml + CatVer via `ensure_metadata` or `CARGO_RC_BENCH_METADATA_DIR`)

| Step | Median time | Notes |
|------|-------------|--------|
| `categorize_roms/full` | ~2.4 s | ~45k machines; `--sample-size 10`, `--measurement-time 25` |
| `plan_export/full` | ~252 ms | Same env; catalog loaded once outside the timed loop; tiny ROM tree |
| Peak RSS (`categorize_roms/full`) | ~2.5 GiB | One `cargo bench --bench categorize_roms_bench -- --test` with `CARGO_RC_RUN_EXPENSIVE_TESTS=true`; read `maximum resident set size` from `/usr/bin/time -l` on macOS |

Full-metadata rows measured on Apple Silicon macOS, 2026-11-04 (`rustup show` / git revision at measure time). Run `copy_roms/full` locally when you need that row (small ROM tree; long setup per iteration).

### Profiling full categorize (local)

Roughly **most CPU time is `roxmltree::Document::parse`** on the full listxml string, plus **~2.5 GiB peak RSS** from holding the XML text, the DOM, and the in-memory `Catalog` (see `run()` in `src/lib.rs`: `read_to_string` → `read_mame_xml` → `parse` → `categorize`).

Reproduce on macOS without extra tools:

```bash
CARGO_RC_RUN_EXPENSIVE_TESTS=true cargo bench --locked --bench categorize_roms_bench -- --profile-time 10 categorize_roms/full &
sample categorize_roms_bench 12 -file target/categorize_full_sample.txt -mayDie
```

Open `target/categorize_full_sample.txt` and follow the `roms_curator::run` → `roxmltree` stack. Optional: `cargo install flamegraph` and `cargo flamegraph --bench categorize_roms_bench` with the same env and Criterion args.

**Next optimization (separate change):** stream or incrementally parse listxml into `RomData` without a full DOM, and/or **cache the parsed catalog** on disk keyed by metadata hashes so GUI and CLI do not reparsed on every plan preview. That is the main lever for large MAME drops and responsive UI; plan (~250 ms on full metadata with 12 ROM files) is secondary until the on-disk collection is huge.

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
