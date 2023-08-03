extern crate core;

use std::process;

use log::{error, info};

use roms_curator::core::args::build_args;
use roms_curator::core::roms_service::RomsExt;
use roms_curator::utils::set_up_logging;

fn main() {
    set_up_logging();

    let args = build_args().unwrap_or_else(|err| {
        error!("Application error: {err}");
        process::exit(1);
    });

    info!("Starting roms_curator with arguments: {:?}", args);

    let roms = roms_curator::run(&args).unwrap_or_else(|err| {
        error!("Application error: {err}");
        process::exit(1);
    });

    if !args.source_path.is_empty() || !args.destination_path.is_empty() {
        let report = roms.copy_roms(&args).unwrap_or_else(|err| {
            error!("Failed to copy roms: {err}");
            process::exit(1);
        });

        if !args.report_path.is_empty() {
            report.to_file(&args.report_path).unwrap_or_else(|err| {
                error!("Error creating report: {err}");
                println!("Note: The report failed but files should have been correctly copied to destination.");
                process::exit(0);
            });
        }
    };

}
