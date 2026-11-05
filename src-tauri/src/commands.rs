use std::path::Path;

use roms_curator::models::plan::{Availability, Plan};
use roms_curator::models::report::{OperationStatus, Report};
use roms_curator::{default_metadata_cache_dir, ensure_metadata, run_with_progress, ExportConfig};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

pub const SELECTION_HELP: &str = "Standard selection keeps arcade games MAME rates good or imperfect. Strict keeps only fully working games. Computers, consoles, and similar CatVer groups are not launch-list games. Parent, BIOS, device, and disk files listed below are copied into the roms folder when a selected game needs them.";

#[derive(Debug, Deserialize, Clone)]
pub struct ExportInput {
    pub source_path: String,
    pub destination_path: String,
    pub mame_version: String,
    pub mame_xml_path: Option<String>,
    pub catver_path: Option<String>,
    pub strict_emulation: bool,
    pub subset_start: Option<String>,
    pub subset_end: Option<String>,
    pub report_path: Option<String>,
    pub metadata_refresh: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PlanPreviewSummary {
    pub mame_build: String,
    pub emulation_policy: String,
    pub selected_games: usize,
    pub present_media: usize,
    pub missing_media: usize,
    pub unresolved: usize,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PlanReviewResult {
    pub summary: PlanPreviewSummary,
    pub selection_help: &'static str,
    pub plan: Plan,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteExportRequest {
    pub input: ExportInput,
    pub plan: Plan,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ExportResult {
    pub all_ok: bool,
    pub selected_games: usize,
    pub roms_entries: usize,
    pub rest_entries: usize,
    pub failed_copies: usize,
    pub missing_chds: usize,
    pub report_path: String,
    pub report_written: bool,
    pub names_error: Option<String>,
}

#[tauri::command]
pub async fn review_export_plan(
    app: AppHandle,
    input: ExportInput,
) -> Result<PlanReviewResult, String> {
    tauri::async_runtime::spawn_blocking(move || review_export_plan_blocking(&app, input))
        .await
        .map_err(|error| format!("Plan task failed: {error}"))?
}

#[tauri::command]
pub async fn execute_export(
    app: AppHandle,
    request: ExecuteExportRequest,
) -> Result<ExportResult, String> {
    tauri::async_runtime::spawn_blocking(move || execute_export_blocking(&app, request))
        .await
        .map_err(|error| format!("Export task failed: {error}"))?
}

fn review_export_plan_blocking(
    app: &AppHandle,
    input: ExportInput,
) -> Result<PlanReviewResult, String> {
    let config = prepare_config(&input)?;
    validate_metadata_override(&input)?;

    emit(app, "Fetching metadata if needed…");
    let cache_root = default_metadata_cache_dir();
    let paths = ensure_metadata(&input.mame_version, &cache_root, input.metadata_refresh)
        .map_err(|error| error.to_string())?;

    let mut config = config;
    config.mame_xml_path = input
        .mame_xml_path
        .unwrap_or_else(|| paths.mame_xml.display().to_string());
    config.catver_path = input
        .catver_path
        .unwrap_or_else(|| paths.catver.display().to_string());

    emit(app, "Validating paths…");
    config.validate().map_err(|error| error.to_string())?;

    let mut status = |message: &str| emit(app, message);
    let catalog =
        run_with_progress(&config, Some(&mut status)).map_err(|error| error.to_string())?;

    let plan = catalog
        .plan_with_progress(&config, Some(&mut status))
        .map_err(|error| error.to_string())?;

    Ok(plan_review(&plan))
}

fn execute_export_blocking(
    app: &AppHandle,
    request: ExecuteExportRequest,
) -> Result<ExportResult, String> {
    let config = prepare_config(&request.input)?;
    validate_metadata_override(&request.input)?;

    emit(app, "Fetching metadata if needed…");
    let cache_root = default_metadata_cache_dir();
    let paths = ensure_metadata(
        &request.input.mame_version,
        &cache_root,
        request.input.metadata_refresh,
    )
    .map_err(|error| error.to_string())?;

    let mut config = config;
    config.mame_xml_path = request
        .input
        .mame_xml_path
        .unwrap_or_else(|| paths.mame_xml.display().to_string());
    config.catver_path = request
        .input
        .catver_path
        .unwrap_or_else(|| paths.catver.display().to_string());

    emit(app, "Validating paths…");
    config.validate().map_err(|error| error.to_string())?;

    if !request.plan.conflicts.is_empty() {
        return Err(
            "Export blocked: resolve destination conflicts shown in the plan first.".into(),
        );
    }

    emit(app, "Loading catalog for export…");
    let mut status = |message: &str| emit(app, message);
    let catalog =
        run_with_progress(&config, Some(&mut status)).map_err(|error| error.to_string())?;

    emit(app, "Copying into roms and rest…");
    let report = catalog
        .copy_with_plan(&config, &request.plan)
        .map_err(|error| error.to_string())?;

    let report_path = config.report_path.clone();
    let report_written = if report_path.is_empty() {
        false
    } else {
        report
            .to_file(&report_path)
            .map_err(|error| error.to_string())?;
        true
    };

    Ok(summarize_export(&report, &report_path, report_written))
}

fn prepare_config(input: &ExportInput) -> Result<ExportConfig, String> {
    Ok(ExportConfig {
        source_path: vec![input.source_path.clone()],
        destination_path: input.destination_path.clone(),
        strict_emulation: input.strict_emulation,
        subset_start: input.subset_start.clone().unwrap_or_default(),
        subset_end: input.subset_end.clone().unwrap_or_default(),
        report_path: input.report_path.clone().unwrap_or_default(),
        progress: false,
        ..ExportConfig::default()
    })
}

fn plan_review(plan: &Plan) -> PlanReviewResult {
    PlanReviewResult {
        summary: summarize_plan(plan),
        selection_help: SELECTION_HELP,
        plan: plan.clone(),
    }
}

fn summarize_plan(plan: &Plan) -> PlanPreviewSummary {
    let present_media = plan
        .media
        .iter()
        .filter(|entry| entry.availability == Availability::Present)
        .count();
    let missing_media = plan
        .media
        .iter()
        .filter(|entry| entry.availability == Availability::Missing)
        .count();

    PlanPreviewSummary {
        mame_build: plan.metadata.mame_build.clone(),
        emulation_policy: plan.metadata.emulation_policy.clone(),
        selected_games: plan.games.len(),
        present_media,
        missing_media,
        unresolved: plan.unresolved.len(),
        conflicts: plan.conflicts.clone(),
    }
}

fn summarize_export(report: &Report, report_path: &str, report_written: bool) -> ExportResult {
    let failed_copies = report
        .roms_working
        .iter()
        .chain(&report.roms_other)
        .filter(|entry| entry.status == OperationStatus::Failed)
        .count();

    ExportResult {
        all_ok: report.all_ok,
        selected_games: report.selected_games,
        roms_entries: report.total_working.max(0) as usize,
        rest_entries: report.total_other.max(0) as usize,
        failed_copies,
        missing_chds: report.missing_chds.len(),
        report_path: report_path.to_string(),
        report_written,
        names_error: report.names_error.clone(),
    }
}

fn validate_metadata_override(input: &ExportInput) -> Result<(), String> {
    let Some(mame_xml) = input.mame_xml_path.as_deref() else {
        return Ok(());
    };
    let catver = input
        .catver_path
        .as_deref()
        .ok_or_else(|| "Both metadata paths are required when overriding cache.".to_string())?;
    if !Path::new(mame_xml).is_file() {
        return Err(format!("MAME XML not found: {mame_xml}"));
    }
    if !Path::new(catver).is_file() {
        return Err(format!("CatVer file not found: {catver}"));
    }
    Ok(())
}

fn emit(app: &AppHandle, message: &str) {
    let _ = app.emit("plan-progress", message);
}

#[cfg(test)]
mod tests {
    use super::*;
    use roms_curator::models::plan::{MediaRole, MetadataIdentity, PlannedMedia, UnresolvedMedia};
    use roms_curator::{planned_media_for_roms, REST_MEDIA_REASON};

    #[test]
    fn metadata_override_requires_both_paths() {
        let only_xml = ExportInput {
            source_path: "/src".into(),
            destination_path: "/dest".into(),
            mame_version: "0.289".into(),
            mame_xml_path: Some("mame.xml".into()),
            catver_path: None,
            strict_emulation: false,
            subset_start: None,
            subset_end: None,
            report_path: None,
            metadata_refresh: false,
        };
        let error = validate_metadata_override(&only_xml).unwrap_err();
        assert!(error.contains("Both metadata paths"));
    }

    #[test]
    fn summarize_plan_counts_media_and_unresolved() {
        let plan = Plan {
            version: 2,
            metadata: MetadataIdentity {
                mame_build: "0.289".into(),
                emulation_policy: "playable".into(),
                mame_xml_path: String::new(),
                mame_xml_sha1: String::new(),
                catver_path: String::new(),
                catver_sha1: String::new(),
            },
            source_paths: vec![],
            destination_path: String::new(),
            selected_names_path: String::new(),
            report_path: String::new(),
            subset_start: String::new(),
            subset_end: String::new(),
            games: vec![],
            media: vec![
                PlannedMedia {
                    machine: "a".into(),
                    role: MediaRole::Archive,
                    availability: Availability::Present,
                    source: None,
                    destination: None,
                    bytes: None,
                    reasons: vec!["Parent archive".into()],
                },
                PlannedMedia {
                    machine: "b".into(),
                    role: MediaRole::Disk,
                    availability: Availability::Missing,
                    source: None,
                    destination: None,
                    bytes: None,
                    reasons: vec![REST_MEDIA_REASON.into()],
                },
            ],
            unresolved: vec![UnresolvedMedia {
                game: "game".into(),
                kind: "archive".into(),
                name: "missing".into(),
            }],
            conflicts: vec!["clash".into()],
        };

        let review = plan_review(&plan);
        assert_eq!(review.summary.present_media, 1);
        assert_eq!(review.summary.missing_media, 1);
        assert!(planned_media_for_roms(&review.plan.media[0]));
        assert!(!planned_media_for_roms(&review.plan.media[1]));
    }
}
