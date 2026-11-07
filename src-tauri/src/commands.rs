use std::path::Path;

use roms_curator::core::launch_ready::policy_selected_count;
use roms_curator::core::launch_ready::HeldBackGame;
use roms_curator::core::roms_service::{
    excluded_games_for_review, held_back_games, indexed_chd_file_count, missing_bios_for_report,
    missing_chds_for_report,
};
use roms_curator::models::plan::{Availability, Plan, PlannedGame};
use roms_curator::models::plan_preview_html::{write_plan_preview_html, PlanPreviewHtml};
use roms_curator::models::report::{
    report_from_plan_preview, MissingBiosEntry, MissingChdEntry, OperationStatus, Report,
};
use roms_curator::{
    check_export_form, default_metadata_cache_dir, ensure_metadata, run_with_progress,
    ExportConfig, ExportFormIssues,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::job::JobState;
use crate::progress::{
    emit_files, emit_phase, preview_phase_total, PhaseCounter, EXPORT_COPY_STEP, EXPORT_PHASE_TOTAL,
};

pub const SELECTION_HELP: &str =
    "Standard selection keeps arcade games MAME rates good or imperfect. Strict keeps only fully working games.";

#[derive(Debug, Deserialize, Clone)]
pub struct ExportInput {
    pub source_path: String,
    pub destination_path: String,
    pub mame_version: String,
    pub mame_xml_path: Option<String>,
    pub catver_path: Option<String>,
    pub strict_emulation: bool,
    pub require_complete_dependencies: Option<bool>,
    pub subset_start: Option<String>,
    pub subset_end: Option<String>,
    pub report_path: Option<String>,
    pub chd_source_path: Option<String>,
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
    pub missing_chds: usize,
    pub missing_bios: usize,
    pub excluded_games: usize,
    pub held_back_games: usize,
    pub policy_selected_games: usize,
    pub chd_files_indexed: usize,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PlanReviewResult {
    pub summary: PlanPreviewSummary,
    pub selection_help: &'static str,
    pub plan: Plan,
    pub missing_chds: Vec<MissingChdEntry>,
    pub missing_bios: Vec<MissingBiosEntry>,
    pub excluded_games: Vec<PlannedGame>,
    pub held_back_games: Vec<HeldBackGame>,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteExportRequest {
    pub input: ExportInput,
    pub plan: Plan,
}

#[derive(Debug, Deserialize)]
pub struct WritePlanPreviewRequest {
    pub output_path: String,
    pub input: ExportInput,
    pub plan: Plan,
    pub missing_chds: Vec<MissingChdEntry>,
    pub missing_bios: Vec<MissingBiosEntry>,
    pub excluded_games: Vec<PlannedGame>,
    pub held_back_games: Vec<HeldBackGame>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct WritePlanReportResult {
    pub report_path: String,
    pub report_written: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct WritePlanHtmlResult {
    pub plan_html_path: String,
    pub plan_html_written: bool,
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

#[derive(Debug, Deserialize)]
pub struct FormCheckInput {
    pub source_path: String,
    pub destination_path: String,
    pub mame_version: String,
    pub subset_start: Option<String>,
    pub subset_end: Option<String>,
}

#[tauri::command]
pub fn validate_export_form(input: FormCheckInput) -> ExportFormIssues {
    check_export_form(
        &input.mame_version,
        &input.source_path,
        &input.destination_path,
        "",
        "",
        input.subset_start.as_deref().unwrap_or(""),
        input.subset_end.as_deref().unwrap_or(""),
    )
}

#[tauri::command]
pub async fn review_export_plan(
    app: AppHandle,
    state: State<'_, JobState>,
    input: ExportInput,
    run_id: u64,
) -> Result<PlanReviewResult, String> {
    let cancel = state.start(run_id);
    let app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        review_export_plan_blocking(&app, input, Some(&cancel))
    })
    .await
    .map_err(|error| format!("Plan task failed: {error}"))?;
    state.clear(run_id);
    result
}

#[tauri::command]
pub async fn execute_export(
    app: AppHandle,
    state: State<'_, JobState>,
    request: ExecuteExportRequest,
    run_id: u64,
) -> Result<ExportResult, String> {
    let cancel = state.start(run_id);
    let app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        execute_export_blocking(&app, request, Some(&cancel))
    })
    .await
    .map_err(|error| format!("Export task failed: {error}"))?;
    state.clear(run_id);
    result
}

#[tauri::command]
pub fn cancel_background_work(state: State<'_, JobState>, run_id: u64) {
    state.request_cancel(run_id);
}

#[tauri::command]
pub async fn write_plan_report(
    request: WritePlanPreviewRequest,
) -> Result<WritePlanReportResult, String> {
    let result = tauri::async_runtime::spawn_blocking(move || write_plan_report_blocking(request))
        .await
        .map_err(|error| format!("Report task failed: {error}"))?;
    result
}

#[tauri::command]
pub async fn write_plan_html(
    request: WritePlanPreviewRequest,
) -> Result<WritePlanHtmlResult, String> {
    let result = tauri::async_runtime::spawn_blocking(move || write_plan_html_blocking(request))
        .await
        .map_err(|error| format!("HTML task failed: {error}"))?;
    result
}

fn write_plan_report_blocking(
    request: WritePlanPreviewRequest,
) -> Result<WritePlanReportResult, String> {
    let report_path = request.output_path.trim().to_string();
    if report_path.is_empty() {
        return Err("Choose where to save the Markdown plan report.".into());
    }
    let issues = plan_preview_form_issues_for_markdown(&request.input, &report_path);
    if issues_blocking_markdown_report(&issues) {
        return Err(format_form_issues(&issues));
    }
    if !request.plan.conflicts.is_empty() {
        return Err(
            "Report blocked: resolve destination conflicts shown in the plan first.".into(),
        );
    }
    let report = report_from_plan_preview(
        &request.plan,
        request.missing_chds,
        request.missing_bios.clone(),
    );
    let written = report
        .to_file(&report_path)
        .map_err(|error| error.to_string())?;
    Ok(WritePlanReportResult {
        report_path,
        report_written: written,
    })
}

fn write_plan_html_blocking(
    request: WritePlanPreviewRequest,
) -> Result<WritePlanHtmlResult, String> {
    let plan_html_path = request.output_path.trim().to_string();
    if plan_html_path.is_empty() {
        return Err("Choose where to save the HTML plan preview.".into());
    }
    let issues = plan_preview_form_issues_for_html(&request.input, &plan_html_path);
    if issues_blocking_plan_html(&issues) {
        return Err(format_form_issues(&issues));
    }
    if !request.plan.conflicts.is_empty() {
        return Err(
            "HTML export blocked: resolve destination conflicts shown in the plan first.".into(),
        );
    }
    let preview = PlanPreviewHtml {
        plan: &request.plan,
        missing_chds: &request.missing_chds,
        missing_bios: &request.missing_bios,
        excluded_games: &request.excluded_games,
        held_back_games: &request.held_back_games,
    };
    let written =
        write_plan_preview_html(&plan_html_path, preview).map_err(|error| error.to_string())?;
    Ok(WritePlanHtmlResult {
        plan_html_path,
        plan_html_written: written,
    })
}

fn plan_preview_form_issues_for_markdown(
    input: &ExportInput,
    output_path: &str,
) -> ExportFormIssues {
    check_export_form(
        &input.mame_version,
        &input.source_path,
        &input.destination_path,
        output_path,
        "",
        input.subset_start.as_deref().unwrap_or(""),
        input.subset_end.as_deref().unwrap_or(""),
    )
}

fn plan_preview_form_issues_for_html(input: &ExportInput, output_path: &str) -> ExportFormIssues {
    check_export_form(
        &input.mame_version,
        &input.source_path,
        &input.destination_path,
        "",
        output_path,
        input.subset_start.as_deref().unwrap_or(""),
        input.subset_end.as_deref().unwrap_or(""),
    )
}

fn issues_blocking_markdown_report(issues: &ExportFormIssues) -> bool {
    issues.mame_version.is_some()
        || issues.source_path.is_some()
        || issues.destination_path.is_some()
        || issues.report_path.is_some()
        || issues.subset_start.is_some()
        || issues.subset_end.is_some()
}

fn issues_blocking_plan_html(issues: &ExportFormIssues) -> bool {
    issues.mame_version.is_some()
        || issues.source_path.is_some()
        || issues.destination_path.is_some()
        || issues.plan_html_path.is_some()
        || issues.subset_start.is_some()
        || issues.subset_end.is_some()
}

fn format_form_issues(issues: &ExportFormIssues) -> String {
    [
        issues.mame_version.as_deref(),
        issues.source_path.as_deref(),
        issues.destination_path.as_deref(),
        issues.report_path.as_deref(),
        issues.plan_html_path.as_deref(),
        issues.subset_start.as_deref(),
        issues.subset_end.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n")
}

fn review_export_plan_blocking(
    app: &AppHandle,
    input: ExportInput,
    cancel: Option<&roms_curator::CancelFlag>,
) -> Result<PlanReviewResult, String> {
    let config = prepare_config(&input)?;
    validate_metadata_override(&input)?;

    let mut phases = PhaseCounter::new(preview_phase_total(config.source_path.len()));
    phases.advance(app, "Fetching metadata if needed…");
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

    phases.advance(app, "Validating paths…");
    config.validate().map_err(|error| error.to_string())?;

    let mut status = phases.status_callback(app);
    let catalog =
        run_with_progress(&config, Some(&mut status), cancel).map_err(|error| error.to_string())?;

    let plan = catalog
        .plan_with_progress(&config, Some(&mut status), cancel)
        .map_err(|error| error.to_string())?;

    let missing_chds =
        missing_chds_for_report(&catalog, &config).map_err(|error| error.to_string())?;
    let missing_bios =
        missing_bios_for_report(&catalog, &config).map_err(|error| error.to_string())?;
    let excluded_games = excluded_games_for_review(&catalog, &config);
    let held_back = held_back_games(&catalog, &config).map_err(|error| error.to_string())?;
    let policy_selected = policy_selected_count(&catalog, &config);
    let chd_files_indexed = indexed_chd_file_count(&config).map_err(|error| error.to_string())?;

    Ok(plan_review(
        &plan,
        missing_chds,
        missing_bios,
        excluded_games,
        held_back,
        policy_selected,
        chd_files_indexed,
    ))
}

fn execute_export_blocking(
    app: &AppHandle,
    request: ExecuteExportRequest,
    cancel: Option<&roms_curator::CancelFlag>,
) -> Result<ExportResult, String> {
    let config = prepare_config(&request.input)?;
    validate_metadata_override(&request.input)?;

    let mut phases = PhaseCounter::new(EXPORT_PHASE_TOTAL);
    phases.advance(app, "Fetching metadata if needed…");
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

    phases.advance(app, "Validating paths…");
    config.validate().map_err(|error| error.to_string())?;

    if !request.plan.conflicts.is_empty() {
        return Err(
            "Export blocked: resolve destination conflicts shown in the plan first.".into(),
        );
    }

    let mut status = phases.status_callback(app);
    let catalog =
        run_with_progress(&config, Some(&mut status), cancel).map_err(|error| error.to_string())?;

    let copy_message = "Copying into roms and rest…";
    let mut copy_progress = |pos: u64, len: u64| {
        emit_files(
            app,
            EXPORT_COPY_STEP,
            EXPORT_PHASE_TOTAL,
            pos,
            len,
            copy_message,
        );
    };
    let report = catalog
        .copy_with_plan_progress(&config, &request.plan, cancel, Some(&mut copy_progress))
        .map_err(|error| error.to_string())?;

    let report_path = config.report_path.clone();
    let report_written = if report_path.is_empty() {
        emit_phase(
            app,
            EXPORT_PHASE_TOTAL,
            EXPORT_PHASE_TOTAL,
            "Export finished.",
        );
        false
    } else {
        emit_phase(
            app,
            EXPORT_PHASE_TOTAL,
            EXPORT_PHASE_TOTAL,
            "Writing report…",
        );
        report
            .to_file(&report_path)
            .map_err(|error| error.to_string())?;
        true
    };

    Ok(summarize_export(&report, &report_path, report_written))
}

fn prepare_config(input: &ExportInput) -> Result<ExportConfig, String> {
    Ok(ExportConfig {
        source_path: source_paths(input),
        destination_path: input.destination_path.clone(),
        strict_emulation: input.strict_emulation,
        require_complete_dependencies: input.require_complete_dependencies.unwrap_or(true),
        subset_start: input.subset_start.clone().unwrap_or_default(),
        subset_end: input.subset_end.clone().unwrap_or_default(),
        report_path: input.report_path.clone().unwrap_or_default(),
        progress: false,
        ..ExportConfig::default()
    })
}

fn source_paths(input: &ExportInput) -> Vec<String> {
    let mut paths = vec![input.source_path.clone()];
    if let Some(chd) = input.chd_source_path.as_deref() {
        let chd = chd.trim();
        if !chd.is_empty() && chd != input.source_path.trim() {
            paths.push(chd.to_string());
        }
    }
    paths
}

fn plan_review(
    plan: &Plan,
    missing_chds: Vec<MissingChdEntry>,
    missing_bios: Vec<MissingBiosEntry>,
    excluded_games: Vec<PlannedGame>,
    held_back: Vec<HeldBackGame>,
    policy_selected: usize,
    chd_files_indexed: usize,
) -> PlanReviewResult {
    PlanReviewResult {
        summary: summarize_plan(
            plan,
            missing_chds.len(),
            missing_bios.len(),
            excluded_games.len(),
            held_back.len(),
            policy_selected,
            chd_files_indexed,
        ),
        selection_help: SELECTION_HELP,
        plan: plan.clone(),
        missing_chds,
        missing_bios,
        excluded_games,
        held_back_games: held_back,
    }
}

fn summarize_plan(
    plan: &Plan,
    missing_chds: usize,
    missing_bios: usize,
    excluded_games: usize,
    held_back: usize,
    policy_selected: usize,
    chd_files_indexed: usize,
) -> PlanPreviewSummary {
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
        missing_chds,
        missing_bios,
        excluded_games,
        held_back_games: held_back,
        policy_selected_games: policy_selected,
        chd_files_indexed,
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
            chd_source_path: None,
            require_complete_dependencies: None,
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

        let review = plan_review(&plan, vec![], vec![], vec![], vec![], 0, 0);
        assert_eq!(review.summary.present_media, 1);
        assert_eq!(review.summary.missing_media, 1);
        assert!(planned_media_for_roms(&review.plan.media[0]));
        assert!(!planned_media_for_roms(&review.plan.media[1]));
    }
}
