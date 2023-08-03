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
     or system ROM.
   - `rom.data.category` tells us the game type category, so if we wanted to
     exclude casino games, we could check if `rom.data.category` contains `Slot Machine`,
     among others. To know all possible categories the best way to check
     [catver.ini](./../README.md) file.
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
the `Args` struct, but this will bypass all validations, so it is not recommended.

## Categorize ROMs

This will return a collection `HashMap<String, Rom>` with all the ROMs
categorized, we can inspect [Rom](/src/models/roms.rs)
struct to know more.

For this, `Args` just needs `mame_xml_path` and `catver_path` paths.

```rust
use roms_curator::core::roms_service::RomsExt;

let roms = roms_curator::run(&args).unwrap_or_else(|err| {
    error!("Application error: {err}");
    process::exit(1);
});
```

## Copy ROMs

To copy ROMs `Args` needs all the mandatory properties,
see [here](#config--arguments).

Invoking `copy_roms` will return a `Report` struct, this can be
used to create a report file in Markdown format.

```rust
let report = roms.copy_roms(&args).unwrap_or_else(|err| {
    error!("Failed to copy roms: {err}");
    process::exit(1);
});
```

## Generating a report

```rust
let report_path = "report.md" // where you want to save the report

report.to_file(&report_path).unwrap_or_else(|err| {
    error!("Error creating report: {err}");
    process::exit(0);
});
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
