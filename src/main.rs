extern crate core;

use std::{env, process};
use log::error;
use roms_curator::core::roms_service::RomsExt;
use roms_curator::models::config::Config;

fn main() {
    log4rs::init_file("logging.yaml", Default::default()).unwrap();

    let config = Config::new().build(env::args()).unwrap_or_else(|err| {
        error!("Problem parsing arguments: {err}");
        process::exit(1);
    });

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
