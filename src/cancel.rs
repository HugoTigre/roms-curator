use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct CancelFlag(Arc<CancelState>);

struct CancelState {
    aborted: AtomicBool,
    #[cfg(test)]
    poll_hook: std::sync::Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl Default for CancelFlag {
    fn default() -> Self {
        Self::new()
    }
}

impl CancelFlag {
    pub fn new() -> Self {
        Self(Arc::new(CancelState {
            aborted: AtomicBool::new(false),
            #[cfg(test)]
            poll_hook: std::sync::Mutex::new(None),
        }))
    }

    pub fn cancel(&self) {
        self.0.aborted.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.aborted.load(Ordering::Relaxed)
    }

    /// Test hook run on every poll, before the cancelled flag is read.
    #[cfg(test)]
    pub fn on_each_poll(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self.0.poll_hook.lock().expect("cancel poll hook") = Some(Arc::new(hook));
    }

    pub(crate) fn poll(&self) -> bool {
        #[cfg(test)]
        if let Some(hook) = self.0.poll_hook.lock().expect("cancel poll hook").clone() {
            hook();
        }
        self.is_cancelled()
    }
}

pub fn check_cancelled(cancel: Option<&CancelFlag>) -> Result<(), Box<dyn Error>> {
    if cancel.is_some_and(CancelFlag::poll) {
        return Err("Operation cancelled.".into());
    }
    Ok(())
}
