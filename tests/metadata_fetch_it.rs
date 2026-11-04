//! Live metadata download smoke tests. Run with:
//! `CARGO_RC_RUN_EXPENSIVE_TESTS=true cargo test --locked --test metadata_fetch_it -- --test-threads=1`

mod utils;

use roms_curator::{ensure_metadata, ExportConfig};

#[test]
fn downloads_listxml_and_catver_and_parses_them() {
    if !utils::run_expensive_tests() {
        return;
    }

    let cache = tempfile::tempdir().expect("temp metadata cache");
    let version = utils::expensive_mame_version();
    let paths = ensure_metadata(&version, cache.path(), true).expect("download MAME metadata");

    assert!(
        paths.mame_xml.is_file(),
        "missing {}",
        paths.mame_xml.display()
    );
    assert!(paths.catver.is_file(), "missing {}", paths.catver.display());

    let destination = cache.path().join("export-out");
    let config = ExportConfig {
        mame_xml_path: paths.mame_xml.to_string_lossy().into_owned(),
        catver_path: paths.catver.to_string_lossy().into_owned(),
        source_path: vec![utils::ROMS_SOURCE_PATH.into()],
        destination_path: destination.to_string_lossy().into_owned(),
        simulation: true,
        progress: false,
        ..ExportConfig::default()
    };
    config.validate().expect("export config should validate");
    roms_curator::run(&config).expect("parse downloaded metadata");
}
