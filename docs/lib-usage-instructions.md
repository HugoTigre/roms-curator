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
1. Create a [config](##config-/-arguments) struct.
1. [Categorize ROMs](##categorize-roms)
   - At this point is we can go through the collection and
      include/exclude the ROMs that you want based on their categorization.
   - `rom.category` tells us if the ROM is working or not or if it is a bios
     or system ROM.
   - `rom.data.category` tells us the game type category, so if we wanted to
     exclude casino games, we could check if `rom.data.category` contains `Slot Machine`,
     among others. To know all possible categories the best way to check
     [catver.ini](./../README.md) file.
1. [Copy the ROMs](##copy-roms) to a directory.
   - If, in the previous step, we created a new collection with only the ROMs
     categories that we want, then only those will be copied.

## Config / Arguments

Manually build the `Config` struct:

Note that to categorize ROMs we just need `mame-xml-path` and `catver-path` properties.

```rust
let config = Config {
    mame_xml_path: "some-valid-path.xml".to_string(),
    catver_path: "some-valid-path.ini".to_string(),
    // we can specify several source paths, typical use case when we have ROMs in one directory and CHDs in another.
    source_path: vec![
        "some-valid-rom-source-directory".to_string(),
        "another-valid-chd-source-directory".to_string(),
    ],
    destination_path: "where-we-want-to-move-roms-to-directory".to_string(),
    report_path: "report.md".to_string(),
    // if true it will note copy not working CHDs
    ignore_not_working_chd: false,
    // if we just want to simulate the process of copying ROMs
    simulate: false,
    // if we want to exclude ROMs, only applies to copy ROMs operation
    subset_start: String::new(),
    // if we want to exclude ROMs, only applies to copy ROMs operation
    subset_end: String::new(),
};
```

Build the `Config` from input arguments:

Note that this method validates all properties, so we need to provide
all mandatory arguments to achieve the entire process, categorize ROMs
and copy them to a destination directory, the following are all mandatory:
`mame-xml-path`, `catver-path`, `source-path` and `destination-path`.
Of course we can always use dummy data.

Also, if we just want to simulate the process, `report-path` is also mandatory.

```rust
let config = Config::new().build(env::args()).unwrap_or_else(|err| {
    error!("Problem parsing arguments: {err}");
    process::exit(1);
});
```

## Categorize ROMs

This will return a collection `HashMap<String, Rom>` with all the ROMs
categorized, we can inspect [Rom](/roms-curator/src/models/roms.rs)
struct to know more.

For this, `Config` just needs `mame_xml_path` and `catver_path` paths.

```rust
let roms = roms_curator::run(&config).unwrap_or_else(|err| {
    error!("Application error: {err}");
    process::exit(1);
});
```

## Copy ROMs

To copy ROMs `Config` needs all the mandatory properties,
see [here](##config-/-arguments).

Invoking `copy_roms` will return a `Report` struct, this can be
used to create a report file in markdown format.

```rust
let report = roms.copy_roms(&config).unwrap_or_else(|err| {
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
