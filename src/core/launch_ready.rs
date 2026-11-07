//! Which policy-selected games are complete enough to export and run.

use std::collections::HashSet;
use std::error::Error;

use crate::core::config::ExportConfig;
use crate::core::export::name_in_subset_excluded;
use crate::models::dependencies::DependencyKind;
use crate::models::disks::DiskKind;
use crate::models::plan::{chd_pairs_in_sources, source_has_chd};
use crate::models::roms::Catalog;

/// One policy-selected game held back because dependencies are not in the source.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HeldBackGame {
    pub machine: String,
    pub category: String,
    pub blockers: Vec<String>,
}

/// Policy-selected machines inside the configured name range (before completeness).
pub fn policy_selected_count(catalog: &Catalog, config: &ExportConfig) -> usize {
    catalog
        .selected
        .iter()
        .filter(|name| !name_in_subset_excluded(config, name))
        .count()
}

/// Policy-selected machines that will be exported (subset range applied).
pub fn export_game_names(
    catalog: &Catalog,
    config: &ExportConfig,
) -> Result<Vec<String>, Box<dyn Error>> {
    let chds = chd_pairs_in_sources(&config.source_path)?;
    let mut ready = Vec::new();
    for name in &catalog.selected {
        if name_in_subset_excluded(config, name) {
            continue;
        }
        if config.require_complete_dependencies && !blocking_gaps(catalog, &chds, name).is_empty() {
            continue;
        }
        ready.push(name.clone());
    }
    ready.sort();
    Ok(ready)
}

pub fn export_game_set(
    catalog: &Catalog,
    config: &ExportConfig,
) -> Result<HashSet<String>, Box<dyn Error>> {
    Ok(export_game_names(catalog, config)?
        .into_iter()
        .map(|name| name.to_ascii_lowercase())
        .collect())
}

/// Policy-selected machines not exported because [`ExportConfig::require_complete_dependencies`] is set.
pub fn held_back_games(
    catalog: &Catalog,
    config: &ExportConfig,
) -> Result<Vec<HeldBackGame>, Box<dyn Error>> {
    if !config.require_complete_dependencies {
        return Ok(Vec::new());
    }
    let chds = chd_pairs_in_sources(&config.source_path)?;
    let mut held = Vec::new();
    for name in &catalog.selected {
        if name_in_subset_excluded(config, name) {
            continue;
        }
        let blockers = blocking_gaps(catalog, &chds, name);
        if blockers.is_empty() {
            continue;
        }
        let rom = catalog
            .roms
            .get(name)
            .ok_or_else(|| format!("Selected machine missing from catalog: {name}"))?;
        held.push(HeldBackGame {
            machine: name.clone(),
            category: rom.data.category.clone(),
            blockers,
        });
    }
    held.sort();
    Ok(held)
}

pub fn game_exports(
    edge_game: &str,
    export_games: &HashSet<String>,
    config: &ExportConfig,
) -> bool {
    !name_in_subset_excluded(config, edge_game)
        && export_games.contains(&edge_game.to_ascii_lowercase())
}

fn blocking_gaps(catalog: &Catalog, chds: &HashSet<(String, String)>, game: &str) -> Vec<String> {
    let mut gaps = Vec::new();
    for missing in catalog
        .dependencies
        .missing
        .iter()
        .filter(|entry| entry.game == game)
    {
        gaps.push(format!(
            "Missing {}: {}",
            dependency_label(missing.kind),
            missing.name
        ));
    }
    for missing in catalog
        .disks
        .missing
        .iter()
        .filter(|entry| entry.game == game)
    {
        gaps.push(format!(
            "Missing CHD link: {} ({})",
            missing.name,
            disk_missing_label(missing.kind)
        ));
    }
    for edge in catalog
        .disks
        .edges
        .iter()
        .filter(|entry| entry.game == game)
    {
        if source_has_chd(chds, &edge.machine, &edge.name) {
            continue;
        }
        gaps.push(format!(
            "CHD not in source: {}/{}.chd ({})",
            edge.machine,
            edge.name,
            disk_edge_label(edge.kind)
        ));
    }
    gaps.sort();
    gaps.dedup();
    gaps
}

fn dependency_label(kind: DependencyKind) -> &'static str {
    match kind {
        DependencyKind::Own => "ROM zip",
        DependencyKind::Parent => "parent ROM zip",
        DependencyKind::RomParent => "ROM-parent zip",
        DependencyKind::Device => "device ROM zip",
    }
}

fn disk_missing_label(kind: DiskKind) -> &'static str {
    match kind {
        DiskKind::Own => "own disk",
        DiskKind::Shared => "shared disk",
        DiskKind::DeltaParent => "delta parent",
        DiskKind::Device => "device disk",
    }
}

fn disk_edge_label(kind: DiskKind) -> &'static str {
    match kind {
        DiskKind::Own => "required disk",
        DiskKind::Shared => "shared disk",
        DiskKind::DeltaParent => "delta base",
        DiskKind::Device => "device disk",
    }
}
