use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use roms_curator::core::args::Args;
use roms_curator::core::roms_service::RomsExt;
use roms_curator::models::roms::Roms;

const CONTENTS: &[u8] = b"source ROM sentinel";

struct Collection {
    root: PathBuf,
    args: Args,
}

impl Collection {
    fn new() -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tests");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!(
            "roms-curator-safety-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir(&root).unwrap();
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("005.zip"), CONTENTS).unwrap();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/resources");
        let args = Args {
            source_path: vec![source.to_str().unwrap().into()],
            destination_path: root.join("output").to_str().unwrap().into(),
            report_path: root.join("report.md").to_str().unwrap().into(),
            mame_xml_path: fixtures.join("listxml_0244.xml").to_str().unwrap().into(),
            catver_path: fixtures.join("catver_0244.ini").to_str().unwrap().into(),
            ..Args::default()
        };
        Self { root, args }
    }

    fn cli(&self) -> Command {
        let mut command = Command::cargo_bin("roms-curator").unwrap();
        command.args([
            "--mame-xml-path",
            &self.args.mame_xml_path,
            "--catver-path",
            &self.args.catver_path,
            "--source-path",
            &self.args.source_path.join(","),
            "--destination-path",
            &self.args.destination_path,
            "--report-path",
            &self.args.report_path,
            "--progress=false",
        ]);
        command
    }

    fn rejected(&self) {
        // Library callers constructing Args manually must receive the same protection.
        let report_before = fs::read(self.root.join("report.md")).ok();
        assert!(Roms::new().copy_roms(&self.args).is_err());
        self.cli().assert().failure();
        assert_eq!(
            fs::read(self.root.join("source/005.zip")).unwrap(),
            CONTENTS
        );
        assert_eq!(fs::read(self.root.join("report.md")).ok(), report_before);
    }
}

impl Drop for Collection {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn rejects_equal_and_dot_aliases_without_creating_output_folders() {
    let mut collection = Collection::new();
    collection.args.destination_path = collection.root.join("source/.").to_str().unwrap().into();
    collection.rejected();
    assert!(!collection.root.join("source/working").exists());
}

#[test]
fn rejects_new_output_inside_source_before_creating_any_ancestor() {
    let mut collection = Collection::new();
    collection.args.destination_path = collection
        .root
        .join("source/new/deep")
        .to_str()
        .unwrap()
        .into();
    collection.rejected();
    assert!(!collection.root.join("source/new").exists());
}

#[test]
fn rejects_source_inside_output() {
    let mut collection = Collection::new();
    collection.args.destination_path = collection.root.to_str().unwrap().into();
    collection.rejected();
    assert!(!collection.root.join("working").exists());
}

#[test]
fn rejects_relative_source_alias_to_absolute_destination() {
    let mut collection = Collection::new();
    collection.args.destination_path = collection.args.source_path[0].clone();
    collection.args.source_path = vec![Path::new("target/tests")
        .join(collection.root.file_name().unwrap())
        .join("source")
        .to_str()
        .unwrap()
        .into()];
    collection.rejected();
    assert!(!collection.root.join("source/working").exists());
}

#[test]
fn checks_every_source_root() {
    let mut collection = Collection::new();
    let other = collection.root.join("second");
    fs::create_dir(&other).unwrap();
    collection
        .args
        .source_path
        .push(other.to_str().unwrap().into());
    collection.args.destination_path = other.join("new/output").to_str().unwrap().into();
    collection.rejected();
    assert!(!other.join("new").exists());
}

#[test]
fn rejects_report_inside_source_before_creating_destination() {
    let mut collection = Collection::new();
    collection.args.report_path = collection
        .root
        .join("source/report.md")
        .to_str()
        .unwrap()
        .into();
    collection.rejected();
    assert!(!collection.root.join("source/report.md").exists());
    assert!(!collection.root.join("output").exists());
}

#[test]
fn rejects_existing_output_hardlink_to_source() {
    let collection = Collection::new();
    let working = collection.root.join("output/working");
    fs::create_dir_all(&working).unwrap();
    fs::hard_link(
        collection.root.join("source/005.zip"),
        working.join("005.zip"),
    )
    .unwrap();
    collection.rejected();
    assert!(!collection.root.join("output/chd_other").exists());
}

#[test]
fn rejects_report_hardlink_to_source() {
    let collection = Collection::new();
    fs::hard_link(
        collection.root.join("source/005.zip"),
        &collection.args.report_path,
    )
    .unwrap();
    collection.rejected();
    assert!(!collection.root.join("output").exists());
}

#[test]
fn independent_new_destination_copies_source_bytes() {
    let collection = Collection::new();
    collection.cli().assert().success();
    assert_eq!(
        fs::read(collection.root.join("output/working/005.zip")).unwrap(),
        CONTENTS
    );
    assert_eq!(
        fs::read(collection.root.join("source/005.zip")).unwrap(),
        CONTENTS
    );
}

#[test]
fn rejects_a_file_as_destination_without_modifying_it() {
    let collection = Collection::new();
    fs::write(&collection.args.destination_path, b"existing output").unwrap();
    collection.rejected();
    assert_eq!(
        fs::read(&collection.args.destination_path).unwrap(),
        b"existing output"
    );
}

#[cfg(unix)]
fn link_directory(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(windows)]
fn link_directory(target: &Path, link: &Path) {
    // Junctions do not require Windows Developer Mode or symbolic-link privileges.
    let output = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(any(unix, windows))]
#[test]
fn rejects_output_root_alias_to_source() {
    let mut collection = Collection::new();
    let alias = collection.root.join("alias");
    link_directory(&collection.root.join("source"), &alias);
    collection.args.destination_path = alias.join("new/output").to_str().unwrap().into();
    collection.rejected();
    assert!(!collection.root.join("source/new").exists());
}

#[cfg(any(unix, windows))]
#[test]
fn rejects_links_inside_existing_output() {
    let collection = Collection::new();
    fs::create_dir(collection.root.join("output")).unwrap();
    link_directory(
        &collection.root.join("source"),
        &collection.root.join("output/working"),
    );
    collection.rejected();
    assert!(!collection.root.join("output/chd_other").exists());
}

#[cfg(any(unix, windows))]
#[test]
fn rejects_linked_source_entries_even_when_their_target_is_external() {
    let collection = Collection::new();
    let external = collection.root.join("external");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("keep"), CONTENTS).unwrap();
    link_directory(&external, &collection.root.join("source/linked"));
    collection.rejected();
    assert_eq!(fs::read(external.join("keep")).unwrap(), CONTENTS);
    assert!(!collection.root.join("output").exists());
}

