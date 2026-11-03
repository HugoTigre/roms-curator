use std::collections::HashMap;
use strum_macros::{Display, EnumString};

pub type Roms = HashMap<String, Rom>;
pub type UnfilteredRoms = HashMap<String, RomData>;

pub trait RomDataExt {
    fn to_working_rom(self) -> Rom;
    fn to_not_working_rom(self) -> Rom;
    fn to_bios_rom(self) -> Rom;
    fn to_system_rom(self) -> Rom;
}

impl RomDataExt for RomData {
    fn to_working_rom(self) -> Rom {
        Rom {
            data: self,
            category: RomCategory::Working,
        }
    }
    fn to_not_working_rom(self) -> Rom {
        Rom {
            data: self,
            category: RomCategory::NotWorking,
        }
    }
    fn to_bios_rom(self) -> Rom {
        Rom {
            data: self,
            category: RomCategory::Bios,
        }
    }
    fn to_system_rom(self) -> Rom {
        Rom {
            data: self,
            category: RomCategory::System,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct RomData {
    pub status: Option<RomStatus>,
    pub is_bios: bool,
    pub is_system: bool,
    pub is_mechanical: bool,
    pub features: Vec<Feature>,
    pub clone_of: Option<String>,
    pub rom_of: Option<String>,
    pub chd: Vec<Chd>,
    /// CatVer category. [`UNKNOWN_CATEGORY`] when the machine has no `[Category]` entry.
    pub category: String,
}

/// Category stored when CatVer has no entry for a machine.
pub const UNKNOWN_CATEGORY: &str = "Unknown";

/// Categorized machines plus the metadata needed to see which source was parsed.
#[derive(Debug)]
pub struct Catalog {
    /// `build` attribute from the MAME XML, stored as written. Any version is accepted.
    pub mame_build: String,
    pub roms: Roms,
    /// Machine names with no `[Category]` entry, sorted by name.
    pub unknown_categories: Vec<String>,
}

impl IntoIterator for Catalog {
    type Item = (String, Rom);
    type IntoIter = std::collections::hash_map::IntoIter<String, Rom>;

    fn into_iter(self) -> Self::IntoIter {
        self.roms.into_iter()
    }
}

#[derive(Debug)]
pub struct Rom {
    pub data: RomData,
    pub category: RomCategory,
}

#[derive(Display, PartialEq, Eq, Debug)]
pub enum RomCategory {
    Working,
    NotWorking,
    Bios,
    /// Includes roms with 'isdevice=true'
    /// Includes System / Computer categories
    System,
    /// problem mapping rom, ideally there should not be any rom here
    /// If there are it probably means different version of mame files (`listxml.xml` and `catver.ini`)
    UnCategorized,
}

#[derive(Display, Debug, PartialEq, Eq, EnumString, Clone)]
#[strum(ascii_case_insensitive)]
pub enum Status {
    Imperfect,
    Preliminary,
    Good,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct RomStatus {
    pub driver: Status,
    pub emulation: Status,
}

#[derive(PartialEq, Eq, Debug)]
pub struct Chd {
    pub name: String,
    /// Dump quality. Omitted MAME `status` means [`ChdStatus::Good`].
    pub status: ChdStatus,
    /// MAME `optional="yes"`. Omitted means the disk is required.
    pub optional: bool,
    /// Parent disk named by MAME `merge`, when this disk shares that dump.
    pub merge: Option<String>,
    pub sha1: Option<String>,
}

#[derive(Display, Debug, PartialEq, Eq, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum ChdStatus {
    Good,
    NoDump,
    BadDump,
}

#[derive(PartialEq, Eq, Debug)]
pub struct Feature {
    pub typ: String,
    pub status: FeatureStatus,
}

#[derive(Display, Debug, PartialEq, Eq, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum FeatureStatus {
    Imperfect,
    Unemulated,
}
