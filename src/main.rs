extern crate core;

use std::io::{self, Write};
use std::path::Path;
use std::process;
use std::process::ExitCode;

use log::{error, info};

use roms_curator::core::args::build_args;
use roms_curator::models::plan::Plan;
use roms_curator::models::report::Report;
use roms_curator::utils::set_up_logging;

fn main() -> ExitCode {
    set_up_logging();

    let args = build_args().unwrap_or_else(|err| {
        error!("Application error: {err}");
        process::exit(1);
    });

    info!("Starting roms_curator with arguments: {:?}", args);

    let roms = roms_curator::run(&args).unwrap_or_else(|err| {
        error!("Application error: {err}");
        process::exit(1);
    });

    let report = if args.plan_path.is_empty() {
        roms.copy_roms(&args)
    } else if Path::new(&args.plan_path).is_file() {
        let plan = Plan::read(Path::new(&args.plan_path)).unwrap_or_else(|err| {
            error!("{err}");
            process::exit(1);
        });
        roms.copy_with_plan(&args, &plan, None)
    } else {
        let plan = roms.plan(&args).unwrap_or_else(|err| {
            error!("Failed to build plan: {err}");
            process::exit(1);
        });
        plan.write(&args.plan_path).unwrap_or_else(|err| {
            error!("{err}");
            process::exit(1);
        });
        roms.copy_with_plan(&args, &plan, None)
    }
    .unwrap_or_else(|err| {
        error!("Failed to copy roms: {err}");
        process::exit(1);
    });

    finish(&report, &args.report_path)
}

fn finish(report: &Report, report_path: &str) -> ExitCode {
    finish_with_writer(report, report_path, io::BufWriter::new(io::stderr().lock()))
}

fn finish_with_writer(report: &Report, report_path: &str, stderr: impl Write) -> ExitCode {
    // Write the failure details before returning an unsuccessful exit status.
    if !report_path.is_empty() {
        if let Err(error) = report.to_file(report_path) {
            error!("{error}");
            if report.simulation {
                error!("Simulation did not copy any ROMs. Writing the Planned report to stderr.");
            } else {
                error!("Any ROM files already copied were kept; reruns will not replace them. Writing the report to stderr.");
            }
            if let Err(error) = report.write_to(stderr) {
                error!("Cannot write the fallback report to stderr: {error}");
            }
            return ExitCode::FAILURE;
        }
    }
    if !report.all_ok {
        if let Some(error) = &report.names_error {
            error!("Selected names were not written: {error}");
        }
        let copy_failed = report
            .roms_working
            .iter()
            .chain(&report.roms_other)
            .any(|entry| entry.status == roms_curator::models::report::OperationStatus::Failed);
        if copy_failed {
            error!("One or more ROMs failed to copy. Files already completed have been retained.");
        } else if report.names_error.is_some() {
            error!("ROM files already copied have been retained.");
        }
        return ExitCode::FAILURE;
    }
    if report.simulation {
        info!(
            "Simulation complete: {} ROMs-folder and {} rest-folder entries Planned. No ROMs were copied.",
            report.total_working, report.total_other
        );
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use roms_curator::models::report::{OperationStatus, ReportDetailEntry};
    use std::fs;

    #[test]
    fn copy_failure_writes_its_report_before_returning_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.md");
        let mut report = Report::new();
        report.add_rom_working(ReportDetailEntry {
            rom_name: "005.zip".into(),
            status: OperationStatus::Failed,
            error: Some("source/005.zip -> output/005.zip: injected copy failure".into()),
            ..ReportDetailEntry::default()
        });
        let mut stderr = Vec::new();
        assert_eq!(
            finish_with_writer(&report, path.to_str().unwrap(), &mut stderr),
            ExitCode::FAILURE
        );
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("All OK: false"));
        assert!(contents.contains("source/005.zip -> output/005.zip: injected copy failure"));
        assert_eq!(finish(&report, ""), ExitCode::FAILURE);
    }

    #[test]
    fn names_file_failure_still_writes_the_report_and_returns_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.md");
        let mut report = Report::new();
        report.all_ok(false);
        report.names_error = Some("Cannot publish selected.txt: denied".into());
        report.add_rom_working(ReportDetailEntry {
            rom_name: "005.zip".into(),
            status: OperationStatus::Copied,
            ..ReportDetailEntry::default()
        });
        let mut stderr = Vec::new();
        assert_eq!(
            finish_with_writer(&report, path.to_str().unwrap(), &mut stderr),
            ExitCode::FAILURE
        );
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("All OK: false"));
        assert!(contents.contains("Cannot publish selected.txt: denied"));
        assert!(contents.contains("- Failed copies: 0"));
        assert!(stderr.is_empty());
    }

    #[test]
    fn requested_report_failure_returns_failure_and_preserves_completed_files() {
        let directory = tempfile::tempdir().unwrap();
        let completed = directory.path().join("completed.zip");
        fs::write(&completed, b"completed ROM").unwrap();
        let mut report = Report {
            all_ok: true,
            ..Report::default()
        };
        report.add_rom_working(ReportDetailEntry {
            rom_name: "completed.zip".into(),
            status: OperationStatus::Copied,
            ..ReportDetailEntry::default()
        });
        let path = directory.path().join("missing/report.md");
        let mut stderr = Vec::new();
        assert_eq!(
            finish_with_writer(&report, path.to_str().unwrap(), &mut stderr),
            ExitCode::FAILURE
        );
        let fallback = String::from_utf8(stderr).unwrap();
        assert!(fallback.contains("# Roms-Curator report"));
        assert!(fallback.contains("All OK: true"));
        assert!(fallback.contains("completed.zip"));
        assert_eq!(fs::read(&completed).unwrap(), b"completed ROM");
        assert!(!path.exists());
        assert_eq!(finish(&report, ""), ExitCode::SUCCESS);
    }

    #[test]
    fn fallback_report_contains_copy_failure_details() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing/report.md");
        let mut report = Report::new();
        report.add_rom_working(ReportDetailEntry {
            rom_name: "005.zip".into(),
            status: OperationStatus::Failed,
            error: Some("source/005.zip -> output/005.zip: injected copy failure".into()),
            ..ReportDetailEntry::default()
        });
        let mut stderr = Vec::new();
        assert_eq!(
            finish_with_writer(&report, path.to_str().unwrap(), &mut stderr),
            ExitCode::FAILURE
        );
        let fallback = String::from_utf8(stderr).unwrap();
        assert!(fallback.contains("All OK: false"));
        assert!(fallback.contains("source/005.zip -> output/005.zip: injected copy failure"));
    }

    #[test]
    fn broken_fallback_stream_returns_failure_without_panicking() {
        struct BrokenStderr;
        impl Write for BrokenStderr {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "injected stderr failure",
                ))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing/report.md");
        let report = Report {
            all_ok: true,
            ..Report::default()
        };
        assert_eq!(
            finish_with_writer(&report, path.to_str().unwrap(), BrokenStderr),
            ExitCode::FAILURE
        );
    }
}
