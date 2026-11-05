use roms_curator::CancelFlag;
use std::sync::Mutex;

struct RunningJob {
    run_id: u64,
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

    pub fn start(&self, run_id: u64) -> CancelFlag {
        let mut state = self.0.lock().expect("job mutex");
        let cancel = CancelFlag::new();
        if state.pending_run == Some(run_id) {
            cancel.cancel();
            state.pending_run = None;
        }
        state.job = Some(RunningJob {
            run_id,
            cancel: cancel.clone(),
        });
        cancel
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
        let flag = state.start(7);
        assert!(flag.is_cancelled());
        state.clear(7);

        state.request_cancel(7);
        let next = state.start(8);
        assert!(!next.is_cancelled());
    }
}
