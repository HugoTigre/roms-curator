//! Platforms the Library screen can index.
//!
//! Add a platform here, then give it a [`super::enrich::MetadataProvider`].
//! The desktop app reads this list instead of keeping its own copy.

/// One collection the user can point at ROM folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    pub id: &'static str,
    pub name: &'static str,
    /// Scraping this platform needs a MAME listxml and CatVer version.
    pub uses_mame_version: bool,
}

const PLATFORMS: &[Platform] = &[
    Platform {
        id: "snes",
        name: "Super Nintendo",
        uses_mame_version: false,
    },
    Platform {
        id: "arcade",
        name: "Arcade",
        uses_mame_version: true,
    },
];

pub fn all() -> &'static [Platform] {
    PLATFORMS
}

pub fn find(id: &str) -> Option<&'static Platform> {
    PLATFORMS.iter().find(|platform| platform.id == id)
}

pub fn require(id: &str) -> Result<&'static Platform, String> {
    find(id).ok_or_else(|| format!("Unknown platform {id}."))
}
