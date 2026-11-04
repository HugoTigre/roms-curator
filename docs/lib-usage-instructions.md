# Library Usage Instructions

__Index__

- [Workflow](#workflow)
- [Config / Arguments](#config--arguments)
- [Categorize ROMs](#categorize-roms)
- [Copy ROMs](#copy-roms)
- [Generating a report](#generating-a-report)
- [Logging](#logging)

## Workflow

The workflow can be different depending on what one wants to achieve,
but as an example, let's say we want to categorize a MAME ROM Set,
so that we can exclude from our collection a certain type of category.

1. Make sure you have the lib required external dependencies.
   See [readme.md](../README.md) for information on how to get this.
1. Create a [config](#config--arguments) struct.
1. [Categorize ROMs](#categorize-roms)
   - At this point is we can go through the collection and
      include/exclude the ROMs that you want based on their categorization.
   - `rom.category` tells us if the ROM is working or not or if it is a bios
     or system ROM. Working means the driver status and emulation are good or
     imperfect. The library does not drop a genre such as slots or shooters
     for being that genre.
   - `rom.data.category` is the CatVer category. A caller can still filter it.
     For example, excluding categories that contain `Slot Machine` keeps those
     machines out of a copied subset. The category names come from
     [catver.ini](./../README.md).
1. [Copy the ROMs](#copy-roms) to a directory.
   - If, in the previous step, we created a new collection with only the ROMs
     categories that we want, then only those will be copied.

## Config / Arguments

Library callers build [`ExportConfig`](../../src/core/config.rs) and call
[`ExportConfig::validate`](../../src/core/config.rs) before planning or copying.
The CLI uses the same struct via [`build_args`](../../src/core/args.rs).

```rust
use roms_curator::ExportConfig;

let mut config = ExportConfig {
    mame_xml_path: "mame.xml".into(),
    catver_path: "catver.ini".into(),
    source_path: vec!["/roms".into()],
    destination_path: "/export".into(),
    ..ExportConfig::default()
};
config.validate().unwrap();
```

For a command-line program, use `roms_curator::core::args::build_args()` instead.
That parses flags and runs the same validation. `Args` is a type alias for
`ExportConfig`.

## Categorize ROMs

`run` returns a [Catalog](/src/models/roms.rs). `catalog.roms` is the
`HashMap<String, Rom>`. `catalog.mame_build` is the XML `build` attribute,
stored as written. `catalog.unknown_categories` lists machines with no CatVer
entry; those machines use the category `Unknown`. A disk with no `status`
attribute is `ChdStatus::Good`.

Selection uses the playable policy: driver status and emulation are each good
or imperfect. Imperfect or unemulated sound and graphics stay selected.
Preliminary machines, mechanical machines, BIOS, devices, and non-arcade CatVer
groups do not. `catalog.selected` is the visible game list, sorted by name.
BIOS, devices, and other unselected machines remain in `catalog.roms`. A clone
stays in `catalog.selected` when its parent is not. Each machine keeps its
`cloneof` parent, `romof` parent, device references, ROM dumps, and disks.
`catalog.dependencies` is the merged-archive list for that visible list. A clone
requires its parent archive, not its own name. BIOS and other `romof` parents
are separate archives. A device archive is included when that device, or a
`cloneof` / `romof` parent of it, has ROM or disk entries. Names that are not
in the catalog, and dependency cycles, are listed on `missing` and `cycles`.
Dump quality does not change `rom.category`. `catalog.disks` lists the CHD
files that visible list needs. A `merge` or a matching sha1 keeps one ancestor
copy. A clone disk with different content also names the parent's same-named
non-optional disk a delta CHD needs. A differently named or optional parent
disk is missing when that parent has disks. A parent with no disks is not missing.
Optional disks are left out. Unresolved merges, delta parents, and
devices are `catalog.disks.missing` and do not change `rom.category`.
`Catalog.policy` is the emulation policy that built the visible list. `Catalog::plan` builds a version-2 JSON plan for the same arguments. It names each selected game and the emulation-policy reason, and each file placed in `roms` or `rest`, with its path, size, and whether the source file is present. Missing references and destination conflicts are on the plan. `plan.write` saves a new file and does not replace an existing one. `Plan::read` loads a version-2 plan. `Plan::recheck` compares the recorded metadata hashes, sizes, and conflicts with the disk. `Catalog::copy_with_plan` runs that recheck on the plan you pass in. `Catalog::copy_roms` builds one plan and rechecks that value.
`Catalog::copy_roms` copies required archives and CHD directories into `roms`, and every other source file this run keeps into `rest`. It writes `selected.txt` beside those folders when every copy succeeds. A failure to publish that file stays on the returned report (`names_error`, `all_ok` false) so a requested report is still written. The report path must not be that file. When a selected game needs a CHD that is not in the source, the report lists it under **Missing CHDs** with the game, machine folder, disk name, and reason. A name-range subset skips stems outside the range, then still copies required archives and disk directories of selected games inside the range. `--ignore-not-working-chd` does not move a required disk out of `roms`. A missing CHD leaves the game selected and its ROM archive copied.
`ParsedMachines::categorize_with(EmulationPolicy::Strict)` narrows
`catalog.selected` to machines whose driver and emulation are both good and leaves
the rest in `catalog.roms`.

For this, `ExportConfig` just needs `mame_xml_path` and `catver_path` paths.

```rust
let catalog = roms_curator::run(&config).unwrap_or_else(|err| {
    error!("Application error: {err}");
    process::exit(1);
});
```

## Copy ROMs

To copy ROMs, `ExportConfig` needs all mandatory fields; see [here](#config--arguments).

Invoking `copy_roms` will return a `Report` struct, this can be
used to create a report file in Markdown format.

Before creating `roms` and `rest` or copying ROMs, `copy_roms` rejects
duplicate destination names (including case-only differences), existing selected
files or CHD directories, an existing `selected.txt`, and reports inside those
directories. Use a fresh output folder: files are not overwritten and CHD folders are not merged.
Files completed before a later copy failure remain; the export has no whole-run
rollback or power-loss durability guarantee.

Preflight/setup errors return `Err`. Individual copy failures are recorded in the
returned report: check `report.all_ok` and the failed entries' `error` fields for
source/destination paths and causes. The CLI writes the requested report before
returning a nonzero status for copy failures. A requested report failure also
returns a nonzero status; completed ROM files remain available.
If saving fails, the CLI prints the full report to stderr and explains that
completed ROMs were kept and will not be replaced on a rerun. Report files are
published only after a successful flushed write, so failed writes leave no
partial report at the requested path. Existing reports are never replaced.

With `config.simulation = true`, `copy_roms` returns a simulation report without
creating output directories or copying files. Entries use `OperationStatus::Planned`;
normal outcomes are `Copied`, `Failed` or `Ignored`. Each entry also has
`ReportDetailEntry.media_kind` (`Archive`, `DiskDirectory`, or `OtherFile`).
The summary lists selected games separately from archives and disk directories in
each export folder. `report.simulation` identifies the mode, and copied/failed
counts exclude Planned entries. Simulation does not test write access or available
space. Only write a report if requested, outside both source/output trees; a CLI
report path is optional.

The lower-level `utils::copy_dir_recursive` requires a destination path that does
not exist yet, with an existing parent directory. It does not merge into an
existing directory. Copy failures surface as `OperationStatus::Failed` with paths
and causes on the report entry.

`copy_all_to_rest` copies every source entry into `rest` without dependency
resolution or `selected.txt`. Prefer `Catalog::copy_roms` for a curated export.

```rust
let report = catalog.copy_roms(&config).unwrap_or_else(|err| {
    error!("Failed to copy roms: {err}");
    process::exit(1);
});
```

## Generating a report

```rust
let report_path = "report.md"; // where you want to save the report

report.to_file(&report_path).unwrap_or_else(|err| {
    error!("Error creating report: {err}");
    process::exit(1);
});

if !report.all_ok {
    process::exit(1);
}
```

## Logging

The library uses [log](https://docs.rs/log/latest/log/) logging facade,
so we can implement the logging framework that you want on top of that,
so let's use [log4rs](https://docs.rs/log4rs/latest/log4rs/) as an example.
We can configure it in a `logging.yml` file or by code:

```rust
fn set_up_logging() {
    if Path::new("logging.yaml").exists() {
        log4rs::init_file("logging.yaml", Default::default()).unwrap();
    } else {
        let stdout_appender = ConsoleAppender::builder()
            .encoder(Box::new(PatternEncoder::new("{h({d(%Y-%m-%d %H:%M:%S)(utc)} - {l}: {m}{n})}")))
            .build();

        let config = log4rs::Config::builder()
            .appender(Appender::builder().build("stdout", Box::new(stdout_appender)))
            .build(Root::builder().appender("stdout").build(LevelFilter::Info))
            .unwrap();

        log4rs::init_config(config).unwrap();
    }
}
```
