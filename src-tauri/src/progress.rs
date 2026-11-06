use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ProgressPayload {
    Phase {
        step: u32,
        total: u32,
        message: String,
    },
    Files {
        step: u32,
        total: u32,
        pos: u64,
        len: u64,
        message: String,
    },
}

pub fn emit_phase(app: &AppHandle, step: u32, total: u32, message: impl Into<String>) {
    let _ = app.emit(
        "plan-progress",
        ProgressPayload::Phase {
            step,
            total,
            message: message.into(),
        },
    );
}

pub fn emit_files(
    app: &AppHandle,
    step: u32,
    total: u32,
    pos: u64,
    len: u64,
    message: impl Into<String>,
) {
    let _ = app.emit(
        "plan-progress",
        ProgressPayload::Files {
            step,
            total,
            pos,
            len,
            message: message.into(),
        },
    );
}

/// Status lines from [`roms_curator::run_with_progress`] (load, CatVer, XML, categorize, summary).
pub const CATALOG_STATUS_LINES: u32 = 5;

/// Preview: metadata, validate, catalog status lines, then plan scan (collection line,
/// one line per source root, conflict check).
pub fn preview_phase_total(source_roots: usize) -> u32 {
    2 + CATALOG_STATUS_LINES + 2 + source_roots as u32
}

/// Export: metadata, validate, catalog status lines, copy, report/finish.
pub const EXPORT_PHASE_TOTAL: u32 = 2 + CATALOG_STATUS_LINES + 2;
pub const EXPORT_COPY_STEP: u32 = 2 + CATALOG_STATUS_LINES + 1;

pub struct PhaseCounter {
    step: u32,
    total: u32,
}

impl PhaseCounter {
    pub fn new(total: u32) -> Self {
        Self { step: 0, total }
    }

    pub fn advance(&mut self, app: &AppHandle, message: impl Into<String>) {
        self.step += 1;
        emit_phase(app, self.step, self.total, message);
    }

    pub fn status_callback<'a>(&'a mut self, app: &'a AppHandle) -> impl FnMut(&str) + 'a {
        move |message| {
            self.step += 1;
            emit_phase(app, self.step, self.total, message.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn preview_phase_total_scales_with_source_roots() {
        assert_eq!(preview_phase_total(1), 10);
        assert_eq!(preview_phase_total(2), 11);
    }

    #[test]
    fn progress_payload_matches_gui_contract() {
        let phase = ProgressPayload::Phase {
            step: 3,
            total: 10,
            message: "Reading listxml…".into(),
        };
        let files = ProgressPayload::Files {
            step: 8,
            total: 9,
            pos: 120,
            len: 5000,
            message: "Copying…".into(),
        };

        let phase_json: Value = serde_json::to_value(&phase).unwrap();
        assert_eq!(phase_json["kind"], "phase");
        assert_eq!(phase_json["step"], 3);
        assert_eq!(phase_json["total"], 10);
        assert_eq!(phase_json["message"], "Reading listxml…");
        assert!(phase_json.get("pos").is_none());

        let files_json: Value = serde_json::to_value(&files).unwrap();
        assert_eq!(
            files_json,
            json!({
                "kind": "files",
                "step": 8,
                "total": 9,
                "pos": 120,
                "len": 5000,
                "message": "Copying…",
            })
        );
    }
}