#[cfg(any(unix, windows))]
#[test]
fn rejects_report_through_source_alias() {
    let mut collection = Collection::new();
    let alias = collection.root.join("alias");
    link_directory(&collection.root.join("source"), &alias);
    collection.args.report_path = alias.join("report.md").to_str().unwrap().into();
    collection.rejected();
    assert!(!collection.root.join("source/report.md").exists());
}

#[cfg(any(unix, windows))]
#[test]
fn independent_root_aliases_are_allowed() {
    let mut collection = Collection::new();
    let alias = collection.root.join("source-alias");
    link_directory(&collection.root.join("source"), &alias);
    collection.args.source_path = vec![alias.to_str().unwrap().into()];
    collection.cli().assert().success();
    assert_eq!(
        fs::read(collection.root.join("output/working/005.zip")).unwrap(),
        CONTENTS
    );
}

#[cfg(unix)]
#[test]
fn resolves_link_before_parent_components() {
    let mut collection = Collection::new();
    let target = collection.root.join("deep/target");
    fs::create_dir_all(&target).unwrap();
    let alias = collection.root.join("alias");
    link_directory(&target, &alias);
    collection.args.destination_path = alias.join("../../source/new").to_str().unwrap().into();
    collection.rejected();
    assert!(!collection.root.join("source/new").exists());
}

#[cfg(any(unix, windows))]
#[test]
fn independent_destination_root_alias_is_allowed() {
    let collection = Collection::new();
    let output = collection.root.join("actual-output");
    fs::create_dir(&output).unwrap();
    link_directory(&output, Path::new(&collection.args.destination_path));
    collection.cli().assert().success();
    assert_eq!(fs::read(output.join("working/005.zip")).unwrap(), CONTENTS);
    assert_eq!(
        fs::read(collection.root.join("source/005.zip")).unwrap(),
        CONTENTS
    );
}

#[cfg(unix)]
#[test]
fn rejects_dangling_output_link_without_creating_its_target() {
    let collection = Collection::new();
    let missing = collection.root.join("missing");
    std::os::unix::fs::symlink(&missing, &collection.args.destination_path).unwrap();
    collection.rejected();
    assert!(!missing.exists());
}

#[cfg(unix)]
#[test]
fn rejects_file_links_inside_source() {
    let collection = Collection::new();
    let external = collection.root.join("external-file");
    fs::write(&external, CONTENTS).unwrap();
    std::os::unix::fs::symlink(&external, collection.root.join("source/linked.zip")).unwrap();
    collection.rejected();
    assert_eq!(fs::read(external).unwrap(), CONTENTS);
}

#[cfg(windows)]
#[test]
fn rejects_case_aliases() {
    let mut collection = Collection::new();
    collection.args.destination_path = collection.args.source_path[0].to_uppercase();
    collection.rejected();
}
