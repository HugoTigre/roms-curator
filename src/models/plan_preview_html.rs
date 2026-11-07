//! Standalone HTML export of a GUI plan preview (collapsible sections).
//!
//! Separate from Markdown [`Report`](crate::models::report::Report) export.

use std::error::Error;
use std::fs::File;
use std::io::{LineWriter, Write};
use std::path::Path;

use chrono;
use tempfile::Builder;

use crate::core::roms_service::REST_MEDIA_REASON;
use crate::models::plan::{Availability, MediaRole, Plan, PlannedGame, PlannedMedia, UnresolvedMedia};
use crate::models::report::MissingChdEntry;

/// Everything shown on the plan review screen.
pub struct PlanPreviewHtml<'a> {
    pub plan: &'a Plan,
    pub missing_chds: &'a [MissingChdEntry],
    pub excluded_games: &'a [PlannedGame],
}

/// Write a new `.html` file. Refuses to replace an existing path.
pub fn write_plan_preview_html(
    path: &str,
    preview: PlanPreviewHtml<'_>,
) -> Result<bool, Box<dyn Error>> {
    publish(path, |file| {
        write_document(LineWriter::new(file), preview)?;
        Ok(())
    })
}

fn publish(
    path: &str,
    write: impl FnOnce(&mut File) -> Result<(), Box<dyn Error>>,
) -> Result<bool, Box<dyn Error>> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => return Err(format!("Cannot create HTML file {path}: file already exists").into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(format!("Cannot create HTML file {path}: {error}").into()),
    }
    let parent = Path::new(path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = Builder::new()
        .prefix(".roms-curator-plan-html-")
        .tempfile_in(parent)
        .map_err(|error| format!("Cannot create HTML file {path}: {error}"))?;
    write(temporary.as_file_mut())?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| format!("Cannot publish HTML file {path}: {}", error.error))?;
    Ok(true)
}

