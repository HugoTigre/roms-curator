use std::error::Error;
use std::fs::File;
use std::io::{LineWriter, Write};
use chrono;

#[derive(Default, Debug, Clone)]
pub struct ReportDetailEntry {
    pub rom_name: String,
    pub moved: bool,
    pub is_chd: bool,
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
    pub all_ok: bool,
}

// Adds extension to LineWriter<File> to reduce duplicated code
pub trait LineWriterExt {
    fn write_all_roms(self, roms: &[ReportDetailEntry], moved: bool) -> Result<Box<LineWriter<File>>, Box<dyn Error>>;
}

impl LineWriterExt for LineWriter<File> {
    fn write_all_roms(mut self, roms: &[ReportDetailEntry], moved: bool) -> Result<Box<LineWriter<File>>, Box<dyn Error>> {
        self.write_all(b"<details>\n  <summary>roms</summary>\n\n```text\n")?;

        roms.iter()
            .filter(|entry| entry.moved == moved)
            .for_each(|entry| {
                let is_chd = if entry.is_chd { "(chd)" } else { "" };
                let to_write = format!("{} {}", entry.rom_name, is_chd);
                self.write_all(to_write.as_bytes()).unwrap();
                self.write_all(b"\n").unwrap();
            });

        self.write_all(b"```\n\n</details>\n\n")?;

        Ok(Box::new(self))
    }
}

impl Report {
    pub fn new() -> Report {
        Default::default()
    }

    pub fn to_file(&self, path: &str) -> Result<bool, Box<dyn Error>> {
        let file = File::create(path)?;
        let mut writer = LineWriter::new(file);

        let title = format!("{}{}{}\n", "# Roms-Curator report [", chrono::offset::Local::now(), "]");
        writer.write_all(title.as_bytes())?;
        writer.write_all(b"\n")?;

        let toc = Self::build_toc()?;

        writer.write_all(toc.as_bytes())?;

        let summary = Self::build_summary(self)?;

        writer.write_all(summary.as_bytes())?;

        // write detail
        writer.write_all(b"## Detail\n\n")?;

        writer.write_all(b"### Moved to Working folder\n\n")?;
        let mut writer = writer.write_all_roms(&self.roms_working, true)?;

        writer.write_all(b"### Moved to Other folder\n\n")?;
        let mut writer = writer.write_all_roms(&self.roms_other, true)?;

        writer.write_all(b"### Failed moving to Working folder\n\n")?;
        let mut writer = writer.write_all_roms(&self.roms_working, false)?;

        writer.write_all(b"### Failed moving to Other folder\n\n")?;
        let mut writer = writer.write_all_roms(&self.roms_other, false)?;

        writer.flush()?;

        Ok(true)
    }

    fn build_toc() -> Result<String, Box<dyn Error>> {
        let toc = format!("{}{}{}{}{}{}{}",
                          "- [Summary](#summary)\n",
                          "- [Detail](#detail)\n",
                          "  - [Moved to Working folder](#moved-to-working-folder)\n",
                          "  - [Moved to Other folder](#moved-to-other-folder)\n",
                          "  - [Failed moving to Working folder](#failed-moving-to-working-folder)\n",
                          "  - [Failed moving to Other folder](#failed-moving-to-other-folder)\n",
                          "\n"
        );

        Ok(toc)
    }

    fn build_summary(report: &Report) -> Result<String, Box<dyn Error>> {
        let moved_to_working_folder = report.roms_working.iter().filter(|entry| entry.moved).count();
        let moved_to_working_folder_chd = report.roms_working.iter().filter(|entry| entry.moved && entry.is_chd).count();

        let moved_to_other_folder = report.roms_other.iter().filter(|entry| entry.moved).count();
        let moved_to_other_folder_chd = report.roms_other.iter().filter(|entry| entry.moved && entry.is_chd).count();

        let roms_failed_to_move = report.roms_working.iter().count() + report.roms_other.iter().count()
            - moved_to_working_folder - moved_to_other_folder;

        let working_folders_entry = format!("{}{}{}{}{}", "\n- Roms moved to working folders: ", moved_to_working_folder, " (", moved_to_working_folder_chd, " CHDs)");
        let other_folders_entry = format!("{}{}{}{}{}", "\n- Roms moved to other folders: ", moved_to_other_folder, " (", moved_to_other_folder_chd, " CHDs)");

        let summary = format!("{}{}{}{}{}{}{}{}",
                              "## Summary",
                              "\n\n- All OK: ", report.all_ok,
                              working_folders_entry,
                              other_folders_entry,
                              "\n- Roms failed to moved: ", roms_failed_to_move,
                              "\n\n"
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

    pub fn all_ok(&mut self, value: bool) -> &mut Report {
        self.all_ok = value;
        self
    }
}

