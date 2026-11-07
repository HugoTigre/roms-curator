//! Field checks for the desktop form. Messages are for the person using the app.

use std::path::Path;

use serde::Serialize;

use crate::core::metadata_fetch::parse_mame_version;

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ExportFormIssues {
    pub mame_version: Option<String>,
    pub source_path: Option<String>,
    pub destination_path: Option<String>,
    pub report_path: Option<String>,
    pub plan_html_path: Option<String>,
    pub subset_start: Option<String>,
    pub subset_end: Option<String>,
}

/// Check the values the desktop form collects before a plan or export.
pub fn check_export_form(
    mame_version: &str,
    source_path: &str,
    destination_path: &str,
    report_path: &str,
    plan_html_path: &str,
    subset_start: &str,
    subset_end: &str,
) -> ExportFormIssues {
    let mut issues = ExportFormIssues {
        mame_version: mame_version_issue(mame_version),
        source_path: source_issue(source_path),
        destination_path: destination_issue(destination_path),
        report_path: report_issue(report_path),
        plan_html_path: plan_html_issue(plan_html_path),
        ..subset_issues(subset_start, subset_end)
    };

    if issues.source_path.is_none()
        && issues.destination_path.is_none()
        && issues.report_path.is_none()
        && issues.plan_html_path.is_none()
    {
        apply_relationship_issues(
            &mut issues,
            source_path.trim(),
            destination_path.trim(),
            report_path.trim(),
            plan_html_path.trim(),
        );
    }

    issues
}

fn mame_version_issue(value: &str) -> Option<String> {
    parse_mame_version(value)
        .err()
        .map(|error| error.to_string())
}

fn source_issue(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return Some("Choose the folder that contains your ROM set.".into());
    }
    match std::fs::metadata(value) {
        Ok(metadata) if metadata.is_dir() => None,
        Ok(_) => Some("Source needs to be a folder, not a file.".into()),
        Err(_) => Some("That source folder was not found.".into()),
    }
}

fn destination_issue(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return Some("Choose a folder where the working set should be saved.".into());
    }
    let path = Path::new(value);
    if path.is_file() {
        return Some("Export destination needs to be a folder, not a file.".into());
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Some("The folder above the export destination was not found.".into());
        }
    }
    None
}

fn report_issue(value: &str) -> Option<String> {
    new_output_file_issue(value, ".md", "Report", "Markdown", "report")
}

fn plan_html_issue(value: &str) -> Option<String> {
    new_output_file_issue(value, ".html", "Plan HTML", "HTML", "plan preview HTML")
}

fn new_output_file_issue(
    value: &str,
    extension: &str,
    label: &str,
    kind: &str,
    choose_hint: &str,
) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if !value.to_ascii_lowercase().ends_with(extension) {
        return Some(format!("{label} file needs to end in {extension}."));
    }
    let path = Path::new(value);
    if path.is_dir() {
        return Some(format!(
            "{label} path needs to be a {kind} file, not a folder."
        ));
    }
    if path.is_file() {
        return Some(format!(
            "A file already exists at that path. Choose a new {choose_hint} file name."
        ));
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Some(format!(
                "Choose a {label} path in a folder that already exists."
            ));
        }
    }
    None
}

fn subset_issues(start: &str, end: &str) -> ExportFormIssues {
    let subset_start = subset_name_issue(start);
    let mut subset_end = subset_name_issue(end);
    if subset_start.is_none() && subset_end.is_none() {
        let start = start.trim().to_ascii_lowercase();
        let end = end.trim().to_ascii_lowercase();
        if !start.is_empty() && !end.is_empty() && start > end {
            subset_end = Some("This name sorts before the start. For example, ac then az.".into());
        }
    }
    ExportFormIssues {
        subset_start,
        subset_end,
        ..Default::default()
    }
}

fn subset_name_issue(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        None
    } else {
        Some(
            "Use a machine name such as pacman or sf2. Letters, digits, and underscores only."
                .into(),
        )
    }
}

/// Overlap and report placement only. Does not walk source or destination trees;
/// preview and export still run the full path check.
fn apply_relationship_issues(
    issues: &mut ExportFormIssues,
    source: &str,
    destination: &str,
    report: &str,
    plan_html: &str,
) {
    let source_path = match crate::core::paths::resolve(Path::new(source)) {
        Ok(path) => path,
        Err(_) => {
            issues.source_path = Some("That source folder could not be checked.".into());
            return;
        }
    };
    let destination_path = match crate::core::paths::resolve(Path::new(destination)) {
        Ok(path) => path,
        Err(_) => {
            issues.destination_path = Some("That export folder could not be checked.".into());
            return;
        }
    };
    if source_path == destination_path
        || destination_path.starts_with(&source_path)
        || source_path.starts_with(&destination_path)
    {
        issues.destination_path =
            Some("Export destination must be a different folder from the source.".into());
    }
    check_output_outside_trees(
        &mut issues.report_path,
        report,
        "That report path could not be checked.",
        "Report file must sit outside the source folder.",
        "Report file must sit outside the export folder.",
        &source_path,
        &destination_path,
    );
    check_output_outside_trees(
        &mut issues.plan_html_path,
        plan_html,
        "That plan HTML path could not be checked.",
        "Plan HTML file must sit outside the source folder.",
        "Plan HTML file must sit outside the export folder.",
        &source_path,
        &destination_path,
    );
}

