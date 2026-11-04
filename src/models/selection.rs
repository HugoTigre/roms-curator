use crate::models::roms::{RomStatus, Status};

/// How driver and emulation status select a runnable machine.
///
/// Sound, graphics, and other feature flags are not part of this decision.
/// Imperfect features stay selected when the driver summary is accepted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmulationPolicy {
    /// Driver and emulation are each `good` or `imperfect`.
    #[default]
    Playable,
    /// Driver and emulation are both `good`.
    Strict,
}

impl EmulationPolicy {
    pub fn accepts(self, status: &RomStatus) -> bool {
        match self {
            Self::Playable => acceptable(&status.driver) && acceptable(&status.emulation),
            Self::Strict => status.driver == Status::Good && status.emulation == Status::Good,
        }
    }

    /// What this policy does to a machine that is otherwise runnable.
    pub fn effect(self) -> &'static str {
        match self {
            Self::Playable => {
                "Selected when driver status and emulation are each good or imperfect. Preliminary driver or emulation is NotWorking. Feature flags do not change this."
            }
            Self::Strict => {
                "Selected only when driver status and emulation are both good. Imperfect and preliminary are NotWorking. Feature flags do not change this."
            }
        }
    }
}

fn acceptable(status: &Status) -> bool {
    matches!(status, Status::Good | Status::Imperfect)
}

/// A CatVer group that is not an arcade game.
///
/// Arcade genres are intentionally absent. Shooter, puzzle, maze, slots, and
/// other playable genres stay eligible when the emulation policy accepts them.
pub struct CategoryPreference {
    /// CatVer group, or `Group / Subgroup`. Matches that text and its children.
    pub pattern: &'static str,
    pub effect: &'static str,
    pub reason: &'static str,
}

/// A named machine kept out of the arcade list.
pub struct MachineOverride {
    pub name: &'static str,
    pub effect: &'static str,
    pub reason: &'static str,
}

/// A machine-type rule that is not an emulation judgment.
pub struct MachineTypeRule {
    pub name: &'static str,
    pub effect: &'static str,
    pub reason: &'static str,
}

const SYSTEM_EFFECT: &str = "Categorized as System and left out of the selected arcade list.";

