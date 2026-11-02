use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::path::Path;
use std::time::Duration;
use std::{env, fs, io};

use log::{info, LevelFilter};
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;

pub fn create_dir(path: &Path, ignore_if_exists: bool) {
    match path.try_exists() {
        Ok(exists) => {
            if !exists {
                fs::create_dir_all(path).unwrap_or_else(|error| {
                    panic!("Error creating {} directory: {error}", path.display())
                });
            } else if !ignore_if_exists {
                panic!("{} directory already exists.", path.to_str().unwrap());
            }
        }
        Err(err) => {
            panic!("Error creating directory {err}");
        }
    }
}

pub fn remove_dir(path: &Path) {
    if let Ok(exists) = path.try_exists() {
        if exists {
            fs::remove_dir_all(path).expect("Error deleting folder.");
        }
    }
}

///
/// To support both '/' and '\' directory delimiters
///
pub fn sanitize_path(path: &str) -> String {
    if env::consts::OS.eq("windows") {
        path.replace('/', "\\")
    } else {
        path.replace('\\', "/")
    }
}

/// Copy a directory tree without overwriting files or merging directories.
/// `destination` must not exist, and its parent directory must exist.
/// On a later failure, already completed files and directories remain.
pub fn copy_dir_recursive(path: &Path, destination: &Path) -> io::Result<()> {
    crate::core::copying::copy_directory(path, destination)
}

pub fn set_up_logging() {
    if Path::new("logging.yaml").exists() {
        log4rs::init_file("logging.yaml", Default::default()).unwrap();
    } else {
        let stdout_appender = ConsoleAppender::builder()
            .encoder(Box::new(PatternEncoder::new(
                "{h({d(%Y-%m-%d %H:%M:%S)(utc)} - {l}: {m}{n})}",
            )))
            .build();

        let config = log4rs::Config::builder()
            .appender(Appender::builder().build("stdout", Box::new(stdout_appender)))
            .build(Root::builder().appender("stdout").build(LevelFilter::Info))
            .unwrap();

        log4rs::init_config(config).unwrap();
    }
}

///
/// Builds a new progress bar.
///
/// Applies style.
/// Refresh terminal once per second.
///
/// # Arguments
/// `total`: The total number of passwords to be tested
///
pub fn build_progress_bar() -> ProgressBar {
    // Create style
    let progress_style = ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {wide_bar:.cyan/blue} {percent}% {pos:>7}/{len:7} throughput:{per_sec} eta:{eta}")
        .expect("Failed to create progress style");

    // Create progress bar and apply style
    let progress_bar = ProgressBar::new(0);
    progress_bar.set_style(progress_style);

    // Refresh terminal once per second (helps with eta smoothing)
    let draw_target = ProgressDrawTarget::stdout_with_hz(1);
    progress_bar.set_draw_target(draw_target);
    progress_bar.enable_steady_tick(Duration::from_millis(1000));

    progress_bar
}

// Progress bar extension methods

pub trait ProgressBarEx {
    fn println(&self, message: &str);
    fn inc(&self);
    fn set_length(&self, length: u64);
}

impl ProgressBarEx for Option<ProgressBar> {
    fn println(&self, message: &str) {
        if self.is_none() {
            info!("{}", message);
        } else {
            self.as_ref().unwrap().println(message);
        }
    }
    fn inc(&self) {
        if self.is_some() {
            self.as_ref().unwrap().inc(1);
        }
    }
    fn set_length(&self, length: u64) {
        if self.is_some() {
            self.as_ref().unwrap().set_length(length);
        }
    }
}
