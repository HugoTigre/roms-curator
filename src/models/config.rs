use std::fs;
use std::fs::metadata;
use std::path::{Path, PathBuf};

/// Stores startup program arguments
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Config {
    pub mame_xml_path: String,
    pub catver_path: String,
    pub source_path: Vec<String>,
    pub destination_path: String,
    pub report_path: String,
    pub ignore_not_working_chd: bool,
    pub simulation: bool, // if true only runs simulation, needs report_path set
}

pub struct DestinationFolders {
    pub working: PathBuf,
    pub other: PathBuf,
    pub chd_working: PathBuf,
    pub chd_other: PathBuf,
}

impl Config {
    pub fn new() -> Config {
        Default::default()
    }

    pub fn build(mut self, mut args: impl Iterator<Item=String>) -> Result<Config, &'static str> {
        args.next();
        let mut help = false;

        for arg in args.by_ref() {
            if arg.eq_ignore_ascii_case("--help") {
                Self::output_help();
                help = true;
                break;
            }

            let arg_split: Vec<&str> = arg.split('=').collect();

            if arg_split.len() != 2 {
                // let msg = format!("{}{}", "Invalid argument: ", arg);
                return Err("Invalid argument");
            }

            let param = arg_split[0].trim().to_lowercase();
            if !Self::check_param(&param) {
                return Err("Argument not recognized.");
            }

            let value = arg_split[1].trim();
            if value.is_empty() {
                return Err("Value cannot be empty.");
            }

            match param.as_str() {
                "--mame_xml_path" => {
                    if !value.ends_with(".xml") {
                        return Err("File needs to be a XML file, for ex, mame.xml.");
                    }
                    self.mame_xml_path = value.to_string()
                }
                "--catver_path" => {
                    if !value.ends_with(".ini") {
                        return Err("File needs to be a ini file, for ex, catver.ini.");
                    }
                    self.catver_path = value.to_string()
                }
                "--source_path" => {
                    let paths: Vec<&str> = value.split(',').collect();
                    let _fail = paths.iter().any(|path| {
                        let metadata = metadata(path);
                        metadata.is_err() || metadata.unwrap().is_file()
                    });
                    if _fail {
                        return Err("Source path needs to be an existing directory.");
                    }
                    self.source_path = paths.iter().map(|v| v.to_string()).collect()
                }
                "--destination_path" => {
                    if metadata(value).unwrap().is_file() {
                        return Err("Destination path needs to be a directory.");
                    }
                    self.destination_path = value.to_string()
                }
                "--report_path" => {
                    if metadata(value).unwrap().is_file() {
                        return Err("Report path needs to be a file.");
                    }
                    self.report_path = value.to_string()
                }
                "--ignore_not_working_chd" => {
                    self.ignore_not_working_chd = value.eq_ignore_ascii_case("true");
                }
                "--simulation" => {
                    self.simulation = value.eq_ignore_ascii_case("true");
                }
                _ =>
                    println!("{} param ignored (not recognized).", param)
            }
        }

        Self::check_mandatory_arguments(&self, help)?;

        Ok(Config {
            mame_xml_path: self.mame_xml_path,
            catver_path: self.catver_path,
            source_path: self.source_path,
            destination_path: self.destination_path,
            report_path: self.report_path,
            ignore_not_working_chd: self.ignore_not_working_chd,
            simulation: self.simulation,
        })
    }

    fn check_param(param: &str) -> bool {
        matches!(
            param,
            "--mame_xml_path" | "--catver_path"
            | "--source_path" | "--destination_path"
            | "--create_report" | "--simulation"
        )
    }

    fn check_mandatory_arguments(config: &Config, help: bool) -> Result<bool, &'static str> {
        if help { return Ok(true); }
        if config.mame_xml_path.is_empty() { return Err("Missing mame_xml_path."); }
        if config.catver_path.is_empty() { return Err("Missing catver_path."); }
        if config.source_path.is_empty() { return Err("Missing source_path."); }
        if config.destination_path.is_empty() { return Err("Missing destination_path."); }
        if config.source_path.iter().any(|path| path.eq_ignore_ascii_case(config.destination_path.as_str())) {
            return Err("SOURCE_PATH and DESTINATION_PATH cannot be the same.");
        }
        if config.simulation && config.report_path.is_empty() {
            return Err("Simulation mode needs REPORT_PATH location.");
        }
        Ok(true)
    }

    pub fn build_destination_folders_path(&self) -> DestinationFolders {
        let destination_dir = Path::new(&self.destination_path);
        let working = destination_dir.join("working");
        let other = destination_dir.join("other");
        let chd_working = destination_dir.join("chd_working");
        let chd_other = destination_dir.join("chd_other");

        fs::create_dir_all(&working).expect(&("Error creating ".to_string() + "working directory"));
        fs::create_dir_all(&other).expect(&("Error creating ".to_string() + "other directory"));
        fs::create_dir_all(&chd_working).expect(&("Error creating ".to_string() + "chd_working directory"));
        fs::create_dir_all(&chd_other).expect(&("Error creating ".to_string() + "chd_other directory"));

        DestinationFolders { working, other, chd_working, chd_other }
    }

    fn output_help() {
        let help = "Usage:  roms-curator [OPTION=VALUE]

Options:
  --mame_xml_path          File path of Mame xml file. Extract with 'mame.exe -listxml > mame.xml'
  --catver_path            File path of roms category file. Download pack from here [https://www.progettosnaps.net/support/]
  --source_path            Directory path(s) where your roms are. If more than one separate with a comma ','.
  --destination_path       Directory path where your roms will be copied and categorized.
  --report_path            File path where the report should be saved. Contains all operations separated by successful and unsuccessful status.
  --ignore_not_working_chd If true, not working CHD files will not be copied to [destination_path]. (true|false).
  --simulation             If true, does not make any real changes. Depends on [report_path]. (true|false).

Example:
  roms-curator --mame_xml_path=/mame/mame.xml --catver_path=/mame/catver.ini --source_path=/roms --destination_path=/roms-new/ ";

        println!("{}", help);
    }
}

