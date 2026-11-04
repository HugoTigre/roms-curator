#![allow(dead_code)]

use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::sync::OnceLock;
use std::{env, fs};

use log::{debug, LevelFilter};
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;
use rand::distr::Alphanumeric;
use rand::Rng;
use roms_curator::core::args::Args;
use roms_curator::ensure_metadata;

pub static TARGET_FOLDER: &str = "target/tests/";
pub static MAME_XML_SAMPLE: &str = "tests/resources/listxml_sample.xml";
pub static CATVER_SAMPLE: &str = "tests/resources/catver_sample.ini";
pub static WORKING_ARCADE_SAMPLE: &str = "tests/resources/working_arcade_sample.ini";
pub static ROMS_SOURCE_PATH: &str = "tests/resources/merged_roms/";
pub static CHDS_SOURCE_PATH: &str = "tests/resources/chds/";
pub static CATEGORIZED_ROMS_FOLDER_NAME: &str = "categorized_roms";
pub static CATEGORIZED_WORKING_FOLDER_NAME: &str = "roms";
pub static CATEGORIZED_OTHER_FOLDER_NAME: &str = "rest";

static INIT: Once = Once::new();

pub fn get_test_tag() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(4)
        .map(char::from)
        .collect()
}

pub fn set_up(tag: &str) {
    debug!("Setting up tests environment [{tag}]...");
    clean_up(tag);

    let test_folder = Path::new(TARGET_FOLDER).join(tag);
    create_test_dir(&test_folder);

    let categorized_roms_path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME);
    create_test_dir(&categorized_roms_path);

    INIT.call_once(|| {
        set_up_logging();
        let label = if run_expensive_tests() {
            format!("downloaded MAME {}", expensive_mame_version())
        } else {
            "sample listxml/catver fixtures".into()
        };
        debug!("Using {label} in integration tests");
    });
}

pub fn clean_up(tag: &str) {
    debug!("Cleaning up tests");

    let test_folder = Path::new(TARGET_FOLDER).join(tag);

    if let Ok(exists) = test_folder.try_exists() {
        if exists {
            fs::remove_dir_all(test_folder).expect("Error deleting folder categorized_roms");
        }
    }
}

fn create_test_dir(path: &Path) {
    match path.try_exists() {
        Ok(exists) => {
            if !exists {
                fs::create_dir_all(path).unwrap_or_else(|error| {
                    panic!("Error creating {} directory: {error}", path.display())
                });
            } else {
                panic!(
                    "{} directory already exists. Try deleting the directory before running tests.",
                    path.to_str().unwrap()
                );
            }
        }
        Err(err) => {
            panic!("Error creating directory {err}");
        }
    }
}

pub fn get_files_from_folder(path: &str) -> Vec<String> {
    let mut roms: Vec<String> = Vec::new();
    for entry in read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        let file_name = path.file_name().unwrap().to_str().unwrap();
        roms.push(file_name.to_string());
    }
    roms
}

pub fn expensive_mame_version() -> String {
    env::var("CARGO_RC_MAME_VERSION").unwrap_or_else(|_| "0.289".into())
}

pub fn expensive_metadata_cache_dir() -> PathBuf {
    Path::new(TARGET_FOLDER).join("metadata-cache")
}

fn expensive_metadata_paths() -> (String, String) {
    static PATHS: OnceLock<(String, String)> = OnceLock::new();
    PATHS
        .get_or_init(|| {
            let version = expensive_mame_version();
            let cache = expensive_metadata_cache_dir();
            fs::create_dir_all(&cache).expect("metadata cache directory");
            let paths = ensure_metadata(&version, &cache, false)
                .unwrap_or_else(|error| panic!("download MAME {version} metadata: {error}"));
            (
                paths.mame_xml.to_string_lossy().into_owned(),
                paths.catver.to_string_lossy().into_owned(),
            )
        })
        .clone()
}

pub fn build_args(tag: &str, simulate: bool, subset_start: String, subset_end: String) -> Args {
    let run_expensive_tests = run_expensive_tests();
    let test_folder = Path::new(TARGET_FOLDER).join(tag);

    let (mame_xml_path, catver_path) = if run_expensive_tests {
        expensive_metadata_paths()
    } else {
        (
            Path::new(MAME_XML_SAMPLE).to_str().unwrap().to_string(),
            Path::new(CATVER_SAMPLE).to_str().unwrap().to_string(),
        )
    };

    let destination_path = test_folder
        .join(CATEGORIZED_ROMS_FOLDER_NAME)
        .to_str()
        .unwrap()
        .to_string();
    let report_path = test_folder.join("report.md").to_str().unwrap().to_string();
    let ignore_not_working_chd = false;
    let simulation = simulate;
    let progress = true;

    Args {
        mame_xml_path,
        catver_path,
        source_path: vec![ROMS_SOURCE_PATH.to_string(), CHDS_SOURCE_PATH.to_string()],
        destination_path,
        report_path,
        plan_path: String::new(),
        ignore_not_working_chd,
        simulation,
        subset_start,
        subset_end,
        progress,
    }
}

fn set_up_logging() {
    if Path::new("logging.yaml").exists() {
        log4rs::init_file("logging.yaml", Default::default()).unwrap();
    } else {
        let stdout_appender = ConsoleAppender::builder()
            .encoder(Box::new(PatternEncoder::new(
                "{h({d(%Y-%m-%d %H:%M:%S)(utc)} - {l}: {m}{n})}",
            )))
            .build();

        let config = log4rs::Config::builder()
            .appender(Appender::builder().build("stdout", Box::new(stdout_appender)))
            .build(Root::builder().appender("stdout").build(LevelFilter::Debug))
            .unwrap();

        log4rs::init_config(config).unwrap();
    }
}

pub fn run_expensive_tests() -> bool {
    match env::var("CARGO_RC_RUN_EXPENSIVE_TESTS") {
        Ok(s) => s == "true",
        _ => false,
    }
}
