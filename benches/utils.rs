//! Shared helpers for Criterion benchmarks. Full metadata uses the same env flag as
//! integration tests: `CARGO_RC_RUN_EXPENSIVE_TESTS`.

#![allow(dead_code)]

use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::{env, fs};

use log::{debug, LevelFilter};
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;
use roms_curator::core::args::Args;
use roms_curator::core::config::ExportConfig;
use roms_curator::models::roms::Catalog;

static RESOURCES_PROD_PATH: &str = "tests/resources/prod_lists_0244.zip";

pub static TARGET_FOLDER: &str = "target/benches/";
pub static SHARED_METADATA_DIR: &str = "target/benches/_shared_metadata";
pub static MAME_XML_SAMPLE: &str = "tests/resources/listxml_0244.xml";
pub static CATVER_SAMPLE: &str = "tests/resources/catver_0244.ini";
pub static ROMS_SOURCE_PATH: &str = "tests/resources/merged_roms/";
pub static CHDS_SOURCE_PATH: &str = "tests/resources/chds/";
pub static CATEGORIZED_ROMS_FOLDER_NAME: &str = "export";

static INIT: Once = Once::new();
static SHARED_METADATA: Once = Once::new();

pub static MAME_XML_FULL: &str = "mame-roms.xml";
pub static CATVER_FULL: &str = "catver.ini";

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

    if run_expensive_tests() {
        ensure_shared_metadata();
    }

    INIT.call_once(|| {
        set_up_logging();
        debug!("Benchmark metadata: {}", bench_metadata_label());
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

/// Directory holding full listxml + CatVer for expensive runs. Override with
/// `CARGO_RC_BENCH_METADATA_DIR` when metadata is already on disk (extracted zip or future CLI cache).
pub fn shared_metadata_dir() -> PathBuf {
    env::var("CARGO_RC_BENCH_METADATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(SHARED_METADATA_DIR))
}

fn ensure_shared_metadata() {
    SHARED_METADATA.call_once(|| {
        let dir = shared_metadata_dir();
        let xml = dir.join(MAME_XML_FULL);
        let catver = dir.join(CATVER_FULL);
        if xml.is_file() && catver.is_file() {
            return;
        }
        fs::create_dir_all(&dir).unwrap_or_else(|error| {
            panic!("create shared metadata directory {}: {error}", dir.display())
        });
        unzip_prod_lists_into(&dir);
    });
}

fn unzip_prod_lists_into(destination: &Path) {
    use std::fs::File;
    use std::io;

    let resource_file = File::open(RESOURCES_PROD_PATH)
        .unwrap_or_else(|error| panic!("open {RESOURCES_PROD_PATH}: {error}"));
    let mut prod_data = zip::ZipArchive::new(resource_file).unwrap();

    for index in 0..prod_data.len() {
        let mut file = prod_data.by_index(index).unwrap();
        let outpath = match file.enclosed_name() {
            Some(path) => destination.join(path),
            None => continue,
        };
        if outpath.to_string_lossy().contains("__MACOS") {
            continue;
        }
        if file.is_dir() {
            fs::create_dir_all(&outpath).unwrap();
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            let mut outfile = File::create(&outpath).unwrap();
            io::copy(&mut file, &mut outfile).unwrap();
        }
    }
}

pub fn build_args(tag: &str, simulation: bool) -> Args {
    let test_folder = Path::new(TARGET_FOLDER).join(tag);

    let (mame_xml_path, catver_path) = if run_expensive_tests() {
        let dir = shared_metadata_dir();
        (
            dir.join(MAME_XML_FULL).to_string_lossy().into_owned(),
            dir.join(CATVER_FULL).to_string_lossy().into_owned(),
        )
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
