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
    /// ScreenScraper system id for a checksum lookup.
    ///
    /// Empty for Arcade and PC, which use their own sources.
    pub screenscraper_id: Option<u32>,
}

const fn cartridge(id: &'static str, name: &'static str, screenscraper_id: u32) -> Platform {
    Platform {
        id,
        name,
        uses_mame_version: false,
        screenscraper_id: Some(screenscraper_id),
    }
}

const fn listed(id: &'static str, name: &'static str) -> Platform {
    Platform {
        id,
        name,
        uses_mame_version: false,
        screenscraper_id: None,
    }
}

const PLATFORMS: &[Platform] = &[
    cartridge("snes", "Nintendo - Super Nintendo", 4),
    Platform {
        id: "arcade",
        name: "Arcade - MAME",
        uses_mame_version: true,
        screenscraper_id: None,
    },
    listed("pc", "PC"),
    cartridge("amiga", "Commodore - Amiga", 64),
    cartridge("atari2600", "Atari - 2600", 26),
    cartridge("atari5200", "Atari - 5200", 40),
    cartridge("atari7800", "Atari - 7800", 41),
    cartridge("jaguar", "Atari - Jaguar", 27),
    cartridge("lynx", "Atari - Lynx", 28),
    cartridge("atarist", "Atari - ST", 42),
    cartridge("c64", "Commodore - 64", 66),
    cartridge("gb", "Nintendo - Game Boy", 9),
    cartridge("gba", "Nintendo - Game Boy Advance", 12),
    cartridge("gbc", "Nintendo - Game Boy Color", 10),
    cartridge("gamegear", "Sega - Game Gear", 21),
    cartridge("mastersystem", "Sega - Master System - Mark III", 2),
    cartridge("megadrive", "Sega - Mega Drive - Genesis", 1),
    cartridge("ngpc", "SNK - NeoGeo Pocket Color", 82),
    cartridge("nes", "Nintendo - Entertainment System", 3),
    cartridge("pcengine", "NEC - PC Engine - TurboGrafx-16", 31),
    cartridge("wonderswancolor", "Bandai - WonderSwan Color", 46),
    cartridge("spectrumtape", "Sinclair - ZX Spectrum", 76),
    cartridge("spectrum", "Sinclair - ZX Spectrum +3", 76),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_ids_are_unique() {
        let mut ids: Vec<_> = all().iter().map(|platform| platform.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all().len());
        assert!(find("snes").unwrap().screenscraper_id == Some(4));
        assert_eq!(find("nes").unwrap().screenscraper_id, Some(3));
        assert!(find("dsi").is_none());
        assert!(find("gb").unwrap().screenscraper_id == Some(9));
        assert_eq!(find("spectrumtape").unwrap().screenscraper_id, Some(76));
        assert_eq!(find("spectrum").unwrap().name, "Sinclair - ZX Spectrum +3");
        assert_eq!(find("spectrum").unwrap().screenscraper_id, Some(76));
    }
}
