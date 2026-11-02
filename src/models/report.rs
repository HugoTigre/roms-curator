use chrono;
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, LineWriter, Write};
use std::path::Path;

use tempfile::Builder;

#[derive(Default, Debug, Clone)]
pub struct ReportDetailEntry {
    pub rom_name: String,
    pub moved: bool,
    pub is_chd: bool,
    /// Operation paths and cause when a copy fails; absent for successful/ignored entries.
    pub error: Option<String>,
}

/// Stores report data
#[derive(Default, Debug)]
pub struct Report {
    pub total_working: i32,
    pub total_other: i32,
    pub source_dir: String,
    pub destination_dir: String,
    pub roms_working: Vec<ReportDetailEntry>,
    // name of rom and if the move was successful
    pub roms_other: Vec<ReportDetailEntry>,
    pub ignored_roms: Vec<ReportDetailEntry>,
    /// False if at least one required copy failed.
    pub all_ok: bool,
}

// Adds extension to LineWriter<File> to reduce duplicated code
pub trait LineWriterExt {
    fn write_all_roms(
        self,
        roms: &[ReportDetailEntry],
        moved: bool,
    ) -> Result<Box<LineWriter<File>>, Box<dyn Error>>;
}

impl LineWriterExt for LineWriter<File> {
    fn write_all_roms(
        mut self,
        roms: &[ReportDetailEntry],
        moved: bool,
    ) -> Result<Box<LineWriter<File>>, Box<dyn Error>> {
        write_all_roms(&mut self, roms, moved)?;
        Ok(Box::new(self))
    }
}

fn write_all_roms(
    writer: &mut impl Write,
    roms: &[ReportDetailEntry],
    moved: bool,
) -> io::Result<()> {
    writer.write_all(b"<details>\n  <summary>roms</summary>\n\n```text\n")?;
    for entry in roms.iter().filter(|entry| entry.moved == moved) {
        let is_chd = if entry.is_chd { "(chd)" } else { "" };
        writeln!(writer, "{} {}", entry.rom_name, is_chd)?;
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

        let toc = Self::build_toc()?;

        writer.write_all(toc.as_bytes())?;

        let summary = Self::build_summary(self)?;

        writer.write_all(summary.as_bytes())?;

        // write detail
        writer.write_all(b"## Detail\n\n")?;

        writer.write_all(b"### Moved to Working folder\n\n")?;
        write_all_roms(&mut writer, &self.roms_working, true)?;

        writer.write_all(b"### Moved to Other folder\n\n")?;
        write_all_roms(&mut writer, &self.roms_other, true)?;

        writer.write_all(b"### Failed moving to Working folder\n\n")?;
        write_all_roms(&mut writer, &self.roms_working, false)?;

        writer.write_all(b"### Failed moving to Other folder\n\n")?;
        write_all_roms(&mut writer, &self.roms_other, false)?;

        writer.write_all(b"### Ignored roms\n\n")?;
        writer.write_all(
            b"Files included here most likely means there are roms files in your collection\n",
        )?;
        writer.write_all(
            b"that are not found in mame.xml file or you have non-rom files in your collection.\n",
        )?;
        writer.write_all(b"You might be using different rom collection and mame versions.\n")?;
        writer.write_all(
            b"Since these roms are not in the database, there is no way to know if it's a normal\n",
        )?;
        writer.write_all(
            b"rom or a CHD file (folders should indicate chd while zip file should be a rom).\n\n",
        )?;
        write_all_roms(&mut writer, &self.ignored_roms, false)?;

        writer.flush()?;

        Ok(())
    }

    fn build_toc() -> Result<String, Box<dyn Error>> {
        let toc = format!(
            "{}{}{}{}{}{}{}{}",
            "- [Summary](#summary)\n",
            "- [Detail](#detail)\n",
            "  - [Moved to Working folder](#moved-to-working-folder)\n",
            "  - [Moved to Other folder](#moved-to-other-folder)\n",
            "  - [Failed moving to Working folder](#failed-moving-to-working-folder)\n",
            "  - [Failed moving to Other folder](#failed-moving-to-other-folder)\n",
            "  - [Ignored roms](#ignored-roms)\n",
            "\n"
        );

        Ok(toc)
    }

    fn build_summary(report: &Report) -> Result<String, Box<dyn Error>> {
        let moved_to_working_folder = report
            .roms_working
            .iter()
            .filter(|entry| entry.moved)
            .count();
        let moved_to_working_folder_chd = report
            .roms_working
            .iter()
            .filter(|entry| entry.moved && entry.is_chd)
            .count();

        let moved_to_other_folder = report.roms_other.iter().filter(|entry| entry.moved).count();
        let moved_to_other_folder_chd = report
            .roms_other
            .iter()
            .filter(|entry| entry.moved && entry.is_chd)
            .count();

        let ignored_roms = report.ignored_roms.len();

        let working_folders_entry = format!(
            "{}{}{}{}{}",
            "\n- Roms moved to working folders: ",
            moved_to_working_folder,
            " (",
            moved_to_working_folder_chd,
            " CHDs)"
        );
        let other_folders_entry = format!(
            "{}{}{}{}{}",
            "\n- Roms moved to other folders: ",
            moved_to_other_folder,
            " (",
            moved_to_other_folder_chd,
            " CHDs)"
        );
        let ignored_roms_entry = format!("{}{}", "\n- Ignored roms: ", ignored_roms);

        let roms_failed_to_move = report.roms_working.len() + report.roms_other.len()
            - moved_to_working_folder
            - moved_to_other_folder;

        let summary = format!(
            "{}{}{}{}{}{}{}{}{}{}{}{}",
            "## Summary",
            "\n\n- All OK: ",
            report.all_ok,
            " **",
            working_folders_entry,
            other_folders_entry,
            ignored_roms_entry,
            "\n- Roms failed to moved: ",
            roms_failed_to_move,
            "\n\n",
            "** (does not consider ignored_roms)",
            "\n\n",
        );

        Ok(summary)
    }

    pub fn build(&self) -> Report {
        Report {
            total_working: self.total_working,
            total_other: self.total_other,
            source_dir: self.source_dir.to_owned(),
            destination_dir: self.destination_dir.to_owned(),
            roms_working: self.roms_working.to_owned(),
            roms_other: self.roms_other.to_owned(),
            ignored_roms: self.ignored_roms.to_owned(),
            all_ok: self.all_ok,
        }
    }

    pub fn total_working(&mut self, value: i32) -> &mut Report {
        self.total_working = value;
        self
    }

    pub fn total_other(&mut self, value: i32) -> &mut Report {
        self.total_other = value;
        self
    }

    pub fn source_dir(&mut self, value: String) -> &mut Report {
        self.source_dir = value;
        self
    }

    pub fn destination_dir(&mut self, value: String) -> &mut Report {
        self.destination_dir = value;
        self
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
            moved: true,
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