/// Non-arcade CatVer groups. Matching is the group itself or `pattern / child`,
/// so a later word such as `Shooter / Computer` is not treated as `Computer`.
pub const NON_ARCADE_CATEGORIES: &[CategoryPreference] = &[
    CategoryPreference {
        pattern: "System",
        effect: SYSTEM_EFFECT,
        reason: "CatVer groups computers, firmware, and devices under System.",
    },
    CategoryPreference {
        pattern: "Computer",
        effect: SYSTEM_EFFECT,
        reason: "General-purpose computers are not arcade games.",
    },
    CategoryPreference {
        pattern: "Handheld",
        effect: SYSTEM_EFFECT,
        reason: "Dedicated handheld hardware is not an arcade cabinet.",
    },
    CategoryPreference {
        pattern: "Board Game",
        effect: SYSTEM_EFFECT,
        reason: "Board-game machines are not arcade games.",
    },
    CategoryPreference {
        pattern: "Game Console",
        effect: SYSTEM_EFFECT,
        reason: "Home consoles are not arcade games.",
    },
    CategoryPreference {
        pattern: "Calculator",
        effect: SYSTEM_EFFECT,
        reason: "Calculators are not arcade games.",
    },
    CategoryPreference {
        pattern: "Misc. / Credit Card Terminal",
        effect: SYSTEM_EFFECT,
        reason: "Payment terminals are not arcade games.",
    },
    CategoryPreference {
        pattern: "Misc. / Clock",
        effect: SYSTEM_EFFECT,
        reason: "Clocks are not arcade games.",
    },
    CategoryPreference {
        pattern: "Misc. / Educational",
        effect: SYSTEM_EFFECT,
        reason: "Educational devices are not arcade games.",
    },
    CategoryPreference {
        pattern: "Misc. / Toy Robot",
        effect: SYSTEM_EFFECT,
        reason: "Toy robots are not arcade games.",
    },
    CategoryPreference {
        pattern: "Misc. / Electronic",
        effect: SYSTEM_EFFECT,
        reason: "Electronic toys are not arcade games.",
    },
    CategoryPreference {
        pattern: "Misc. / Speech",
        effect: SYSTEM_EFFECT,
        reason: "Speech devices are not arcade games.",
    },
    CategoryPreference {
        pattern: "Misc. / VTR Control",
        effect: SYSTEM_EFFECT,
        reason: "Video-recorder controls are not arcade games.",
    },
    CategoryPreference {
        pattern: "Music / Keyboard",
        effect: SYSTEM_EFFECT,
        reason: "Musical keyboards are not arcade games.",
    },
    CategoryPreference {
        pattern: "Music / Audio",
        effect: SYSTEM_EFFECT,
        reason: "Audio equipment is not an arcade game.",
    },
    CategoryPreference {
        pattern: "Music / Drum",
        effect: SYSTEM_EFFECT,
        reason: "Drum machines are not arcade games.",
    },
    CategoryPreference {
        pattern: "Music / Instruments",
        effect: SYSTEM_EFFECT,
        reason: "Instruments are not arcade games.",
    },
    CategoryPreference {
        pattern: "Music / Karaoke",
        effect: SYSTEM_EFFECT,
        reason: "Karaoke machines are not arcade games.",
    },
    CategoryPreference {
        pattern: "Music / Sequencer",
        effect: SYSTEM_EFFECT,
        reason: "Sequencers are not arcade games.",
    },
    CategoryPreference {
        pattern: "Music / Synthesizer",
        effect: SYSTEM_EFFECT,
        reason: "Synthesizers are not arcade games.",
    },
    CategoryPreference {
        pattern: "Music / Tone",
        effect: SYSTEM_EFFECT,
        reason: "Tone generators are not arcade games.",
    },
    CategoryPreference {
        pattern: "Watch / LCD Game",
        effect: SYSTEM_EFFECT,
        reason: "LCD watches and games are not arcade cabinets.",
    },
    CategoryPreference {
        pattern: "Tabletop",
        effect: SYSTEM_EFFECT,
        reason: "Tabletop electronic games are not arcade cabinets.",
    },
    CategoryPreference {
        pattern: "Utilities",
        effect: SYSTEM_EFFECT,
        reason: "Utility software is not an arcade game.",
    },
];

const OVERRIDE_REASON: &str =
    "CatVer does not place this machine in a non-arcade group, but it is a system and stays out of the selected arcade list.";

/// Systems the non-arcade CatVer groups do not catch.
pub const MACHINE_OVERRIDES: &[MachineOverride] = &[
    MachineOverride {
        name: "mrchalgr",
        effect: SYSTEM_EFFECT,
        reason: OVERRIDE_REASON,
    },
    MachineOverride {
        name: "gamefgtr",
        effect: SYSTEM_EFFECT,
        reason: OVERRIDE_REASON,
    },
    MachineOverride {
        name: "pjoyn50",
        effect: SYSTEM_EFFECT,
        reason: OVERRIDE_REASON,
    },
    MachineOverride {
        name: "pjoys30",
        effect: SYSTEM_EFFECT,
        reason: OVERRIDE_REASON,
    },
    MachineOverride {
        name: "pjoys60",
        effect: SYSTEM_EFFECT,
        reason: OVERRIDE_REASON,
    },
    MachineOverride {
        name: "quizard_10",
        effect: SYSTEM_EFFECT,
        reason: OVERRIDE_REASON,
    },
    MachineOverride {
        name: "sy888b",
        effect: SYSTEM_EFFECT,
        reason: OVERRIDE_REASON,
    },
];

