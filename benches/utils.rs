#![allow(dead_code)]

use std::{env, fs, io};
use std::borrow::ToOwned;
use std::fs::{File, read_dir};
use std::path::Path;
use std::sync::Once;

use log::{debug, LevelFilter};
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;
use rand::distributions::Alphanumeric;
use rand::Rng;
use roms_curator::core::args::Args;

static RESOURCES_PROD_PATH: &str = "tests/resources/prod_lists_0244.zip";

pub static TARGET_FOLDER: &str = "target/tests/";
pub static MAME_XML_FILE_NAME_FULL_SET: &str = "mame-roms.xml";
pub static MAME_XML_FILE_NAME_SMALL_SET: &str = "tests/resources/listxml_0244.xml";
pub static CATEGORY_LIST_FILE_NAME_FULL_SET: &str = "catver.ini";
pub static CATEGORY_LIST_FILE_NAME_SMALL_SET: &str = "tests/resources/catver_0244.ini";
pub static WORKING_ARCADE_LIST_PATH: &str = "tests/resources/working_arcade_0244.ini";
pub static ROMS_SOURCE_PATH: &str = "tests/resources/merged_roms/";
pub static CHDS_SOURCE_PATH: &str = "tests/resources/chds/";
pub static CATEGORIZED_ROMS_FOLDER_NAME: &str = "categorized_roms";
pub static CATEGORIZED_WORKING_FOLDER_NAME: &str = "working";
pub static CATEGORIZED_OTHER_FOLDER_NAME: &str = "other";
pub static CATEGORIZED_CHD_OTHER_FOLDER_NAME: &str = "chd_other";
pub static CATEGORIZED_CHD_WORKING_FOLDER_NAME: &str = "chd_working";

static INIT: Once = Once::new();

pub fn get_test_tag() -> String {
    rand::thread_rng()
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

    if run_expensive_tests() {
        unzip_test_resources_archive(tag);
    }

    INIT.call_once(|| {
        set_up_logging();
        let small_or_full = if run_expensive_tests() { "full" } else { "small" };
        debug!("Using mame {small_or_full} set in integration tests");
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

fn unzip_test_resources_archive(tag: &str) {
    let resource_file = File::open(RESOURCES_PROD_PATH).unwrap();
    let test_folder = Path::new(TARGET_FOLDER).join(tag);

    let mut prod_data = zip::ZipArchive::new(resource_file).unwrap();

    for i in 0..prod_data.len() {
        let mut file = prod_data.by_index(i).unwrap();
        let outpath = match file.enclosed_name() {
            Some(path) => test_folder.join(path).to_owned(),
            None => continue,
        };

        if outpath.to_str().unwrap().contains("__MACOS") { continue; }

        if (*file.name()).ends_with('/') {
            fs::create_dir_all(&outpath).unwrap();
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p).unwrap();
                }
            }
            let mut outfile = File::create(&outpath).unwrap();
            io::copy(&mut file, &mut outfile).unwrap();
        }

        // Get and Set permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&outpath, fs::Permissions::from_mode(mode)).unwrap();
            }
        }
    }
}

fn create_test_dir(path: &Path) {
    match path.try_exists() {
        Ok(exists) => {
            if !exists {
                fs::create_dir_all(path)
                    .expect(&("Error creating ".to_string() + path.to_str().unwrap() + " directory"));
            } else {
                panic!("{} directory already exists. Try deleting the directory before running tests.", path.to_str().unwrap());
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

pub fn build_args(
    tag: &str, simulate: bool, subset_start: String, subset_end: String,
) -> Args {
    let run_expensive_tests = run_expensive_tests();
    let test_folder = Path::new(TARGET_FOLDER).join(tag);

    let mame_xml_path = if run_expensive_tests {
        test_folder.join(MAME_XML_FILE_NAME_FULL_SET).to_str().unwrap().to_string()
    } else {
        Path::new(MAME_XML_FILE_NAME_SMALL_SET).to_str().unwrap().to_string()
    };

    let catver_path = if run_expensive_tests {
        test_folder.join(CATEGORY_LIST_FILE_NAME_FULL_SET).to_str().unwrap().to_string()
    } else {
        Path::new(CATEGORY_LIST_FILE_NAME_SMALL_SET).to_str().unwrap().to_string()
    };

    let destination_path = test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME).to_str().unwrap().to_string();
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
            .encoder(Box::new(PatternEncoder::new("{h({d(%Y-%m-%d %H:%M:%S)(utc)} - {l}: {m}{n})}")))
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
        _ => false
    }
}
