use std::{env, fs, io};
use std::borrow::ToOwned;
use std::fs::File;
use std::path::Path;
use std::sync::Once;

use log::{debug, LevelFilter};
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;

static TARGET_FOLDER: &str = "target/tests/";
static RESOURCES_PROD_PATH: &str = "tests/resources/prod_lists_0244.zip";
static CATEGORIZED_ROMS_FOLDER_NAME: &str = "categorized_roms";

static INIT: Once = Once::new();

pub fn set_up(tag: &String) {
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
        if run_expensive_tests() { debug!("Using mame full set in integration tests"); } else { debug!("Using mame small set in integration tests"); };
    });
}

pub fn clean_up(tag: &String) {
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
