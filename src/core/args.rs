extern crate core;

use std::{env, fs};
use std::error::Error;
use std::fs::{File, metadata};
use std::path::Path;

use clap::{Arg, Command, crate_authors, crate_description, crate_name, crate_version};

/// Stores startup program arguments
///
/// ## Arguments
/// - mamexml_path: Path to MAME ROM database file. See README on how to get this.
/// - catver_path: Path to MAME support file. See README on how to get this.
/// - source_path: Where the original ROM collection is. Can be more than one directory.
/// - destination_path: Where to copy the roms.
/// - report_path: Path to the generated report in markdown format. Ex: report.md.
/// - ignore_not_working_chd: If true, not-working CHD ROMs and Directories will not be copied.
/// - simulation: If true, no ROMs will be copied, but the report will still be generated as if they were (Needs valid `report_path`).
/// - subset_start: If set, only roms which ascii name alphabetical order is higher than this will be copied.
/// - subset_end: If set, only roms which ascii name alphabetical order is lower than this will be copied.
///
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Args {
    /// Path to MAME ROM database file. See README on how to get this.
    pub mame_xml_path: String,
    /// Path to MAME support file. See README on how to get this.
    pub catver_path: String,
    /// Where the original ROM collection is. Can be more than one directory.
    pub source_path: Vec<String>,
    /// Where to copy the roms.
    pub destination_path: String,
    /// Path to the generated report in markdown format. Ex: report.md.
    pub report_path: String,
    /// If true, not-working CHD ROMs and Directories will not be copied.
    pub ignore_not_working_chd: bool,
    /// If true, no ROMs will be copied, but the report will still be generated as if they were.
    /// Needs valid `report_path`.
    pub simulation: bool,
    /// If set, only roms which ascii name alphabetical order is higher than this will be copied.
    pub subset_start: String,
    /// If set, only roms which ascii name alphabetical order is lower than this will be copied.
    pub subset_end: String,
    /// If true, show progress bar.
    pub progress: bool,
}

impl Args {
    pub fn new() -> Args {
        Default::default()
    }
}

pub fn command() -> Command {
    Command::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!("\n"))
        .about(crate_description!())
        .arg_required_else_help(true)
        .after_help("Examples:
    roms-curator --mame-xml-path=/mame/mame.xml --catver-path=/mame/catver.ini --source-path=/roms --destination-path=/roms-new/
    roms-curator -m /mame/mame.xml -c /mame/catver.ini -s /roms -d /roms-new/")
        .arg(
            Arg::new("mame_xml_path")
                .help("File path of Mame xml file. Extract with 'mame.exe -listxml > mame.xml'")
                .long("mame-xml-path")
                .short('m')
                .num_args(1)
                .required(true)
                .value_parser(validate_mame_xml_file)
        )
        .arg(
            Arg::new("catver_path")
                .help("File path of roms category file. Download pack from here [https://www.progettosnaps.net/support/]")
                .long("catver-path")
                .short('c')
                .num_args(1)
                .required(true)
                .value_parser(validate_catver_ini_file),
        )
        .arg(
            Arg::new("source_path")
                .help("Directory path(s) where your roms are. If more than one separate with a comma ','")
                .long("source-path")
                .short('s')
                .num_args(1)
                .required(true)
                .value_parser(validate_source_paths),
        )
        .arg(
            Arg::new("destination_path")
                .help("Directory path where your roms will be copied and categorized")
                .long("destination-path")
                .short('d')
                .num_args(1)
                .required(true)
                .value_parser(validate_destination_path),
        )
        .arg(
            Arg::new("report_path")
                .help("File path where the report should be saved. Contains all operations separated by successful and unsuccessful status")
                .long("report-path")
                .short('r')
                .num_args(1)
                .required(false)
                .default_value("")
                .value_parser(validate_report_path),
        )
        .arg(
            Arg::new("ignore_not_working_chd")
                .help("If true, not working CHD files will not be copied to [destination_path]. (true|false)")
                .long("ignore-not-working-chd")
                .short('i')
                .num_args(1)
                .required(false)
                .default_value("false")
                .value_parser(validate_ignore_not_working_chd),
        )
        .arg(
            Arg::new("simulation")
                .help("If true, not working CHD files will not be copied to [destination_path]. (true|false)")
                .long("simulation")
                .num_args(1)
                .required(false)
                .default_value("false")
                .value_parser(validate_simulation),
        )
        .arg(
            Arg::new("subset_start")
                .help("If true, not working CHD files will not be copied to [destination_path]. (true|false)")
                .long("subset-start")
                .num_args(1)
                .required(false)
                .default_value("")
                .value_parser(validate_subset),
        )
        .arg(
            Arg::new("subset_end")
                .help("If true, not working CHD files will not be copied to [destination_path]. (true|false)")
                .long("subset-end")
                .num_args(1)
                .required(false)
                .default_value("")
                .value_parser(validate_subset),
        )
        .arg(
            Arg::new("progress")
                .help("If true, shows a progress bar. Default is true. (true|false).")
                .long("progress")
                .short('p')
                .num_args(1)
                .required(false)
                .default_value("true")
                .value_parser(validate_progress),
        )
}

