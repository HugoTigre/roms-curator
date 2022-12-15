extern crate core;

use std::{env, process};
use std::path::Path;
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;
use log::{error, LevelFilter};
use roms_curator::core::roms_service::RomsExt;
use roms_curator::models::config::Config;

fn main() {
    set_up_logging();

    let config = Config::new().build(env::args()).unwrap_or_else(|err| {
        error!("Problem parsing arguments: {err}");
        println!("Please run 'roms-curator.exe --help' for instructions.");
        process::exit(1);
    });

    if config == Config::default() { process::exit(0) }

    let roms = roms_curator::run_debug(&config).unwrap_or_else(|err| {
        error!("Application error: {err}");
        process::exit(1);
    });

    if !config.source_path.is_empty() || !config.destination_path.is_empty() {
        let report = roms.copy_roms(&config).unwrap_or_else(|err| {
            error!("Failed to copy roms: {err}");
            process::exit(1);
        });

        if !config.report_path.is_empty() {
            report.to_file(&config.report_path).unwrap_or_else(|err| {
                error!("Error creating report: {err}");
                println!("Note: The report failed but files should have been correctly copied to destination.");
                process::exit(0);
            });
        }
    };
}

fn set_up_logging() {
    if Path::new("logging.yaml").exists() {
        log4rs::init_file("logging.yaml", Default::default()).unwrap();
    } else {
        let stdout_appender = ConsoleAppender::builder()
            .encoder(Box::new(PatternEncoder::new("{h({d(%Y-%m-%d %H:%M:%S)(utc)} - {l}: {m}{n})}")))
            .build();

        let config = log4rs::Config::builder()
            .appender(Appender::builder().build("stdout", Box::new(stdout_appender)))
            .build(Root::builder().appender("stdout").build(LevelFilter::Warn))
            .unwrap();

        log4rs::init_config(config).unwrap();
    }
}
