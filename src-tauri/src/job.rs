use roms_curator::CancelFlag;
use std::sync::Mutex;

#[derive(Clone, Copy)]
pub enum JobKind {
    Scan,
    Scrape,
    Export,
}

impl JobKind {
    fn busy_message(self) -> &'static str {
        match self {
            JobKind::Scan => "Wait for the current scan to finish or cancel it.",
            JobKind::Scrape => "Wait for the current scrape to finish or cancel it.",
            JobKind::Export => "Wait for the current export to finish or cancel it.",
        }
    }
}

struct RunningJob {
    run_id: u64,
    kind: JobKind,
    cancel: CancelFlag,
}

struct Inner {
    job: Option<RunningJob>,
    /// Cancel clicked before `start` stored this run.
    pending_run: Option<u64>,
}

pub struct JobState(Mutex<Inner>);

impl JobState {
    pub fn new() -> Self {
        Self(Mutex::new(Inner {
            job: None,
            pending_run: None,
        }))
    }

    pub fn start(&self, run_id: u64, kind: JobKind) -> Result<CancelFlag, &'static str> {
        let mut state = self.0.lock().expect("job mutex");
        if let Some(job) = &state.job {
            return Err(job.kind.busy_message());
        }
        let cancel = CancelFlag::new();
        if state.pending_run == Some(run_id) {
            cancel.cancel();
            state.pending_run = None;
        }
        state.job = Some(RunningJob {
            run_id,
            kind,
            cancel: cancel.clone(),
        });
        Ok(cancel)
    }

    pub fn clear(&self, run_id: u64) {
        let mut state = self.0.lock().expect("job mutex");
        if state.job.as_ref().is_some_and(|job| job.run_id == run_id) {
            state.job = None;
        }
    }

    pub fn request_cancel(&self, run_id: u64) {
        let mut state = self.0.lock().expect("job mutex");
        if let Some(job) = state.job.as_ref() {
            if job.run_id == run_id {
                job.cancel.cancel();
            }
            return;
        }
        state.pending_run = Some(run_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_requested_before_start_stops_that_run_only() {
        let state = JobState::new();
        state.request_cancel(7);
        let flag = state.start(7, JobKind::Scan).unwrap();
        assert!(flag.is_cancelled());
        state.clear(7);

        state.request_cancel(7);
        let next = state.start(8, JobKind::Scan).unwrap();
        assert!(!next.is_cancelled());
    }

    #[test]
    fn start_rejects_a_second_job_until_the_first_is_cleared() {
        let state = JobState::new();
        let first = state.start(1, JobKind::Export).unwrap();
        let Err(message) = state.start(2, JobKind::Scrape) else {
            panic!("a second job was started");
        };
        assert_eq!(
            message,
            "Wait for the current export to finish or cancel it."
        );
        state.request_cancel(2);
        assert!(!first.is_cancelled());
        state.request_cancel(1);
        assert!(first.is_cancelled());
        state.clear(1);
        let next = state.start(2, JobKind::Scrape).unwrap();
        assert!(!next.is_cancelled());
    }
}