pub fn build_args() -> Result<Args, Box<dyn Error>> {
    let command = command();
    let matches = command.get_matches();

    let mame_xml_path: &String = matches.get_one("mame_xml_path").expect("validated in args parser");

    let catver_path: &String = matches.get_one("catver_path").expect("validated in args parser");

    let source_path: &Vec<String> = matches.get_one("source_path").expect("validated in args parser");

    let destination: &String = matches.get_one("destination_path").expect("validated in args parser");
    let destination_path = Path::new(destination.as_str());
    if destination_path.is_file() {
        return Err("Destination path needs to be a directory.".into());
    } else if !destination_path.exists() &&
        fs::create_dir_all(destination_path).is_err() {
        return Err("Destination directory cannot be created, verify path and/or permissions.".into());
    }
    // source path and destination path cannot be equal
    if source_path.iter().any(|path| path.eq_ignore_ascii_case(destination.as_str())) {
        return Err("[source-path] and [destination-path] cannot be the same.".into());
    }

    let report: &String = matches.get_one("report_path").expect("validated in args parser");
    if !report.is_empty() {
        let report_path = Path::new(report.as_str());
        if report_path.is_file() {
            return Err("Report file already exists.".into());
        } else if File::create(report_path).is_err() {
            return Err("Report file cannot be created, verify path and/or permissions.".into());
        } else {
            fs::remove_file(report_path).unwrap();
        }
    }

    let ignore_not_working_chd: &bool = matches.get_one("ignore_not_working_chd").expect("validated in args parser");

    let simulation: &bool = matches.get_one("simulation").expect("validated in args parser");
    // simulation implies report
    if *simulation && report.is_empty() {
        return Err("Simulation mode requires a report file.".into());
    }

    let subset_start: &String = matches.get_one("subset_start").expect("validated in args parser");
    let subset_end: &String = matches.get_one("subset_end").expect("validated in args parser");

    let progress: &bool = matches.get_one("progress").expect("validated in args parser");

    Ok(Args {
        mame_xml_path: mame_xml_path.clone(),
        catver_path: catver_path.clone(),
        source_path: source_path.to_vec(),
        destination_path: destination.clone(),
        report_path: report.clone(),
        ignore_not_working_chd: *ignore_not_working_chd,
        simulation: *simulation,
        subset_start: subset_start.clone(),
        subset_end: subset_end.clone(),
        progress: *progress,
    })
}

fn validate_mame_xml_file(path: &str) -> Result<String, String> {
    if !path.ends_with(".xml") {
        Err("File needs to be a XML file, for ex, mame.xml.".into())
    } else {
        validate_file_arg(path)
    }
}

fn validate_catver_ini_file(path: &str) -> Result<String, String> {
    if !path.ends_with(".ini") {
        Err("File needs to be a ini file, for ex, catver.ini.".into())
    } else {
        validate_file_arg(path)
    }
}

fn validate_source_paths(values: &str) -> Result<Vec<String>, String> {
    let paths: Vec<&str> = values.split(',').collect();

    let paths_sanitized: Vec<String> = paths.iter()
        .map(|p| sanitize_path(p))
        .collect();

    let fail = paths_sanitized.iter().any(|path| {
        let metadata = metadata(path);
        metadata.is_err() || metadata.unwrap().is_file()
    });
    if fail {
        return Err("Source path needs to be an existing directory(s).".into());
    }

    Ok(paths_sanitized)
}

fn validate_destination_path(value: &str) -> Result<String, String> {
    let path = sanitize_path(value);
    let destination_path = Path::new(path.as_str());

    if destination_path.is_file() {
        Err("Destination path needs to be a directory.".into())
    } else {
        Ok(path.to_string())
    }
}

fn validate_report_path(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Ok(value.to_string());
    }

    if !value.ends_with(".md") {
        return Err("Report file should have the extension .md".into());
    }

    let path = sanitize_path(value);
    let report = Path::new(path.as_str());

    if report.is_file() {
        Err("Report file already exists.".into())
    } else {
        Ok(path)
    }
}

fn validate_ignore_not_working_chd(value: &str) -> Result<bool, String> {
    if value.eq_ignore_ascii_case("true") {
        Ok(true)
    } else if value.eq_ignore_ascii_case("false") {
        Ok(false)
    } else {
        Err("Invalid value for ignore_not_working_chd. (true|false)".into())
    }
}

fn validate_simulation(value: &str) -> Result<bool, String> {
    if value.eq_ignore_ascii_case("true") {
        Ok(true)
    } else if value.eq_ignore_ascii_case("false") {
        Ok(false)
    } else {
        Err("Invalid value for simulation. (true|false)".into())
    }
}

fn validate_subset(value: &str) -> Result<String, String> {
    Ok(value.to_ascii_lowercase())
}

fn validate_progress(value: &str) -> Result<bool, String> {
    if value.eq_ignore_ascii_case("true") {
        Ok(true)
    } else if value.eq_ignore_ascii_case("false") {
        Ok(false)
    } else {
        Err("Invalid value for progress. (true|false)".into())
    }
}

///
/// Checks if file exists and have access to it.
///
fn validate_file_arg(path: &str) -> Result<String, String> {
    let sanitized_path = sanitize_path(path);

    if !Path::new(&sanitized_path).is_file() {
        Err("File not found.".to_string())
    } else {
        Ok(path.to_string())
    }
}

///
/// To support both '/' and '\' directory delimiters
///
fn sanitize_path(path: &str) -> String {
    if env::consts::OS.eq("windows") {
        path.replace('/', "\\")
    } else {
        path.replace('\\', "/")
    }
}
