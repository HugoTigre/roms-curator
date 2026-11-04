//! Shared helpers for Criterion benchmarks. Full metadata uses the same env flag as
//! integration tests: `CARGO_RC_RUN_EXPENSIVE_TESTS`.

#![allow(dead_code)]

use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::sync::{Once, OnceLock};
use std::{env, fs};

use log::{debug, LevelFilter};
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;
use roms_curator::core::args::Args;
use roms_curator::core::config::ExportConfig;
use roms_curator::ensure_metadata;
use roms_curator::models::roms::Catalog;

pub static TARGET_FOLDER: &str = "target/benches/";
pub static SHARED_METADATA_CACHE: &str = "target/benches/metadata-cache";
pub static MAME_XML_SAMPLE: &str = "tests/resources/listxml_sample.xml";
pub static CATVER_SAMPLE: &str = "tests/resources/catver_sample.ini";
pub static ROMS_SOURCE_PATH: &str = "tests/resources/merged_roms/";
pub static CHDS_SOURCE_PATH: &str = "tests/resources/chds/";
pub static CATEGORIZED_ROMS_FOLDER_NAME: &str = "export";

static INIT: Once = Once::new();

pub fn bench_metadata_label() -> &'static str {
    if run_expensive_tests() {
        "full"
    } else {
        "sample"
    }
}

pub fn set_up(tag: &str) {
    clean_up(tag);
    let test_folder = Path::new(TARGET_FOLDER).join(tag);
    create_test_dir(&test_folder);
    create_test_dir(&test_folder.join(CATEGORIZED_ROMS_FOLDER_NAME));

    INIT.call_once(|| {
        set_up_logging();
        let label = if run_expensive_tests() {
            format!(
                "MAME {} metadata ({})",
                expensive_mame_version(),
                bench_metadata_label()
            )
        } else {
            "sample listxml/catver fixtures".into()
        };
        debug!("Using {label} in benchmarks");
    });
}

pub fn clean_up(tag: &str) {
    let test_folder = Path::new(TARGET_FOLDER).join(tag);
    if test_folder.try_exists().unwrap_or(false) {
        fs::remove_dir_all(&test_folder).unwrap_or_else(|error| {
            panic!(
                "remove benchmark directory {}: {error}",
                test_folder.display()
            );
        });
    }
}

fn create_test_dir(path: &Path) {
    if path.try_exists().unwrap_or(false) {
        panic!(
            "{} already exists; remove {} before benchmarking.",
            path.display(),
            TARGET_FOLDER
        );
    }
    fs::create_dir_all(path).unwrap();
}

pub fn expensive_mame_version() -> String {
    env::var("CARGO_RC_MAME_VERSION").unwrap_or_else(|_| "0.289".into())
}

/// Override directory for full listxml + CatVer (`catver.ini` plus `mame-roms.xml` or `mame*.xml`).
pub fn shared_metadata_dir() -> Option<PathBuf> {
    env::var("CARGO_RC_BENCH_METADATA_DIR")
        .ok()
        .map(PathBuf::from)
}

fn metadata_paths_from_dir(dir: &Path) -> (String, String) {
    let catver = dir.join("catver.ini");
    if !catver.is_file() {
        panic!("metadata directory {}: missing catver.ini", dir.display());
    }
    let legacy_xml = dir.join("mame-roms.xml");
    let mame_xml = if legacy_xml.is_file() {
        legacy_xml
    } else {
        read_dir(dir)
            .unwrap_or_else(|error| panic!("read metadata directory {}: {error}", dir.display()))
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find(|path| {
                path.is_file()
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("mame") && name.ends_with(".xml"))
            })
            .unwrap_or_else(|| {
                panic!(
                    "metadata directory {}: expected mame-roms.xml or mame*.xml",
                    dir.display()
                )
            })
    };
    (
        mame_xml.to_string_lossy().into_owned(),
        catver.to_string_lossy().into_owned(),
    )
}

fn expensive_metadata_paths() -> (String, String) {
    static PATHS: OnceLock<(String, String)> = OnceLock::new();
    PATHS
        .get_or_init(|| {
            if let Some(dir) = shared_metadata_dir() {
                return metadata_paths_from_dir(&dir);
            }
            let version = expensive_mame_version();
            let cache = PathBuf::from(SHARED_METADATA_CACHE);
            fs::create_dir_all(&cache).unwrap_or_else(|error| {
                panic!("create metadata cache {}: {error}", cache.display())
            });
            let paths = ensure_metadata(&version, &cache, false).unwrap_or_else(|error| {
                panic!("download MAME {version} metadata for benchmarks: {error}")
            });
            (
                paths.mame_xml.to_string_lossy().into_owned(),
                paths.catver.to_string_lossy().into_owned(),
            )
        })
        .clone()
}

pub fn build_args(tag: &str, simulation: bool) -> Args {
    let test_folder = Path::new(TARGET_FOLDER).join(tag);

    let (mame_xml_path, catver_path) = if run_expensive_tests() {
        expensive_metadata_paths()
    } else {
        (MAME_XML_SAMPLE.to_string(), CATVER_SAMPLE.to_string())
    };

    Args {
        mame_xml_path,
        catver_path,
        source_path: vec![ROMS_SOURCE_PATH.to_string(), CHDS_SOURCE_PATH.to_string()],
        destination_path: test_folder
            .join(CATEGORIZED_ROMS_FOLDER_NAME)
            .to_string_lossy()
            .into_owned(),
        report_path: String::new(),
        plan_path: String::new(),
        ignore_not_working_chd: false,
        simulation,
        subset_start: String::new(),
        subset_end: String::new(),
        progress: false,
    }
}

pub fn load_catalog(args: &ExportConfig) -> Catalog {
    roms_curator::run(args).unwrap_or_else(|error| {
        panic!(
            "load catalog for bench (metadata={}): {error}",
            bench_metadata_label()
        )
    })
}

pub fn export_destination(parent: &Path) -> PathBuf {
    parent.join("curated")
}

fn set_up_logging() {
    let stdout_appender = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{h({d(%Y-%m-%d %H:%M:%S)(utc)} - {l}: {m}{n})}",
        )))
        .build();
    let config = log4rs::Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout_appender)))
        .build(Root::builder().appender("stdout").build(LevelFilter::Warn))
        .unwrap();
    let _ = log4rs::init_config(config);
}

pub fn run_expensive_tests() -> bool {
    matches!(
        env::var("CARGO_RC_RUN_EXPENSIVE_TESTS"),
        Ok(value) if value == "true"
    )
}

pub fn count_source_files() -> usize {
    read_dir(ROMS_SOURCE_PATH)
        .map(|entries| entries.count())
        .unwrap_or(0)
        + read_dir(CHDS_SOURCE_PATH)
            .map(|entries| entries.count())
            .unwrap_or(0)
}