fn write_document(mut w: impl Write, preview: PlanPreviewHtml<'_>) -> std::io::Result<()> {
    let plan = preview.plan;
    let present = plan
        .media
        .iter()
        .filter(|m| m.availability == Availability::Present)
        .count();
    let missing_media = plan
        .media
        .iter()
        .filter(|m| m.availability == Availability::Missing)
        .count();
    let support: Vec<&PlannedMedia> = plan
        .media
        .iter()
        .filter(|m| {
            m.reasons
                .iter()
                .any(|reason| reason != REST_MEDIA_REASON)
        })
        .collect();

    writeln!(
        w,
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\">"
    )?;
    writeln!(
        w,
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">"
    )?;
    writeln!(w, "<title>Roms Curator plan preview</title>")?;
    writeln!(w, "<style>{STYLES}</style></head><body>")?;
    writeln!(w, "<header><h1>Roms Curator plan preview</h1>")?;
    writeln!(
        w,
        "<p class=\"lede\">Generated {}. No files were copied.</p></header>",
        html_escape(&chrono::offset::Local::now().to_string())
    )?;

    writeln!(w, "<section class=\"panel\" id=\"summary\">")?;
    writeln!(w, "<h2>Plan summary</h2>")?;
    writeln!(w, "<p class=\"hint\">High-level counts for this preview. Open sections below for full lists.</p>")?;
    writeln!(w, "<dl class=\"summary\">")?;
    summary_row(&mut w, "MAME build", &plan.metadata.mame_build)?;
    summary_row(
        &mut w,
        "Selection",
        match plan.metadata.emulation_policy.as_str() {
            "strict" => "Strict",
            _ => "Standard",
        },
    )?;
    summary_row(&mut w, "Selected / working games", &plan.games.len().to_string())?;
    summary_row(&mut w, "Media present", &present.to_string())?;
    summary_row(&mut w, "Media missing", &missing_media.to_string())?;
    summary_row(
        &mut w,
        "Missing CHDs",
        &preview.missing_chds.len().to_string(),
    )?;
    summary_row(
        &mut w,
        "Unresolved references",
        &plan.unresolved.len().to_string(),
    )?;
    summary_row(
        &mut w,
        "Not on working list",
        &preview.excluded_games.len().to_string(),
    )?;
    writeln!(w, "</dl>")?;
    if !plan.conflicts.is_empty() {
        writeln!(w, "<h3>Destination conflicts</h3><p>Resolve these before export.</p><ul>")?;
        for line in &plan.conflicts {
            writeln!(w, "<li>{}</li>", html_escape(line))?;
        }
        writeln!(w, "</ul>")?;
    }
    writeln!(w, "</section>")?;

    details_table(
        &mut w,
        &format!("Selected / working games ({})", plan.games.len()),
        &["Machine", "Category", "Reason"],
        plan.games.iter().map(|game| {
            vec![
                html_escape(&game.machine),
                html_escape(&game.category),
                html_escape(&game.reason),
            ]
        }),
    )?;

    details_table(
        &mut w,
        &format!("Support files for roms ({})", support.len()),
        &["Name", "Type", "Status", "Why"],
        support.into_iter().map(|entry| {
            let status = if entry.availability == Availability::Missing {
                "Not in source"
            } else {
                "In source"
            };
            vec![
                html_escape(&entry.machine),
                html_escape(support_role_label(entry.role)),
                status.into(),
                html_escape(&entry.reasons.join("; ")),
            ]
        }),
    )?;

    if !preview.missing_chds.is_empty() {
        details_table(
            &mut w,
            &format!("Missing CHDs ({})", preview.missing_chds.len()),
            &["Disk", "Game", "Folder", "Reason"],
            preview.missing_chds.iter().map(|entry| {
                vec![
                    html_escape(&entry.disk),
                    html_escape(&entry.game),
                    html_escape(&entry.machine),
                    html_escape(&entry.reason),
                ]
            }),
        )?;
    }

    if !plan.unresolved.is_empty() {
        details_table(
            &mut w,
            &format!("Unresolved references ({})", plan.unresolved.len()),
            &["Name", "Kind", "Explanation"],
            plan.unresolved.iter().map(|entry| {
                vec![
                    html_escape(&entry.name),
                    html_escape(&entry.kind),
                    html_escape(&unresolved_blurb(entry)),
                ]
            }),
        )?;
    }

    if !preview.excluded_games.is_empty() {
        details_table(
            &mut w,
            &format!("Not on working list ({})", preview.excluded_games.len()),
            &["Machine", "Category", "Reason"],
            preview.excluded_games.iter().map(|game| {
                vec![
                    html_escape(&game.machine),
                    html_escape(&game.category),
                    html_escape(&game.reason),
                ]
            }),
        )?;
    }

    writeln!(w, "</body></html>")?;
    w.flush()
}

fn summary_row(w: &mut impl Write, label: &str, value: &str) -> std::io::Result<()> {
    writeln!(
        w,
        "<div><dt>{}</dt><dd>{}</dd></div>",
        html_escape(label),
        html_escape(value)
    )
}

fn details_table(
    w: &mut impl Write,
    title: &str,
    headers: &[&str],
    rows: impl Iterator<Item = Vec<String>>,
) -> std::io::Result<()> {
    writeln!(w, "<details class=\"panel\">")?;
    writeln!(w, "<summary>{}</summary>", html_escape(title))?;
    writeln!(w, "<div class=\"table-wrap\"><table>")?;
    writeln!(w, "<thead><tr>")?;
    for header in headers {
        writeln!(w, "<th>{}</th>", html_escape(header))?;
    }
    writeln!(w, "</tr></thead><tbody>")?;
    for row in rows {
        writeln!(w, "<tr>")?;
        for cell in row {
            writeln!(w, "<td>{}</td>", cell)?;
        }
        writeln!(w, "</tr>")?;
    }
    writeln!(w, "</tbody></table></div></details>")
}

fn support_role_label(role: MediaRole) -> &'static str {
    match role {
        MediaRole::Archive => "ROM zip",
        MediaRole::Disk => "CHD folder",
    }
}

