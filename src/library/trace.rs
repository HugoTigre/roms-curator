//! Lines recorded during one lookup. Enrichment reads them after the provider returns.

use std::cell::RefCell;

thread_local! {
    static LINES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn clear() {
    LINES.with(|lines| lines.borrow_mut().clear());
}

pub(crate) fn line(message: impl Into<String>) {
    LINES.with(|lines| {
        let mut lines = lines.borrow_mut();
        if lines.len() < 40 {
            lines.push(message.into());
        }
    });
}

pub(crate) fn take() -> Vec<String> {
    LINES.with(|lines| std::mem::take(&mut *lines.borrow_mut()))
}
