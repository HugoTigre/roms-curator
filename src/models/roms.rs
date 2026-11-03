use std::collections::HashMap;
use strum_macros::{Display, EnumString};

use super::dependencies::DependencySet;
use super::disks::DiskSet;

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
            selected: true,
        }
    }
    fn to_not_working_rom(self) -> Rom {
        Rom {
            data: self,
            category: RomCategory::NotWorking,
            selected: false,
        }
    }
    fn to_bios_rom(self) -> Rom {
        Rom {
            data: self,
            category: RomCategory::Bios,
            selected: false,
        }
    }
    fn to_system_rom(self) -> Rom {
        Rom {
            data: self,
            category: RomCategory::System,
            selected: false,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RomData {
    pub status: Option<RomStatus>,
    pub is_bios: bool,
    pub is_system: bool,
    pub is_mechanical: bool,
    pub features: Vec<Feature>,
    /// Game parent from MAME `cloneof`.
    pub clone_of: Option<String>,
    /// ROM parent from MAME `romof`. This is often the clone parent or a BIOS.
    pub rom_of: Option<String>,
    /// `<device_ref name>` values, in document order.
    pub devices: Vec<String>,
    /// `<rom>` entries. These are dump requirements, not the copy-folder category.
    pub roms: Vec<RomFile>,
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
    /// Every parsed machine, including BIOS, devices, and games that were not selected.
    pub roms: Roms,
    /// Visible game list: machine names selected from their own emulation policy, sorted by name.
    ///
    /// A parent, BIOS, or device can be absent from this list and still be named by
    /// [`RomData::clone_of`], [`RomData::rom_of`], or [`RomData::devices`].
    /// [`Catalog::dependencies`] names the merged archives those links require.
    pub selected: Vec<String>,
    /// Merged archives and support files required by [`Catalog::selected`].
    ///
    /// A clone names its parent archive, not its own name. Dump quality does not
    /// change [`Rom::category`] or this list.
    pub dependencies: DependencySet,
    /// CHD files required by [`Catalog::selected`].
    ///
    /// A shared disk names the ancestor that stores the one copy. A clone disk
    /// with its own content also names the parent's same-named non-optional disk
    /// a delta CHD needs. When that parent has disks and none matches, it is missing.
    /// Optional disks are omitted. [`DiskSet::missing`] is unresolved media,
    /// not a change to [`Rom::category`].
    pub disks: DiskSet,
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
    /// Copy-folder classification from this machine's own type and emulation policy.
    /// Dependency resolution does not change it.
    pub category: RomCategory,
    /// This machine is on the visible game list. See [`Catalog::selected`].
    pub selected: bool,
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

#[derive(PartialEq, Eq, Debug, Clone)]
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

#[derive(Display, Debug, PartialEq, Eq, PartialOrd, Ord, EnumString, Clone, Copy)]
#[strum(ascii_case_insensitive)]
pub enum ChdStatus {
    Good,
    NoDump,
    BadDump,
}

/// One MAME `<rom>` entry.
///
/// `status` uses the same dump vocabulary as a disk. Omitted `status` means [`ChdStatus::Good`].
/// Omitted `size` is recorded as absent; a nodump entry often has no size.
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct RomFile {
    pub name: String,
    pub size: Option<u64>,
    pub crc: Option<String>,
    pub sha1: Option<String>,
    /// Parent ROM named by MAME `merge`, when this file shares that dump.
    pub merge: Option<String>,
    /// BIOS set named by MAME `bios`, when this file belongs to that set.
    pub bios: Option<String>,
    pub status: ChdStatus,
    /// MAME `optional="yes"`. Omitted means the ROM is required.
    pub optional: bool,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Feature {
    pub typ: String,
    pub status: FeatureStatus,
}

#[derive(Display, Debug, PartialEq, Eq, EnumString, Clone)]
#[strum(ascii_case_insensitive)]
pub enum FeatureStatus {
    Imperfect,
    Unemulated,
}
