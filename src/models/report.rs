use chrono;
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, LineWriter, Write};
use std::path::Path;

use tempfile::Builder;

/// Outcome recorded for one source entry.
///
/// [`Ignored`](Self::Ignored) is the default. A failed copy has to set
/// [`Failed`](Self::Failed). An entry left at the default is omitted from the
/// failure section and from the failed-copy count.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationStatus {
    /// Selected during simulation. This is neither a copy nor a failure.
    Planned,
    /// The archive or directory was copied.
    Copied,
    /// The copy was attempted and failed.
    Failed,
    /// The source entry was not selected.
    #[default]
    Ignored,
}

/// Kind of source entry recorded in the export report.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportMediaKind {
    /// ROM archive or other non-directory source file (for example `.zip`), in `roms` or `rest`.
    Archive,
    /// CHD directory for a machine that has disk metadata in MAME XML.
    DiskDirectory,
    /// Other directory source entry (not classified as a CHD directory).
    #[default]
    OtherFile,
}

#[derive(Default, Debug, Clone)]
pub struct ReportDetailEntry {
    pub rom_name: String,
    /// Defaults to [`OperationStatus::Ignored`]. Failures must set
    /// [`OperationStatus::Failed`].
    pub status: OperationStatus,
    pub media_kind: ExportMediaKind,
    /// Operation paths and cause when a copy fails; absent for successful/ignored entries.
    pub error: Option<String>,
}

/// One CHD file a selected game needs that is not in the source.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MissingChdEntry {
    /// Selected game that needs this disk.
    pub game: String,
    /// Machine directory that should store the CHD (filename stem).
    pub machine: String,
    /// Disk name from MAME metadata (CHD filename stem).
    pub disk: String,
    /// Why this file is required. Separate from emulation status.
    pub reason: String,
}

/// Stores report data
#[derive(Default, Debug)]
pub struct Report {
    /// True for a read-only simulation; selected entries have status Planned.
    pub simulation: bool,
    /// Selected games inside the configured name range.
    pub selected_games: usize,
    pub total_working: i32,
    pub total_other: i32,
    pub roms_working: Vec<ReportDetailEntry>,
    pub roms_other: Vec<ReportDetailEntry>,
    pub ignored_roms: Vec<ReportDetailEntry>,
    /// Set when every ROM copy succeeded but `selected.txt` could not be published.
    pub names_error: Option<String>,
    /// CHDs required by selected games but absent from the source.
    pub missing_chds: Vec<MissingChdEntry>,
    /// False if a copy failed or `selected.txt` could not be published. A
    /// successful simulation does not verify write access or available space.
    pub all_ok: bool,
}

fn write_all_roms(
    writer: &mut impl Write,
    roms: &[ReportDetailEntry],
    status: OperationStatus,
) -> io::Result<()> {
    writer.write_all(b"<details>\n  <summary>roms</summary>\n\n```text\n")?;
    for entry in roms.iter().filter(|entry| entry.status == status) {
        let label = match entry.media_kind {
            ExportMediaKind::Archive => "(archive)",
            ExportMediaKind::DiskDirectory => "(disk)",
            ExportMediaKind::OtherFile => "",
        };
        if label.is_empty() {
            writeln!(writer, "{}", entry.rom_name)?;
        } else {
            writeln!(writer, "{} {label}", entry.rom_name)?;
        }
        if let Some(error) = &entry.error {
            writeln!(writer, "  Error: {error}")?;
        }
    }
    writer.write_all(b"```\n\n</details>\n\n")
}

