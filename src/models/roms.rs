use std::collections::{HashMap, HashSet};
use std::string::ToString;
use lazy_static::lazy_static;
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
        Rom { data: self, category: RomCategory::Working }
    }
    fn to_not_working_rom(self) -> Rom {
        Rom { data: self, category: RomCategory::NotWorking }
    }
    fn to_bios_rom(self) -> Rom { Rom { data: self, category: RomCategory::Bios } }
    fn to_system_rom(self) -> Rom {
        Rom { data: self, category: RomCategory::System }
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
    pub category: String,
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


#[derive(Display, Debug, PartialEq, Eq, EnumString, Clone, Copy)]
#[strum(ascii_case_insensitive)]
pub enum Status {
    Imperfect,
    Preliminary,
    Good,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct RomStatus {
    pub driver: Status,
    pub emulation: Status,
}

#[derive(PartialEq, Eq, Debug)]
pub struct Chd {
    pub name: String,
    pub status: ChdStatus,
}

#[derive(Display, Debug, PartialEq, Eq, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum ChdStatus {
    NoStatus,
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

lazy_static! {
    pub static ref SPECIAL_CASES_DEMOTE: HashSet<String> = {
        let mut m = HashSet::new();
        // demote
        m.insert("mrchalgr".to_string()); // system
        m.insert("gamefgtr".to_string()); // system
        m.insert("pjoyn50".to_string()); // system
        m.insert("pjoys30".to_string()); // system
        m.insert("pjoys60".to_string()); // system
        m.insert("quizard_10".to_string());
        m.insert("sy888b".to_string()); // System
        m
    };

    /// Roms with imperfect sound and graphics are considered
    /// not working, except the ones on this list.
    pub static ref SPECIAL_CASES_PROMOTE: HashSet<String> = {
        let mut m = HashSet::new();
        m.insert("venom".to_string()); // seems ok
        m
    };
}

lazy_static! {
    /// Excluded roms categories. Roms in these categories
    /// are categorized as [RomMetadata::System]
    pub static ref EXCLUDED_CATEGORIES: HashSet<String> = {
        let mut m = HashSet::new();
        m.insert("System".to_string());
        m.insert("Computer".to_string());
        m.insert("Handheld".to_string());
        m.insert("Board Game".to_string());
        m.insert("Game Console".to_string());
        m.insert("Calculator".to_string());
        m.insert("Misc. / Credit Card Terminal".to_string());
        m.insert("Misc. / Clock".to_string());
        m.insert("Misc. / Educational".to_string());
        m.insert("Misc. / Toy Robot".to_string());
        m.insert("Misc. / Electronic".to_string());
        m.insert("Misc. / Speech".to_string());
        m.insert("Misc. / VTR Control".to_string());
        m.insert("Music / Keyboard".to_string());
        m.insert("Music / Audio".to_string());
        m.insert("Music / Drum".to_string());
        m.insert("Music / Instruments".to_string());
        m.insert("Music / Karaoke".to_string());
        m.insert("Music / Sequencer".to_string());
        m.insert("Music / Synthesizer".to_string());
        m.insert("Music / Tone".to_string());
        m.insert("Watch / LCD Game".to_string());
        m.insert("Tabletop".to_string());
        m.insert("Utilities".to_string());
        m
    };
}