pub const MACHINE_TYPE_RULES: &[MachineTypeRule] = &[
    MachineTypeRule {
        name: "bios",
        effect: "Categorized as Bios and left out of the selected arcade list.",
        reason: "A BIOS image supports other machines. It is not a game.",
    },
    MachineTypeRule {
        name: "device",
        effect: SYSTEM_EFFECT,
        reason: "MAME marks the machine isdevice=yes. Devices are support hardware.",
    },
    MachineTypeRule {
        name: "mechanical",
        effect: "Categorized as NotWorking and left out of the selected arcade list.",
        reason: "MAME marks the machine ismechanical=yes. It is not a runnable arcade game.",
    },
];

pub fn category_preference(category: &str) -> Option<&'static CategoryPreference> {
    NON_ARCADE_CATEGORIES
        .iter()
        .find(|preference| category_matches(category, preference.pattern))
}

pub fn machine_override(name: &str) -> Option<&'static MachineOverride> {
    MACHINE_OVERRIDES
        .iter()
        .find(|override_rule| override_rule.name == name)
}

fn category_matches(category: &str, pattern: &str) -> bool {
    category == pattern
        || category
            .strip_prefix(pattern)
            .is_some_and(|rest| rest.starts_with(" / "))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::core::roms_service::parse_mame_xml_from_str;
    use crate::models::roms::RomCategory::{Bios, NotWorking, System};
    use crate::parse_categories;

    use super::*;

    #[test]
    fn every_rule_names_its_effect_and_reason() {
        assert!(!EmulationPolicy::Playable.effect().is_empty());
        assert!(!EmulationPolicy::Strict.effect().is_empty());
        assert!(EmulationPolicy::Playable.effect().contains("imperfect"));
        assert!(EmulationPolicy::Strict.effect().contains("both good"));

        for preference in NON_ARCADE_CATEGORIES {
            assert!(!preference.pattern.is_empty(), "{}", preference.reason);
            assert!(preference.effect.contains("System"));
            assert!(!preference.reason.is_empty());
        }
        let mut names = Vec::new();
        for rule in MACHINE_OVERRIDES {
            assert!(rule.effect.contains("System"));
            assert!(!rule.reason.is_empty());
            names.push(rule.name);
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), MACHINE_OVERRIDES.len());
        for rule in MACHINE_TYPE_RULES {
            assert!(!rule.name.is_empty());
            assert!(!rule.effect.is_empty());
            assert!(!rule.reason.is_empty());
        }

        // The effect text has to name the category the live classifiers assign.
        let categories = parse_categories(
            "[Category]\nbios=Shooter / Gallery\ndevice=Shooter / Gallery\nmechanical=Shooter / Gallery\n",
            Path::new("catver.ini"),
        )
        .unwrap();
        let catalog = parse_mame_xml_from_str(
            Path::new("selection.xml"),
            r#"<mame build="fixture">
                <machine name="bios" isbios="yes"><driver status="good" emulation="good"/></machine>
                <machine name="device" isdevice="yes" runnable="no"/>
                <machine name="mechanical" ismechanical="yes"><driver status="good" emulation="good"/></machine>
            </mame>"#,
            &categories,
        )
            .unwrap()
            .categorize()
            .unwrap();
        let classified = [
            ("bios", Bios),
            ("device", System),
            ("mechanical", NotWorking),
        ];
        for (name, category) in classified {
            let rule = MACHINE_TYPE_RULES
                .iter()
                .find(|rule| rule.name == name)
                .unwrap();
            assert!(
                rule.effect.contains(category.to_string().as_str()),
                "{} effect {} does not name {category}",
                rule.name,
                rule.effect
            );
            assert_eq!(catalog.roms[name].category, category);
        }
    }

    #[test]
    fn arcade_genres_stay_eligible_and_groups_match_by_prefix() {
        assert!(category_preference("Computer / Home").is_some());
        assert!(category_preference("System / BIOS").is_some());
        assert!(category_preference("Music / Keyboard / Digital").is_some());
        assert!(category_preference("Shooter / Computer").is_none());
        assert!(category_preference("Slot Machine / Video Slot").is_none());
        assert!(category_preference("Puzzle / Maze").is_none());
        assert!(category_preference("Music / Keyboard Extra").is_none());
    }
}
