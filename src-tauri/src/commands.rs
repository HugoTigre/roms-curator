use std::path::Path;

use roms_curator::models::plan::{Availability, Plan};
use roms_curator::{default_metadata_cache_dir, ensure_metadata, run_with_progress, ExportConfig};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Deserialize)]
pub struct PlanPreviewInput {
    pub source_path: String,
    pub destination_path: String,
    pub mame_version: String,
    pub mame_xml_path: Option<String>,
    pub catver_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PlanPreviewResult {
    pub mame_build: String,
    pub emulation_policy: String,
    pub selected_games: usize,
    pub present_media: usize,
    pub missing_media: usize,
    pub unresolved: usize,
    pub conflicts: Vec<String>,
}

#[tauri::command]
pub async fn build_plan_preview(
    app: AppHandle,
    input: PlanPreviewInput,
) -> Result<PlanPreviewResult, String> {
    tauri::async_runtime::spawn_blocking(move || build_plan_preview_blocking(&app, input))
        .await
        .map_err(|error| format!("Plan task failed: {error}"))?
}

fn build_plan_preview_blocking(
    app: &AppHandle,
    input: PlanPreviewInput,
) -> Result<PlanPreviewResult, String> {
    validate_metadata_override(&input)?;

    emit(app, "Fetching metadata if needed…");
    let cache_root = default_metadata_cache_dir();
    let paths = ensure_metadata(&input.mame_version, &cache_root, false)
        .map_err(|error| error.to_string())?;

    let mame_xml_path = input
        .mame_xml_path
        .unwrap_or_else(|| paths.mame_xml.display().to_string());
    let catver_path = input
        .catver_path
        .unwrap_or_else(|| paths.catver.display().to_string());

    let config = ExportConfig {
        mame_xml_path,
        catver_path,
        source_path: vec![input.source_path],
        destination_path: input.destination_path,
        progress: false,
        ..ExportConfig::default()
    };

    emit(app, "Validating paths…");
    config.validate().map_err(|error| error.to_string())?;

    let mut status = |message: &str| emit(app, message);
    let catalog = run_with_progress(&config, Some(&mut status))
        .map_err(|error| error.to_string())?;

    let plan: Plan = catalog
        .plan_with_progress(&config, Some(&mut status))
        .map_err(|error| error.to_string())?;

    Ok(summarize_plan(&plan))
}

fn validate_metadata_override(input: &PlanPreviewInput) -> Result<(), String> {
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

fn summarize_plan(plan: &Plan) -> PlanPreviewResult {
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

    PlanPreviewResult {
        mame_build: plan.metadata.mame_build.clone(),
        emulation_policy: plan.metadata.emulation_policy.clone(),
        selected_games: plan.games.len(),
        present_media,
        missing_media,
        unresolved: plan.unresolved.len(),
        conflicts: plan.conflicts.clone(),
    }
}

fn emit(app: &AppHandle, message: &str) {
    let _ = app.emit("plan-progress", message);
}

#[cfg(test)]
mod tests {
    use super::*;
    use roms_curator::models::plan::{
        MediaRole, MetadataIdentity, PlannedMedia, UnresolvedMedia,
    };

    #[test]
    fn metadata_override_requires_both_paths() {
        let only_xml = PlanPreviewInput {
            source_path: "/src".into(),
            destination_path: "/dest".into(),
            mame_version: "0.289".into(),
            mame_xml_path: Some("mame.xml".into()),
            catver_path: None,
        };
        let error = validate_metadata_override(&only_xml).unwrap_err();
        assert!(error.contains("Both metadata paths"));

        let neither = PlanPreviewInput {
            catver_path: None,
            mame_xml_path: None,
            ..only_xml
        };
        assert!(validate_metadata_override(&neither).is_ok());
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
                    reasons: vec![],
                },
                PlannedMedia {
                    machine: "b".into(),
                    role: MediaRole::Disk,
                    availability: Availability::Missing,
                    source: None,
                    destination: None,
                    bytes: None,
                    reasons: vec![],
                },
            ],
            unresolved: vec![UnresolvedMedia {
                game: "game".into(),
                kind: "archive".into(),
                name: "missing".into(),
            }],
            conflicts: vec!["clash".into()],
        };

        let summary = summarize_plan(&plan);
        assert_eq!(summary.present_media, 1);
        assert_eq!(summary.missing_media, 1);
        assert_eq!(summary.unresolved, 1);
        assert_eq!(summary.conflicts, vec!["clash"]);
    }
}
