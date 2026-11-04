use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
#[cfg(unix)]
use predicates::prelude::PredicateBooleanExt;
use roms_curator::core::args::Args;
use roms_curator::core::roms_service::copy_all_to_rest;
use roms_curator::models::report::OperationStatus;
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
            mame_xml_path: fixtures.join("listxml_sample.xml").to_str().unwrap().into(),
            catver_path: fixtures.join("catver_sample.ini").to_str().unwrap().into(),
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
            "--subset-start",
            &self.args.subset_start,
            "--subset-end",
            &self.args.subset_end,
        ]);
        command.arg(format!("--simulation={}", self.args.simulation));
        command
    }

    fn copy_conflict(&self, message: &str) {
        let catalog = roms_curator::run(&self.args).unwrap();
        let error = catalog.copy_roms(&self.args).unwrap_err().to_string();
        assert!(error.contains(message), "{error}");
        // A rejected copy must not publish the selected-name list.
        assert!(!self.root.join("output/selected.txt").exists());
        self.cli()
            .assert()
            .failure()
            .stdout(predicates::str::contains(message));
        assert_eq!(
            fs::read(self.root.join("source/005.zip")).unwrap(),
            CONTENTS
        );
        assert!(!Path::new(&self.args.report_path).exists());
    }

    fn rejected(&self) {
        // Library callers constructing Args manually must receive the same protection.
        let report_before = fs::read(self.root.join("report.md")).ok();
        assert!(copy_all_to_rest(Roms::new(), &self.args).is_err());
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
    assert!(!collection.root.join("source/roms").exists());
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
    assert!(!collection.root.join("roms").exists());
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
    assert!(!collection.root.join("source/roms").exists());
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
    let working = collection.root.join("output/roms");
    fs::create_dir_all(&working).unwrap();
    fs::hard_link(
        collection.root.join("source/005.zip"),
        working.join("005.zip"),
    )
    .unwrap();
    collection.rejected();
    assert!(!collection.root.join("output/rest").exists());
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
        fs::read(collection.root.join("output/roms/005.zip")).unwrap(),
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
    // mklink treats `/` as a switch. Path::join("target/tests") keeps that slash,
    // so pass native separators or the junction path is rejected as an option.
    let link = native_path(link);
    let target = native_path(target);
    let output = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "mklink /J \"{}\" \"{}\" failed: {}{}",
        link.display(),
        target.display(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}

#[cfg(windows)]
fn native_path(path: &Path) -> PathBuf {
    let mut native = PathBuf::new();
    for component in path.components() {
        native.push(component);
    }
    native
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
        &collection.root.join("output/roms"),
    );
    collection.rejected();
    assert!(!collection.root.join("output/rest").exists());
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
        fs::read(collection.root.join("output/roms/005.zip")).unwrap(),
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
    assert_eq!(fs::read(output.join("roms/005.zip")).unwrap(), CONTENTS);
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

#[test]
fn existing_archive_is_preserved_and_nothing_else_is_copied() {
    let collection = Collection::new();
    fs::write(collection.root.join("source/robocop.zip"), b"another ROM").unwrap();
    let working = collection.root.join("output/roms");
    fs::create_dir_all(&working).unwrap();
    fs::write(working.join("005.zip"), b"existing archive").unwrap();
    collection.copy_conflict("Destination already exists");
    assert_eq!(
        fs::read(working.join("005.zip")).unwrap(),
        b"existing archive"
    );
    assert!(!working.join("robocop.zip").exists());
}

#[test]
fn duplicate_archive_names_in_different_sources_are_rejected_before_copying() {
    let mut collection = Collection::new();
    let second = collection.root.join("second");
    fs::create_dir(&second).unwrap();
    fs::write(second.join("005.zip"), b"different archive").unwrap();
    collection
        .args
        .source_path
        .push(second.to_str().unwrap().into());
    collection.copy_conflict("Conflicting destination");
    assert!(!collection.root.join("output/roms").exists());
    assert_eq!(
        fs::read(second.join("005.zip")).unwrap(),
        b"different archive"
    );
}

#[test]
fn case_only_duplicate_archive_names_are_rejected_on_every_platform() {
    let mut collection = Collection::new();
    let second = collection.root.join("second");
    fs::create_dir(&second).unwrap();
    fs::write(second.join("005.ZIP"), b"case variant").unwrap();
    collection
        .args
        .source_path
        .push(second.to_str().unwrap().into());
    collection.copy_conflict("Conflicting destination");
    assert!(!collection.root.join("output/roms").exists());
}

#[test]
fn existing_case_variant_archive_is_preserved() {
    let collection = Collection::new();
    let working = collection.root.join("output/roms");
    fs::create_dir_all(&working).unwrap();
    fs::write(working.join("005.ZIP"), b"existing case variant").unwrap();
    collection.copy_conflict("Destination already exists");
    assert_eq!(
        fs::read(working.join("005.ZIP")).unwrap(),
        b"existing case variant"
    );
    assert_eq!(fs::read_dir(&working).unwrap().count(), 1);
}

#[test]
fn existing_case_variant_category_directory_cannot_hide_a_conflict() {
    let collection = Collection::new();
    let working = collection.root.join("output/ROMS");
    fs::create_dir_all(&working).unwrap();
    fs::write(working.join("005.zip"), b"existing archive").unwrap();
    collection.copy_conflict("Destination");
    assert_eq!(
        fs::read(working.join("005.zip")).unwrap(),
        b"existing archive"
    );
    assert_eq!(
        fs::read_dir(collection.root.join("output"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn existing_chd_directory_is_preserved_without_merging() {
    let collection = Collection::new();
    let source = collection.root.join("source/area51");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("disk.chd"), b"new disk").unwrap();
    let destination = collection.root.join("output/roms/area51");
    fs::create_dir_all(&destination).unwrap();
    fs::write(destination.join("keep.chd"), b"existing disk").unwrap();
    collection.copy_conflict("Destination already exists");
    assert_eq!(
        fs::read(destination.join("keep.chd")).unwrap(),
        b"existing disk"
    );
    assert!(!destination.join("disk.chd").exists());
    assert!(!collection.root.join("output/roms/005.zip").exists());
}

#[test]
fn duplicate_chd_directories_are_rejected_even_with_disjoint_files() {
    let mut collection = Collection::new();
    let first = collection.root.join("source/area51");
    let second = collection.root.join("second");
    fs::create_dir(&first).unwrap();
    fs::create_dir_all(second.join("area51")).unwrap();
    fs::write(first.join("first.chd"), b"first disk").unwrap();
    fs::write(second.join("area51/second.chd"), b"second disk").unwrap();
    collection
        .args
        .source_path
        .push(second.to_str().unwrap().into());
    collection.copy_conflict("Conflicting destination");
    assert!(!collection.root.join("output/roms").exists());
}

#[test]
fn a_file_blocking_a_category_directory_is_rejected_before_copying() {
    let collection = Collection::new();
    fs::create_dir(collection.root.join("output")).unwrap();
    fs::write(collection.root.join("output/rest"), b"keep file").unwrap();
    collection.copy_conflict("Output directory is blocked");
    assert_eq!(
        fs::read(collection.root.join("output/rest")).unwrap(),
        b"keep file"
    );
    assert!(!collection.root.join("output/roms").exists());
}

#[test]
fn existing_selected_names_file_stops_the_copy() {
    let collection = Collection::new();
    fs::create_dir(collection.root.join("output")).unwrap();
    fs::write(collection.root.join("output/selected.txt"), b"keep list").unwrap();
    let error = roms_curator::run(&collection.args)
        .unwrap()
        .copy_roms(&collection.args)
        .unwrap_err()
        .to_string();
    assert!(error.contains("Destination already exists"), "{error}");
    collection
        .cli()
        .assert()
        .failure()
        .stdout(predicates::str::contains("Destination already exists"));
    assert_eq!(
        fs::read(collection.root.join("output/selected.txt")).unwrap(),
        b"keep list"
    );
    assert!(!collection.root.join("output/roms").exists());
    assert!(!collection.root.join("output/rest").exists());
}

#[test]
fn report_cannot_be_the_selected_names_file() {
    let mut collection = Collection::new();
    collection.args.report_path = collection
        .root
        .join("output/selected.txt")
        .to_str()
        .unwrap()
        .into();
    let roms = roms_curator::run(&collection.args).unwrap();
    let error = roms.copy_roms(&collection.args).unwrap_err().to_string();
    assert!(
        error.contains("Report path and selected names path must be different files."),
        "{error}"
    );
    assert!(!collection.root.join("output").exists());
}

#[test]
fn requested_report_cannot_occupy_an_output_directory_or_archive() {
    for target in [
        "output/roms",
        "output/roms/notes.md",
        "output/rest/notes.md",
        "output/roms/area51/disk.md",
    ] {
        let mut collection = Collection::new();
        fs::create_dir(collection.root.join("source/area51")).unwrap();
        collection.args.report_path = collection.root.join(target).to_str().unwrap().into();
        // The CLI also requires a .md suffix, so exercise this library conflict directly.
        let roms = roms_curator::run(&collection.args).unwrap();
        let error = roms.copy_roms(&collection.args).unwrap_err().to_string();
        assert!(error.contains("Report path conflicts"), "{error}");
        assert!(!collection.root.join("output").exists());
    }
}

#[test]
fn subset_exclusions_do_not_cause_spurious_conflicts() {
    let mut collection = Collection::new();
    let second = collection.root.join("second");
    fs::create_dir(&second).unwrap();
    fs::write(second.join("005.zip"), b"excluded duplicate").unwrap();
    fs::write(collection.root.join("source/robocop.zip"), CONTENTS).unwrap();
    collection
        .args
        .source_path
        .push(second.to_str().unwrap().into());
    collection.args.subset_start = "r".into();
    collection.cli().assert().success();
    assert_eq!(
        fs::read(collection.root.join("output/roms/robocop.zip")).unwrap(),
        CONTENTS
    );
    assert!(!collection.root.join("output/roms/005.zip").exists());
}

#[test]
fn successful_nested_chd_copy_preserves_bytes_and_leaves_no_temporary_files() {
    let collection = Collection::new();
    fs::create_dir_all(collection.root.join("source/area51/nested/empty")).unwrap();
    fs::write(
        collection.root.join("source/area51/nested/disk.chd"),
        b"disk contents",
    )
    .unwrap();
    fs::write(collection.root.join("source/area51/empty.chd"), b"").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            collection.root.join("source/area51/nested/disk.chd"),
            fs::Permissions::from_mode(0o640),
        )
        .unwrap();
        fs::set_permissions(
            collection.root.join("source/005.zip"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
    }
    collection.cli().assert().success();
    let destination = collection.root.join("output/roms/area51");
    assert_eq!(
        fs::read(destination.join("nested/disk.chd")).unwrap(),
        b"disk contents"
    );
    assert_eq!(fs::read(destination.join("empty.chd")).unwrap(), b"");
    assert!(destination.join("nested/empty").is_dir());
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 2);
    assert_eq!(fs::read_dir(destination.join("nested")).unwrap().count(), 2);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(destination.join("nested/disk.chd"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
        assert_eq!(
            fs::metadata(collection.root.join("output/roms/005.zip"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
    }
    assert_eq!(
        fs::read(collection.root.join("source/area51/nested/disk.chd")).unwrap(),
        b"disk contents"
    );
}

#[cfg(unix)]
struct RestorePermissions {
    path: PathBuf,
    permissions: fs::Permissions,
}

#[cfg(unix)]
impl Drop for RestorePermissions {
    fn drop(&mut self) {
        fs::set_permissions(&self.path, self.permissions.clone()).unwrap();
    }
}

#[cfg(unix)]
fn block_working_directory(collection: &Collection) -> Option<RestorePermissions> {
    use std::os::unix::fs::PermissionsExt;
    let path = collection.root.join("output/roms");
    fs::create_dir_all(&path).unwrap();
    let restore = RestorePermissions {
        permissions: fs::metadata(&path).unwrap().permissions(),
        path,
    };
    fs::set_permissions(&restore.path, fs::Permissions::from_mode(0o555)).unwrap();
    // Root or filesystems without Unix permissions cannot exercise this failure.
    let probe = restore.path.join("permission-probe");
    if fs::write(&probe, b"probe").is_ok() {
        fs::remove_file(probe).unwrap();
        eprintln!("Skipping permission failure check: filesystem permits writes to mode 0555");
        return None;
    }
    Some(restore)
}

#[cfg(unix)]
#[test]
fn library_records_copy_paths_and_cause_and_retains_successful_copies() {
    let collection = Collection::new();
    fs::write(collection.root.join("source/100lions.zip"), b"other ROM").unwrap();
    let Some(_restore) = block_working_directory(&collection) else {
        return;
    };
    let report = roms_curator::run(&collection.args)
        .unwrap()
        .copy_roms(&collection.args)
        .unwrap();
    assert!(!report.all_ok);
    assert_eq!(report.roms_working[0].status, OperationStatus::Failed);
    let error = report.roms_working[0].error.as_ref().unwrap();
    assert!(
        error.contains(collection.root.join("source/005.zip").to_str().unwrap()),
        "{error}"
    );
    assert!(
        error.contains(
            collection
                .root
                .join("output/roms/005.zip")
                .to_str()
                .unwrap()
        ),
        "{error}"
    );
    assert!(
        error.contains("denied") || error.contains("permitted"),
        "{error}"
    );
    assert_eq!(report.roms_other[0].status, OperationStatus::Copied);
    assert!(report.roms_other[0].error.is_none());
    report.to_file(&collection.args.report_path).unwrap();
    let contents = fs::read_to_string(&collection.args.report_path).unwrap();
    assert!(contents.contains(error));
    assert!(contents.contains("### Copied to rest folder"));
    assert!(contents.contains("### Failed copies to required folder (roms)"));
    assert!(contents.contains("- Files copied: 1"));
    assert!(contents.contains("- Failed copies: 1"));
    let copied_other = contents
        .split_once("### Copied to rest folder")
        .unwrap()
        .1
        .split_once("### Failed copies to required folder (roms)")
        .unwrap()
        .0;
    let failed_working = contents
        .split_once("### Failed copies to required folder (roms)")
        .unwrap()
        .1
        .split_once("### Failed copies to rest folder")
        .unwrap()
        .0;
    assert!(copied_other.contains("100lions.zip"));
    assert!(!copied_other.contains("005.zip"));
    assert!(failed_working.contains("005.zip"));
    assert!(!failed_working.contains("100lions.zip"));
    assert_eq!(
        fs::read(collection.root.join("output/rest/100lions.zip")).unwrap(),
        b"other ROM"
    );
    assert_eq!(
        fs::read(collection.root.join("source/005.zip")).unwrap(),
        CONTENTS
    );
    assert_eq!(
        fs::read_dir(collection.root.join("output/roms"))
            .unwrap()
            .count(),
        0
    );
    assert!(!collection.root.join("output/selected.txt").exists());
}

#[cfg(unix)]
#[test]
fn cli_copy_failures_return_nonzero_with_and_without_a_report() {
    for requested_report in [false, true] {
        let mut collection = Collection::new();
        fs::write(collection.root.join("source/100lions.zip"), b"other ROM").unwrap();
        if !requested_report {
            collection.args.report_path.clear();
        }
        let Some(_restore) = block_working_directory(&collection) else {
            return;
        };
        let assertion = collection
            .cli()
            .assert()
            .failure()
            .stdout(predicates::str::contains("Failed to copy"))
            .stdout(predicates::str::contains("005.zip"))
            .stderr(predicates::str::contains("panicked").not());
        assert!(!String::from_utf8_lossy(&assertion.get_output().stdout).contains("panicked"));
        assert_eq!(
            fs::read(collection.root.join("output/rest/100lions.zip")).unwrap(),
            b"other ROM"
        );
        assert_eq!(
            fs::read(collection.root.join("source/005.zip")).unwrap(),
            CONTENTS
        );
        if requested_report {
            let contents = fs::read_to_string(&collection.args.report_path).unwrap();
            assert!(contents.contains("All OK: false"));
            assert!(contents.contains("### Copied to rest folder"));
            assert!(contents.contains("### Failed copies to required folder (roms)"));
            assert!(contents.contains("- Files copied: 1"));
            assert!(contents.contains("- Failed copies: 1"));
            assert!(contents.contains("Failed to copy"));
            let failed_working = contents
                .split_once("### Failed copies to required folder (roms)")
                .unwrap()
                .1
                .split_once("### Failed copies to rest folder")
                .unwrap()
                .0;
            assert!(failed_working.contains("005.zip"));
            assert!(!failed_working.contains("100lions.zip"));
        } else {
            assert!(!collection.root.join("report.md").exists());
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct TreeEntry {
    path: PathBuf,
    contents: Option<Vec<u8>>,
    modified: std::time::SystemTime,
    readonly: bool,
}

fn tree_snapshot(root: &Path) -> Vec<TreeEntry> {
    fn collect(root: &Path, path: &Path, entries: &mut Vec<TreeEntry>) {
        let metadata = fs::metadata(path).unwrap();
        entries.push(TreeEntry {
            path: path.strip_prefix(root).unwrap().into(),
            contents: if metadata.is_file() {
                Some(fs::read(path).unwrap())
            } else {
                None
            },
            modified: metadata.modified().unwrap(),
            readonly: metadata.permissions().readonly(),
        });
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                collect(root, &entry.unwrap().path(), entries);
            }
        }
    }
    let mut entries = Vec::new();
    collect(root, root, &mut entries);
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    entries
}

#[test]
fn simulation_leaves_missing_output_ancestors_absent_with_or_without_a_report() {
    for requested_report in [false, true] {
        let mut collection = Collection::new();
        collection.args.simulation = true;
        collection.args.destination_path = collection
            .root
            .join("new/deep/output")
            .to_str()
            .unwrap()
            .into();
        if !requested_report {
            collection.args.report_path.clear();
        }
        fs::create_dir_all(collection.root.join("source/area51/nested")).unwrap();
        fs::write(
            collection.root.join("source/area51/nested/disk.chd"),
            b"disk bytes",
        )
        .unwrap();
        let before = tree_snapshot(&collection.root.join("source"));
        let report = roms_curator::run(&collection.args)
            .unwrap()
            .copy_roms(&collection.args)
            .unwrap();
        assert!(report.simulation && report.all_ok);
        assert!(report
            .roms_working
            .iter()
            .all(|entry| entry.status == OperationStatus::Planned));
        assert!(!collection.root.join("new").exists());
        collection
            .cli()
            .assert()
            .success()
            .stdout(predicates::str::contains("Planning from source"))
            .stdout(predicates::str::contains("entries Planned"));
        assert!(!collection.root.join("new").exists());
        assert_eq!(tree_snapshot(&collection.root.join("source")), before);
        assert_eq!(
            fs::read_dir(&collection.root).unwrap().count(),
            if requested_report { 2 } else { 1 }
        );
        if requested_report {
            let contents = fs::read_to_string(&collection.args.report_path).unwrap();
            assert!(contents.contains("Planned for required folder (roms)"));
            assert!(contents.contains("area51 (disk)"));
            assert!(contents.contains("Files copied: 0"));
            assert!(contents.contains("Failed copies: 0"));
            assert!(!contents.contains("### Copied to"));
        }
    }
}

#[test]
fn simulation_leaves_existing_source_and_output_trees_unchanged() {
    let mut collection = Collection::new();
    collection.args.simulation = true;
    fs::create_dir_all(collection.root.join("output/roms")).unwrap();
    fs::create_dir_all(collection.root.join("output/custom")).unwrap();
    fs::write(
        collection.root.join("output/roms/unselected.zip"),
        b"keep archive",
    )
    .unwrap();
    fs::write(
        collection.root.join("output/custom/notes.txt"),
        b"keep notes",
    )
    .unwrap();
    let source = tree_snapshot(&collection.root.join("source"));
    let output = tree_snapshot(&collection.root.join("output"));
    let report = roms_curator::run(&collection.args)
        .unwrap()
        .copy_roms(&collection.args)
        .unwrap();
    assert_eq!(report.roms_working[0].status, OperationStatus::Planned);
    collection.cli().assert().success();
    assert_eq!(tree_snapshot(&collection.root.join("source")), source);
    assert_eq!(tree_snapshot(&collection.root.join("output")), output);
    assert!(Path::new(&collection.args.report_path).is_file());
}

#[test]
fn simulation_still_rejects_conflicts_without_writing_a_report_or_output() {
    let mut collection = Collection::new();
    collection.args.simulation = true;
    fs::create_dir_all(collection.root.join("output/roms")).unwrap();
    fs::write(
        collection.root.join("output/roms/005.zip"),
        b"existing archive",
    )
    .unwrap();
    let before = tree_snapshot(&collection.root);
    collection.copy_conflict("Destination already exists");
    assert_eq!(tree_snapshot(&collection.root), before);
}

#[test]
fn simulation_reports_must_be_outside_existing_and_future_output_trees() {
    for existing in [false, true] {
        for report in ["output/notes.md", "OUTPUT/notes.md"] {
            let mut collection = Collection::new();
            collection.args.simulation = true;
            collection.args.report_path = collection.root.join(report).to_str().unwrap().into();
            if existing {
                fs::create_dir(collection.root.join("output")).unwrap();
                fs::write(collection.root.join("output/keep"), b"keep bytes").unwrap();
            }
            let before = tree_snapshot(&collection.root);
            let error = copy_all_to_rest(Roms::new(), &collection.args)
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("Simulation report must be outside"),
                "{error}"
            );
            collection
                .cli()
                .assert()
                .failure()
                .stdout(predicates::str::contains(
                    "Simulation report must be outside",
                ));
            assert_eq!(tree_snapshot(&collection.root), before);
        }
    }
}

#[test]
fn simulation_rejects_a_case_alias_of_the_future_output_root_itself() {
    let mut collection = Collection::new();
    collection.args.simulation = true;
    collection.args.destination_path = collection.root.join("Öutput.md").to_str().unwrap().into();
    collection.args.report_path = collection.root.join("öutput.md").to_str().unwrap().into();
    let before = tree_snapshot(&collection.root);
    collection.rejected();
    assert_eq!(tree_snapshot(&collection.root), before);
}

#[test]
fn simulation_report_failure_keeps_trees_unchanged_and_prints_a_planned_fallback() {
    let mut collection = Collection::new();
    collection.args.simulation = true;
    collection.args.destination_path = collection.root.join("new/output").to_str().unwrap().into();
    collection.args.report_path = collection
        .root
        .join("missing-report/report.md")
        .to_str()
        .unwrap()
        .into();
    let before = tree_snapshot(&collection.root);
    let assertion = collection
        .cli()
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "Simulation: Planned operations only",
        ))
        .stderr(predicates::str::contains("Files copied: 0"));
    assert!(!String::from_utf8_lossy(&assertion.get_output().stdout)
        .contains("Any ROM files already copied"));
    assert_eq!(tree_snapshot(&collection.root), before);
}

#[cfg(unix)]
#[test]
fn simulation_does_not_require_writable_source_or_output_directories() {
    use std::os::unix::fs::PermissionsExt;
    let mut collection = Collection::new();
    collection.args.simulation = true;
    collection.args.report_path.clear();
    let Some(_output_permissions) = block_working_directory(&collection) else {
        return;
    };
    let source = collection.root.join("source");
    let _source_permissions = RestorePermissions {
        permissions: fs::metadata(&source).unwrap().permissions(),
        path: source.clone(),
    };
    fs::set_permissions(&source, fs::Permissions::from_mode(0o555)).unwrap();
    collection.args.destination_path = collection
        .root
        .join("output/roms/new/deep")
        .to_str()
        .unwrap()
        .into();
    let before_source = tree_snapshot(&source);
    let before_output = tree_snapshot(&collection.root.join("output"));
    let report = roms_curator::run(&collection.args)
        .unwrap()
        .copy_roms(&collection.args)
        .unwrap();
    assert_eq!(report.roms_working[0].status, OperationStatus::Planned);
    collection.cli().assert().success();
    assert_eq!(tree_snapshot(&source), before_source);
    assert_eq!(
        tree_snapshot(&collection.root.join("output")),
        before_output
    );
}

#[cfg(any(unix, windows))]
#[test]
fn simulation_rejects_a_report_inside_an_output_root_alias() {
    let mut collection = Collection::new();
    collection.args.simulation = true;
    fs::create_dir(collection.root.join("output")).unwrap();
    link_directory(
        &collection.root.join("output"),
        &collection.root.join("output-alias"),
    );
    collection.args.report_path = collection
        .root
        .join("output-alias/notes.md")
        .to_str()
        .unwrap()
        .into();
    collection.rejected();
    assert!(!collection.root.join("output/notes.md").exists());
}
