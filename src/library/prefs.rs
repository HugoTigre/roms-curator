//! Saved library choices that are not part of the ROM index.

use std::error::Error;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::paths::library_prefs_path;

/// Last ScreenScraper allowance we have actually seen. Missing fields stay unknown.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ScreenScraperQuota {
    pub max_per_minute: Option<u32>,
    pub max_per_day: Option<u32>,
    pub used_today: Option<u32>,
    pub max_misses_per_day: Option<u32>,
    pub misses_today: Option<u32>,
    pub max_threads: Option<u32>,
    pub max_download_kbps: Option<u32>,
}

impl ScreenScraperQuota {
    /// How many games fit in the allowance a response has already reported.
    ///
    /// `calls_per_game` is 1 for a text lookup and 2 when that lookup also
    /// downloads a cover. Misses and daily requests both count. `None` means
    /// this account has not reported either cap yet.
    pub fn games_that_fit(&self, calls_per_game: u32) -> Option<u64> {
        let calls = u64::from(calls_per_game.max(1));
        let requests = match (self.max_per_day, self.used_today) {
            (Some(max), Some(used)) => Some(u64::from(max.saturating_sub(used)) / calls),
            _ => None,
        };
        let misses = match (self.max_misses_per_day, self.misses_today) {
            (Some(max), Some(used)) => Some(u64::from(max.saturating_sub(used))),
            _ => None,
        };
        match (requests, misses) {
            (Some(requests), Some(misses)) => Some(requests.min(misses)),
            (Some(requests), None) => Some(requests),
            (None, Some(misses)) => Some(misses),
            (None, None) => None,
        }
    }
}

/// Choices that survive deleting the index.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LibraryPrefs {
    /// When false, a scrape stores text only and does not download covers.
    #[serde(default = "default_download_covers")]
    pub download_covers: bool,
    /// Empty until a ScreenScraper response includes `ssuser`.
    #[serde(default)]
    pub screenscraper: ScreenScraperQuota,
}

fn default_download_covers() -> bool {
    true
}

impl Default for LibraryPrefs {
    fn default() -> Self {
        Self {
            download_covers: true,
            screenscraper: ScreenScraperQuota::default(),
        }
    }
}

impl LibraryPrefs {
    pub fn load() -> Result<Self, Box<dyn Error>> {
        Self::load_at(&library_prefs_path()?)
    }

    pub fn load_at(path: &Path) -> Result<Self, Box<dyn Error>> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path).map_err(|error| {
            format!(
                "Cannot read library preferences {}: {error}",
                path.display()
            )
        })?;
        serde_json::from_str(&text).map_err(|error| {
            format!(
                "Library preferences {} are not valid: {error}",
                path.display()
            )
            .into()
        })
    }

    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        self.save_at(&library_prefs_path()?)
    }

    pub fn save_at(&self, path: &Path) -> Result<(), Box<dyn Error>> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_vec_pretty(self)?;
        fs::write(path, json).map_err(|error| {
            format!(
                "Cannot write library preferences {}: {error}",
                path.display()
            )
            .into()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_downloads_covers() {
        let dir = tempfile::tempdir().unwrap();
        let prefs = LibraryPrefs::load_at(&dir.path().join("library.json")).unwrap();
        assert!(prefs.download_covers);
    }

    #[test]
    fn saved_choice_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        LibraryPrefs {
            download_covers: false,
            ..LibraryPrefs::default()
        }
        .save_at(&path)
        .unwrap();
        let prefs = LibraryPrefs::load_at(&path).unwrap();
        assert!(!prefs.download_covers);
    }

    #[test]
    fn games_that_fit_uses_the_tighter_known_cap() {
        let quota = ScreenScraperQuota {
            max_per_day: Some(100),
            used_today: Some(10),
            max_misses_per_day: Some(20),
            misses_today: Some(5),
            ..ScreenScraperQuota::default()
        };
        assert_eq!(quota.games_that_fit(1), Some(15));
        assert_eq!(quota.games_that_fit(2), Some(15));
        let requests_only = ScreenScraperQuota {
            max_per_day: Some(100),
            used_today: Some(10),
            ..ScreenScraperQuota::default()
        };
        assert_eq!(requests_only.games_that_fit(2), Some(45));
        assert_eq!(ScreenScraperQuota::default().games_that_fit(1), None);
    }
}