fn check_output_outside_trees(
    issue: &mut Option<String>,
    path_str: &str,
    resolve_error: &str,
    inside_source: &str,
    inside_destination: &str,
    source_path: &std::path::Path,
    destination_path: &std::path::Path,
) {
    if path_str.is_empty() {
        return;
    }
    let resolved = match crate::core::paths::resolve(Path::new(path_str)) {
        Ok(path) => path,
        Err(_) => {
            *issue = Some(resolve_error.into());
            return;
        }
    };
    if resolved.starts_with(source_path) {
        *issue = Some(inside_source.into());
    } else if resolved.starts_with(destination_path) {
        *issue = Some(inside_destination.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issues(
        version: &str,
        source: &str,
        destination: &str,
        report: &str,
        start: &str,
        end: &str,
    ) -> ExportFormIssues {
        check_export_form(version, source, destination, report, "", start, end)
    }

    #[test]
    fn mame_version_accepts_the_usual_forms() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        std::fs::create_dir(&source).unwrap();
        for version in ["0.289", "289", "mame0289", "0.289.0"] {
            let found = issues(
                version,
                source.to_str().unwrap(),
                destination.to_str().unwrap(),
                "",
                "",
                "",
            );
            assert!(
                found.mame_version.is_none(),
                "{version}: {:?}",
                found.mame_version
            );
        }
    }

    #[test]
    fn mame_version_rejects_an_unknown_shape() {
        let found = issues("latest", "", "", "", "", "");
        let message = found.mame_version.unwrap();
        assert!(message.contains("0.289"), "{message}");
    }

    #[test]
    fn mame_version_rejects_a_non_zero_major() {
        let found = issues("1.289", "", "", "", "", "");
        let message = found.mame_version.unwrap();
        assert!(message.contains("expected 0.xxx"), "{message}");
    }

    #[test]
    fn subset_explains_the_name_range() {
        let bad = issues("0.289", "", "", "", "pac-man", "f");
        assert!(bad.subset_start.unwrap().contains("pacman"));

        let order = issues("0.289", "", "", "", "f", "a");
        assert!(order.subset_end.unwrap().contains("ac then az"));

        let ok = issues("0.289", "", "", "", "SF2", "sv0");
        assert!(ok.subset_start.is_none());
        assert!(ok.subset_end.is_none());
    }

    #[test]
    fn paths_must_be_folders_and_must_not_overlap() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let file = root.path().join("not-a-folder");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(&file, b"x").unwrap();

        let missing = issues(
            "0.289",
            root.path().join("missing").to_str().unwrap(),
            root.path().join("out").to_str().unwrap(),
            "",
            "",
            "",
        );
        assert!(missing.source_path.unwrap().contains("not found"));

        let source_file = issues(
            "0.289",
            file.to_str().unwrap(),
            root.path().join("out").to_str().unwrap(),
            "",
            "",
            "",
        );
        assert!(source_file.source_path.unwrap().contains("folder"));

        let same = issues(
            "0.289",
            source.to_str().unwrap(),
            source.to_str().unwrap(),
            "",
            "",
            "",
        );
        assert!(same.destination_path.unwrap().contains("different folder"));
    }

    #[test]
    fn report_must_be_a_new_markdown_file_outside_the_export() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&destination).unwrap();

        let extension = issues(
            "0.289",
            source.to_str().unwrap(),
            destination.to_str().unwrap(),
            root.path().join("notes.txt").to_str().unwrap(),
            "",
            "",
        );
        assert!(extension.report_path.unwrap().contains(".md"));

        let inside = issues(
            "0.289",
            source.to_str().unwrap(),
            destination.to_str().unwrap(),
            destination.join("report.md").to_str().unwrap(),
            "",
            "",
        );
        assert!(inside.report_path.unwrap().contains("outside the export"));

        let in_source = issues(
            "0.289",
            source.to_str().unwrap(),
            destination.to_str().unwrap(),
            source.join("report.md").to_str().unwrap(),
            "",
            "",
        );
        assert!(in_source
            .report_path
            .unwrap()
            .contains("outside the source"));
    }

    #[test]
    fn plan_html_must_be_a_new_html_file_outside_the_export() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&destination).unwrap();

        let extension = check_export_form(
            "0.289",
            source.to_str().unwrap(),
            destination.to_str().unwrap(),
            "",
            root.path().join("preview.txt").to_str().unwrap(),
            "",
            "",
        );
        assert!(extension.plan_html_path.unwrap().contains(".html"));

        let inside = check_export_form(
            "0.289",
            source.to_str().unwrap(),
            destination.to_str().unwrap(),
            "",
            destination.join("preview.html").to_str().unwrap(),
            "",
            "",
        );
        assert!(inside
            .plan_html_path
            .unwrap()
            .contains("outside the export"));
    }
}
