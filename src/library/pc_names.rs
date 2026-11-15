//! Turn a loose PC archive name into a title ScreenScraper can be compared with.
//!
//! PC folders are not No-Intro dumps. A zip called `Infernax.v1.05.017.zip` or
//! `Streets of Rage 4 MODDED.zip` still names the game, with a version or a note
//! stuck on the end. A match is kept only when that cleaned name and a returned
//! title are the same after punctuation and edition words are ignored.

use std::path::Path;

const TRAILING: &[&str] = &[
    "game of the year",
    "definitive edition",
    "deluxe edition",
    "special edition",
    "remastered",
    "modified",
    "modded",
    "deluxe",
    "goty",
    "hd",
];

/// Search text for a `.zip` or `.7z` file name. Other files are not PC games.
pub(crate) fn pc_search_text(file_name: &str) -> Option<String> {
    let name = Path::new(file_name).file_name()?.to_str()?;
    let (stem, extension) = name.rsplit_once('.')?;
    if !extension.eq_ignore_ascii_case("zip") && !extension.eq_ignore_ascii_case("7z") {
        return None;
    }
    if stem.is_empty() {
        return None;
    }
    let cleaned = strip_trailing_phrases(&collapse(&separate_letters_digits(
        &strip_versions(&strip_groups(stem)).replace('_', " "),
    )));
    let cleaned = trim_edges(&cleaned);
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// True when both names are the same game, not merely sharing a word.
pub(crate) fn titles_match(query: &str, title: &str) -> bool {
    let left = title_core(query);
    let right = title_core(title);
    !left.is_empty() && left == right
}

fn title_core(text: &str) -> String {
    strip_trailing_phrases(&normalize_title(text))
}

fn normalize_title(text: &str) -> String {
    let expanded = text.to_lowercase().replace('&', " and ");
    let mut out = String::new();
    for ch in expanded.chars() {
        if ch.is_ascii_alphanumeric() || ch.is_whitespace() {
            out.push(ch);
        } else if ch == '\'' || ch == '\u{2019}' || ch == '.' {
            // Drop apostrophes and dots so "G.I. Joe" and "GI Joe" compare equal.
        } else {
            out.push(' ');
        }
    }
    collapse(&out)
}

fn strip_groups(input: &str) -> String {
    let mut out = String::new();
    let mut depth = 0i32;
    for ch in input.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

fn strip_versions(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        let boundary = index == 0 || !chars[index - 1].is_ascii_alphanumeric();
        let version = boundary
            && chars[index].eq_ignore_ascii_case(&'v')
            && chars.get(index + 1).is_some_and(char::is_ascii_digit);
        if version {
            index += 1;
            while chars
                .get(index)
                .is_some_and(|ch| ch.is_ascii_digit() || *ch == '.' || *ch == '_')
            {
                index += 1;
            }
            continue;
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

fn separate_letters_digits(text: &str) -> String {
    let mut out = String::new();
    let mut previous_letter = false;
    for ch in text.chars() {
        if ch.is_ascii_digit() && previous_letter {
            out.push(' ');
        }
        previous_letter = ch.is_ascii_alphabetic();
        out.push(ch);
    }
    out
}

fn strip_trailing_phrases(text: &str) -> String {
    let mut current = collapse(text);
    loop {
        let lower = current.to_lowercase();
        let Some(phrase) = TRAILING.iter().find(|phrase| {
            let needle = format!(" {phrase}");
            lower.ends_with(&needle) && lower.chars().count() > needle.chars().count()
        }) else {
            break;
        };
        let needle_len = phrase.chars().count() + 1;
        let keep = current.chars().count() - needle_len;
        current = collapse(&current.chars().take(keep).collect::<String>());
    }
    current
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn trim_edges(text: &str) -> String {
    text.trim_matches(|ch: char| ch.is_whitespace() || matches!(ch, '.' | '-' | '_'))
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_versions_notes_and_build_numbers() {
        assert_eq!(
            pc_search_text("Blasphemous 2.zip").as_deref(),
            Some("Blasphemous 2")
        );
        assert_eq!(
            pc_search_text("Contra Operation Galuga (Build 14178934).zip").as_deref(),
            Some("Contra Operation Galuga")
        );
        assert_eq!(
            pc_search_text("Infernax.v1.05.017.zip").as_deref(),
            Some("Infernax")
        );
        assert_eq!(
            pc_search_text("Earthion v5.3.0.zip").as_deref(),
            Some("Earthion")
        );
        assert_eq!(
            pc_search_text("Kill The Crows - v1_5.zip").as_deref(),
            Some("Kill The Crows")
        );
        assert_eq!(
            pc_search_text("Octopath Traveler0.zip").as_deref(),
            Some("Octopath Traveler 0")
        );
        assert_eq!(
            pc_search_text("Streets of Rage 4 MODDED.zip").as_deref(),
            Some("Streets of Rage 4")
        );
        assert_eq!(
            pc_search_text("The Eternal Castle [REMASTERED].zip").as_deref(),
            Some("The Eternal Castle")
        );
        assert_eq!(
            pc_search_text("Raiden III x MIKADO MANIAX Deluxe Edition.zip").as_deref(),
            Some("Raiden III x MIKADO MANIAX")
        );
        assert_eq!(pc_search_text("MFE.zip").as_deref(), Some("MFE"));
        assert_eq!(pc_search_text("notes.txt"), None);
    }

    #[test]
    fn accepts_the_same_title_and_rejects_a_longer_one() {
        assert!(titles_match("Sea of Stars", "Sea of Stars"));
        assert!(titles_match(
            "Castlevania Belmonts Curse",
            "Castlevania: Belmont's Curse"
        ));
        assert!(titles_match(
            "GI Joe Wrath of Cobra",
            "G.I. Joe: Wrath of Cobra"
        ));
        assert!(titles_match(
            "The Eternal Castle",
            "The Eternal Castle Remastered"
        ));
        assert!(titles_match("Streets of Rage 4", "Streets of Rage 4"));
        assert!(!titles_match("Return", "Return to Monkey Island"));
        assert!(!titles_match("MFE", "Final Fantasy"));
    }
}