fn publish_report(
    path: &str,
    write: impl FnOnce(&mut File) -> Result<(), Box<dyn Error>>,
) -> Result<bool, Box<dyn Error>> {
    match fs::symlink_metadata(path) {
        Ok(_) => return Err(format!("Cannot create report {path}: file already exists").into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(format!("Cannot create report {path}: {error}").into()),
    }
    let parent = Path::new(path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = Builder::new()
        .prefix(".roms-curator-report-")
        .tempfile_in(parent)
        .map_err(|error| format!("Cannot create report {path}: {error}"))?;
    write(temporary.as_file_mut())
        .map_err(|error| format!("Cannot write report {path}: {error}"))?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| format!("Cannot publish report {path}: {}", error.error))?;
    Ok(true)
}

impl Report {
    pub fn new() -> Report {
        Default::default()
    }

    /// Returns `Ok(true)` after a flushed write and publication without replacement.
    /// On failure, the error string includes the report path and cause. Failed
    /// writes do not leave a partial report at the requested path.
    pub fn to_file(&self, path: &str) -> Result<bool, Box<dyn Error>> {
        publish_report(path, |file| self.write_to(LineWriter::new(file)))
    }

    /// Write and flush the Markdown report to a caller-provided stream.
    /// Returns an error if writing or flushing fails.
    pub fn write_to(&self, mut writer: impl Write) -> Result<(), Box<dyn Error>> {
        let title = format!(
            "{}{}{}\n",
            "# Roms-Curator report [",
            chrono::offset::Local::now(),
            "]"
        );
        writer.write_all(title.as_bytes())?;
        writer.write_all(b"\n")?;
        if self.simulation {
            writer.write_all(b"Simulation: Planned operations only. No ROMs were copied.\n\n")?;
        }

        let toc = self.build_toc();

        writer.write_all(toc.as_bytes())?;

        let summary = self.build_summary();

        writer.write_all(summary.as_bytes())?;

        // write detail
        writer.write_all(b"## Detail\n\n")?;

        let (label, status) = self.success_label();
        writeln!(writer, "### {label} required folder (roms)\n")?;
        write_all_roms(&mut writer, &self.roms_working, status)?;

        writeln!(writer, "### {label} rest folder\n")?;
        write_all_roms(&mut writer, &self.roms_other, status)?;

        if !self.simulation {
            writer.write_all(b"### Failed copies to required folder (roms)\n\n")?;
            write_all_roms(&mut writer, &self.roms_working, OperationStatus::Failed)?;

            writer.write_all(b"### Failed copies to rest folder\n\n")?;
            write_all_roms(&mut writer, &self.roms_other, OperationStatus::Failed)?;
        }

        if !self.ignored_roms.is_empty() {
            writer.write_all(b"### Ignored roms\n\n")?;
            writer.write_all(b"These source entries were not copied.\n\n")?;
            write_all_roms(&mut writer, &self.ignored_roms, OperationStatus::Ignored)?;
        }

        if !self.missing_chds.is_empty() {
            writer.write_all(b"### Missing CHDs\n\n")?;
            writer.write_all(
                b"Selected games still need these CHD files. They were not found in the source.\n",
            )?;
            writer.write_all(
                b"Obtain only these disks rather than downloading a full CHD collection.\n\n",
            )?;
            writer.write_all(b"```text\n")?;
            for entry in &self.missing_chds {
                writeln!(writer, "game: {}", entry.game)?;
                writeln!(writer, "  machine folder: {}", entry.machine)?;
                writeln!(writer, "  disk: {}", entry.disk)?;
                writeln!(writer, "  reason: {}", entry.reason)?;
                writer.write_all(b"\n")?;
            }
            writer.write_all(b"```\n\n")?;
        }

        writer.flush()?;

        Ok(())
    }

    fn success_label(&self) -> (&'static str, OperationStatus) {
        if self.simulation {
            ("Planned for", OperationStatus::Planned)
        } else {
            ("Copied to", OperationStatus::Copied)
        }
    }

    fn build_toc(&self) -> String {
        let (label, _) = self.success_label();
        let anchor = if self.simulation {
            "planned-for"
        } else {
            "copied-to"
        };
        let failures = if self.simulation {
            ""
        } else {
            "  - [Failed copies to required folder (roms)](#failed-copies-to-required-folder-roms)\n  - [Failed copies to rest folder](#failed-copies-to-rest-folder)\n"
        };
        let ignored = if self.ignored_roms.is_empty() {
            ""
        } else {
            "  - [Ignored roms](#ignored-roms)\n"
        };
        let missing = if self.missing_chds.is_empty() {
            ""
        } else {
            "  - [Missing CHDs](#missing-chds)\n"
        };
        format!(
            "- [Summary](#summary)\n- [Detail](#detail)\n  - [{label} required folder (roms)](#{anchor}-required-folder-roms)\n  - [{label} rest folder](#{anchor}-rest-folder)\n{failures}{ignored}{missing}\n"
        )
    }

    fn build_summary(&self) -> String {
        let (label, status) = self.success_label();
        let (working, working_archives, working_disks) = folder_counts(&self.roms_working, status);
        let (other, other_archives, other_disks) = folder_counts(&self.roms_other, status);
        let copied = self
            .roms_working
            .iter()
            .chain(&self.roms_other)
            .filter(|entry| entry.status == OperationStatus::Copied)
            .count();
        let failed = self
            .roms_working
            .iter()
            .chain(&self.roms_other)
            .filter(|entry| entry.status == OperationStatus::Failed)
            .count();
        let names = match &self.names_error {
            Some(error) => format!("- Selected names: {error}\n"),
            None => String::new(),
        };
        format!(
            "## Summary\n\n- All OK: {}\n- Selected games: {}\n- {label} required folder (roms): {working} ({working_archives} archives, {working_disks} disk directories)\n- {label} rest folder: {other} ({other_archives} archives, {other_disks} disk directories)\n- Files copied: {copied}\n- Ignored source entries: {}\n- Failed copies: {failed}\n- Missing CHDs: {}\n{names}\n",
            self.all_ok,
            self.selected_games,
            self.ignored_roms.len(),
            self.missing_chds.len()
        )
    }

    pub fn add_rom_working(&mut self, value: ReportDetailEntry) -> &mut Report {
        self.roms_working.push(value);
        self
    }

    pub fn add_rom_other(&mut self, value: ReportDetailEntry) -> &mut Report {
        self.roms_other.push(value);
        self
    }

    pub fn add_ignored_rom(&mut self, value: ReportDetailEntry) -> &mut Report {
        self.ignored_roms.push(value);
        self
    }

    pub fn all_ok(&mut self, value: bool) -> &mut Report {
        self.all_ok = value;
        self
    }

    pub fn total_working(&mut self, value: i32) -> &mut Report {
        self.total_working = value;
        self
    }

    pub fn total_other(&mut self, value: i32) -> &mut Report {
        self.total_other = value;
        self
    }
}

fn folder_counts(entries: &[ReportDetailEntry], status: OperationStatus) -> (usize, usize, usize) {
    entries
        .iter()
        .filter(|entry| entry.status == status)
        .fold((0, 0, 0), |(total, archives, disks), entry| {
            let archives = archives + usize::from(entry.media_kind == ExportMediaKind::Archive);
            let disks = disks + usize::from(entry.media_kind == ExportMediaKind::DiskDirectory);
            (total + 1, archives, disks)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulation_reports_planned_entries_without_counting_them_as_copies_or_failures() {
        let mut report = Report {
            simulation: true,
            all_ok: true,
            ..Report::default()
        };
        report.add_rom_working(ReportDetailEntry {
            rom_name: "005.zip".into(),
            status: OperationStatus::Planned,
            media_kind: ExportMediaKind::Archive,
            ..ReportDetailEntry::default()
        });
        report.add_rom_other(ReportDetailEntry {
            rom_name: "disk".into(),
            status: OperationStatus::Planned,
            media_kind: ExportMediaKind::DiskDirectory,
            ..ReportDetailEntry::default()
        });
        let mut output = Vec::new();
        report.write_to(&mut output).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Simulation: Planned operations only"));
        assert!(output.contains("Planned for required folder (roms): 1 (1 archives, 0 disk directories)"));
        assert!(output.contains("Planned for rest folder: 1 (0 archives, 1 disk directories)"));
        assert!(output.contains("Files copied: 0"));
        assert!(output.contains("Failed copies: 0"));
        assert!(output.contains("005.zip (archive)"));
        assert!(output.contains("disk (disk)"));
        assert!(!output.contains("### Copied to"));
        assert!(!output.contains("### Failed copies"));
        assert!(report.simulation);
    }

    #[test]
    fn copy_report_separates_copied_failed_and_ignored_entries() {
        let mut report = Report {
            all_ok: false,
            ..Report::default()
        };
        report.add_rom_working(ReportDetailEntry {
            rom_name: "005.zip".into(),
            status: OperationStatus::Failed,
            media_kind: ExportMediaKind::Archive,
            error: Some("source/005.zip -> output/005.zip: denied".into()),
            ..ReportDetailEntry::default()
        });
        report.add_rom_other(ReportDetailEntry {
            rom_name: "100lions.zip".into(),
            status: OperationStatus::Copied,
            media_kind: ExportMediaKind::Archive,
            ..ReportDetailEntry::default()
        });
        report.add_ignored_rom(ReportDetailEntry {
            rom_name: "notes.txt".into(),
            ..ReportDetailEntry::default()
        });
        let mut output = Vec::new();
        report.write_to(&mut output).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("- Copied to required folder (roms): 0 (0 archives, 0 disk directories)"));
        assert!(output.contains("- Copied to rest folder: 1 (1 archives, 0 disk directories)"));
        assert!(output.contains("- Files copied: 1"));
        assert!(output.contains("- Ignored source entries: 1"));
        assert!(output.contains("- Failed copies: 1"));
        assert!(output.contains("### Copied to required folder (roms)"));
        assert!(output.contains("### Copied to rest folder"));
        assert!(output.contains("### Failed copies to required folder (roms)"));
        assert!(output.contains("### Failed copies to rest folder"));
        assert!(!output.contains("### Planned for"));

        let copied_other = section(
            &output,
            "### Copied to rest folder",
            "### Failed copies to required folder (roms)",
        );
        let failed_working = section(
            &output,
            "### Failed copies to required folder (roms)",
            "### Failed copies to rest folder",
        );
        let ignored = section(&output, "### Ignored roms", "");
        assert!(copied_other.contains("100lions.zip"));
        assert!(!copied_other.contains("005.zip"));
        assert!(failed_working.contains("005.zip"));
        assert!(failed_working.contains("denied"));
        assert!(!failed_working.contains("100lions.zip"));
        assert!(ignored.contains("notes.txt"));
        assert!(!ignored.contains("005.zip"));
        assert!(!ignored.contains("100lions.zip"));
    }

    #[test]
    fn missing_chd_section_lists_each_required_disk() {
        let mut report = Report::default();
        report.missing_chds.push(MissingChdEntry {
            game: "area51".into(),
            machine: "Area51".into(),
            disk: "area51".into(),
            reason: "Own disk".into(),
        });
        let mut output = Vec::new();
        report.write_to(&mut output).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("### Missing CHDs"));
        assert!(output.contains("- Missing CHDs: 1"));
        assert!(output.contains("game: area51"));
        assert!(output.contains("machine folder: Area51"));
        assert!(output.contains("disk: area51"));
        assert!(output.contains("reason: Own disk"));
        assert!(output.contains("full CHD collection"));
    }

    #[test]
    fn a_names_file_failure_is_listed_without_counting_as_a_failed_copy() {
        let mut report = Report {
            all_ok: false,
            names_error: Some("Cannot publish destination/selected.txt: denied".into()),
            ..Report::default()
        };
        report.add_rom_working(ReportDetailEntry {
            rom_name: "005.zip".into(),
            status: OperationStatus::Copied,
            ..ReportDetailEntry::default()
        });
        let mut output = Vec::new();
        report.write_to(&mut output).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("- All OK: false"));
        assert!(output.contains("- Failed copies: 0"));
        assert!(
            output.contains("- Selected names: Cannot publish destination/selected.txt: denied")
        );
    }

    fn section<'a>(report: &'a str, start: &str, end: &str) -> &'a str {
        let rest = &report[report.find(start).unwrap() + start.len()..];
        match rest.find(end) {
            Some(end_at) if !end.is_empty() => &rest[..end_at],
            _ => rest,
        }
    }

    #[test]
    fn detail_write_failure_is_returned_without_panicking() {
        struct FailingWriter;
        impl Write for FailingWriter {
            fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
                if buffer
                    .windows(b"broken.zip".len())
                    .any(|part| part == b"broken.zip")
                {
                    return Err(io::Error::other("injected detail write failure"));
                }
                Ok(buffer.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut report = Report::new();
        report.add_rom_working(ReportDetailEntry {
            rom_name: "broken.zip".into(),
            status: OperationStatus::Copied,
            ..ReportDetailEntry::default()
        });
        let error = report.write_to(FailingWriter).unwrap_err();
        assert!(error.to_string().contains("injected detail write failure"));
    }

    #[test]
    fn final_flush_failure_is_returned_without_publishing_a_partial_report() {
        struct FailingFlush<'a>(&'a mut File);
        impl Write for FailingFlush<'_> {
            fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
                self.0.write(buffer)
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::Error::other("injected flush failure"))
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.md");
        let error = publish_report(path.to_str().unwrap(), |file| {
            Report::new().write_to(FailingFlush(file))
        })
        .unwrap_err();
        assert!(error.to_string().contains("injected flush failure"));
        assert!(!path.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn partial_report_write_failure_cleans_up_and_allows_a_later_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.md");
        let error = publish_report(path.to_str().unwrap(), |file| {
            file.write_all(b"partial report")?;
            Err("injected report write failure".into())
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains(path.to_str().unwrap()), "{error}");
        assert!(error.contains("injected report write failure"), "{error}");
        assert!(!path.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
        assert!(Report::new().to_file(path.to_str().unwrap()).unwrap());
        assert!(fs::read_to_string(&path)
            .unwrap()
            .contains("# Roms-Curator report"));
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn an_existing_report_is_not_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.md");
        fs::write(&path, b"keep existing report").unwrap();
        let error = Report::new().to_file(path.to_str().unwrap()).unwrap_err();
        assert!(error.to_string().contains("already exists"));
        assert_eq!(fs::read(&path).unwrap(), b"keep existing report");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn report_created_during_writing_is_preserved_and_staging_is_removed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.md");
        let error = publish_report(path.to_str().unwrap(), |file| {
            file.write_all(b"new report")?;
            file.flush()?;
            fs::write(&path, b"keep raced report")?;
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("Cannot publish report"));
        assert_eq!(fs::read(&path).unwrap(), b"keep raced report");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn report_creation_failure_includes_the_report_path_and_cause() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing/report.md");
        let cause = File::create(&path).unwrap_err().to_string();
        let error = Report::new()
            .to_file(path.to_str().unwrap())
            .unwrap_err()
            .to_string();
        assert!(error.contains(path.to_str().unwrap()), "{error}");
        assert!(error.contains(&cause), "{error}");
    }
}
