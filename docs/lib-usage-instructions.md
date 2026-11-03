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

Build the `Args` struct:

```rust
use roms_curator::core::args::build_args;

let args = build_args().unwrap_or_else(|err| {
    error!("Application error: {err}");
    process::exit(1);
});
```

Arguments are build from command line input arguments, see [here](app-run-instructions.md)
for more details on that including minimum required arguments. We can also manually build
the `Args` struct. This skips CLI value parsing; `copy_roms` still validates
collection paths and destination conflicts. Parsing arguments performs no writes.

## Categorize ROMs

`run` returns a [Catalog](/src/models/roms.rs). `catalog.roms` is the
`HashMap<String, Rom>`. `catalog.mame_build` is the XML `build` attribute,
stored as written. `catalog.unknown_categories` lists machines with no CatVer
entry; those machines use the category `Unknown`. A disk with no `status`
attribute is `ChdStatus::Good`.

Selection uses the playable policy: driver status and emulation are each good
or imperfect. Imperfect or unemulated sound and graphics stay selected.
Preliminary machines, mechanical machines, BIOS, devices, and non-arcade CatVer
groups do not. `ParsedMachines::categorize_with(EmulationPolicy::Strict)` keeps
only machines whose driver and emulation are both good.

For this, `Args` just needs `mame_xml_path` and `catver_path` paths.

```rust
let catalog = roms_curator::run(&args).unwrap_or_else(|err| {
    error!("Application error: {err}");
    process::exit(1);
});
```

## Copy ROMs

To copy ROMs `Args` needs all the mandatory properties,
see [here](#config--arguments).

Invoking `copy_roms` will return a `Report` struct, this can be
used to create a report file in Markdown format.

Before creating category directories or copying ROMs, `copy_roms` rejects
duplicate destination names (including case-only differences), existing selected
files or CHD directories, and reports inside category directories. Use a fresh
output folder: files are not overwritten and CHD folders are not merged.
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

With `args.simulation = true`, `copy_roms` returns a simulation report without
creating output directories or copying files. Entries use `OperationStatus::Planned`;
normal outcomes are `Copied`, `Failed` or `Ignored`. The old `moved` boolean has
been replaced by `ReportDetailEntry.status`. `report.simulation` identifies the
mode, and copied/failed counts exclude Planned entries. Simulation does not test
write access or available space. Only write a report if requested, outside both
source/output trees; a CLI report path is optional.

The lower-level `utils::copy_dir_recursive` requires a destination path that does
not exist yet, with an existing parent directory. It does not merge into an
existing directory. `RomsExt::copy_rom` returns `io::Result<()>` with copy error
context rather than a success boolean.

```rust
let report = catalog.copy_roms(&args).unwrap_or_else(|err| {
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