fn unresolved_blurb(entry: &UnresolvedMedia) -> String {
    let game = if entry.game.is_empty() {
        "A selected game".to_string()
    } else {
        format!("“{}”", entry.game)
    };
    let base = match entry.kind.as_str() {
        "delta-parent" => format!(
            "{game} uses a delta CHD that needs base CHD “{}” in the source; it was not found.",
            entry.name
        ),
        "own-disk" | "shared-disk" | "device-disk" => format!(
            "{game} needs disk “{}” but no matching CHD was found.",
            entry.name
        ),
        "parent-archive" | "rom-parent-archive" | "own-archive" | "device-archive" => format!(
            "{game} needs ROM zip “{}” ({}) but it is not in the source.",
            entry.name,
            entry.kind.replace('-', " ")
        ),
        "dependency-cycle" | "disk-cycle" => format!(
            "Circular reference in {}: {}",
            entry.kind.replace('-', " "),
            entry.name
        ),
        _ => format!("{game} — {}: {}", entry.kind, entry.name),
    };
    format!("{base} The game stays selected; export copies what you have.")
}

fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(character),
        }
    }
    out
}

const STYLES: &str = r#"
:root { font-family: system-ui, sans-serif; line-height: 1.45; color: #1a1a1a; background: #f4f5f7; }
body { max-width: 960px; margin: 0 auto; padding: 1rem 1.25rem 2rem; }
header h1 { margin: 0 0 0.25rem; font-size: 1.5rem; }
.lede, .hint { color: #555; font-size: 0.95rem; }
.panel { background: #fff; border: 1px solid #d0d4dc; border-radius: 8px; padding: 0.75rem 1rem; margin: 0.75rem 0; }
details.panel { padding: 0; }
details.panel > summary { cursor: pointer; font-weight: 600; padding: 0.75rem 1rem; list-style: disclosure-closed; }
details.panel[open] > summary { border-bottom: 1px solid #e4e7ec; }
.table-wrap { overflow: auto; max-height: 28rem; padding: 0 1rem 1rem; }
table { width: 100%; border-collapse: collapse; font-size: 0.88rem; }
th, td { text-align: left; padding: 0.35rem 0.5rem; border-bottom: 1px solid #eceef2; vertical-align: top; }
th { position: sticky; top: 0; background: #f9fafb; }
dl.summary { display: grid; grid-template-columns: 1fr 1fr; gap: 0.35rem 1rem; margin: 0.5rem 0 0; }
dt { margin: 0; color: #666; font-size: 0.85rem; }
dd { margin: 0; font-weight: 600; }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::plan::{
        Availability, MediaRole, MetadataIdentity, PlannedGame, PlannedMedia,
    };
    use std::io::Cursor;

    fn sample_plan() -> Plan {
        Plan {
            version: 2,
            metadata: MetadataIdentity {
                mame_build: "0.289".into(),
                emulation_policy: "playable".into(),
                mame_xml_path: String::new(),
                mame_xml_sha1: String::new(),
                catver_path: String::new(),
                catver_sha1: String::new(),
            },
            source_paths: vec!["/src".into()],
            destination_path: "/dest".into(),
            selected_names_path: "/dest/selected.txt".into(),
            report_path: String::new(),
            subset_start: String::new(),
            subset_end: String::new(),
            games: vec![PlannedGame {
                machine: "galaga".into(),
                category: "Arcade".into(),
                folder: "Working".into(),
                reason: "playable".into(),
            }],
            media: vec![PlannedMedia {
                machine: "galaga".into(),
                role: MediaRole::Archive,
                availability: Availability::Present,
                source: Some("/src/galaga.zip".into()),
                destination: Some("/dest/roms/galaga.zip".into()),
                bytes: Some(1),
                reasons: vec!["Selected game".into()],
            }],
            unresolved: Vec::new(),
            conflicts: Vec::new(),
        }
    }

    #[test]
    fn html_contains_collapsible_sections() {
        let plan = sample_plan();
        let mut buf = Vec::new();
        write_document(
            Cursor::new(&mut buf),
            PlanPreviewHtml {
                plan: &plan,
                missing_chds: &[],
                excluded_games: &[],
            },
        )
        .unwrap();
        let html = String::from_utf8(buf).unwrap();
        assert!(html.contains("<details"));
        assert!(html.contains("Selected / working games"));
        assert!(html.contains("galaga"));
        assert!(!html.contains("<script"));
    }

    #[test]
    fn html_escapes_markup() {
        assert_eq!(html_escape("<a>&"), "&lt;a&gt;&amp;");
    }
}
