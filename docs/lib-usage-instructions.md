# Library usage

## Quick start

```rust
use roms_curator::ExportConfig;

let mut config = ExportConfig {
    mame_xml_path: "mame.xml".into(),
    catver_path: "catver.ini".into(),
    source_path: vec!["/roms".into()],
    destination_path: "/export".into(),
    ..ExportConfig::default()
};
config.validate()?;

let catalog = roms_curator::run(&config)?;
let report = catalog.copy_roms(&config)?;
```

CLI programs can use `roms_curator::core::args::build_args()` (`Args` is a type alias for `ExportConfig`).

Metadata download:

```rust
use roms_curator::{ensure_metadata, default_metadata_cache_dir, ExportConfig};

let paths = ensure_metadata("0.289", &default_metadata_cache_dir(), false)?;
```

## Catalog

`run` returns a **`Catalog`**: `catalog.roms` (all machines), `catalog.selected` (policy-selected arcade games), `catalog.dependencies` (merged archives required), `catalog.disks` (required CHDs), `catalog.policy`, `catalog.mame_build`, `catalog.unknown_categories`.

Selection defaults to **playable** (good/imperfect driver and emulation). Set `config.strict_emulation = true` or use `ParsedMachines::categorize_with(EmulationPolicy::Strict)` when building from parsed XML yourself.

Listxml is streamed (no full DOM). Lower level: `parse_mame_xml_file` / `parse_mame_xml_from_str`, then `ParsedMachines::categorize()`.

## Plan and copy

Call `config.validate()` before `Catalog::plan`, `plan_with_progress`, `copy_roms`, or `copy_with_plan`.

- **`Catalog::plan`** — version-2 JSON: games, `roms`/`rest` media, missing refs, conflicts, metadata hashes.
- **`Plan::read` / `Plan::recheck`** — reuse a saved plan; recheck before copy.
- **`Catalog::copy_roms`** — validate, plan, recheck, copy to `roms`/`rest`, write `selected.txt` when every copy succeeds.
- **`copy_all_to_rest`** — copy every source entry to `rest` only (no dependency curation).

Launch-ready names (default): `require_complete_dependencies` is true unless you pass CLI **`--include-incomplete-games=true`** (or set `require_complete_dependencies: false` on `ExportConfig`). Copy routing uses `export_game_set` for `roms` vs `rest`; held-back games are not in that set. Helpers: `core::launch_ready::export_game_names`, `held_back_games`.

Simulation: `config.simulation = true` — no output dirs, `OperationStatus::Planned` in the report.

Safety: fresh destination required (no overwrite, no CHD merge). Completed files stay after a later failure; see report `all_ok`, per-entry errors, and `names_error` if `selected.txt` could not be written.

## Report

```rust
report.to_file("report.md")?;
if !report.all_ok {
    std::process::exit(1);
}
```

## Logging

The crate uses the [`log`](https://docs.rs/log) facade; wire any backend (e.g. [`log4rs`](https://docs.rs/log4rs)).

More CLI flags and paths: [app-run-instructions.md](app-run-instructions.md).
