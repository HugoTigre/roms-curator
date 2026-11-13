//! Arcade text from a local MAME listxml and CatVer file.
//!
//! A `.zip` or `.7z` is the machine named by its file stem. Other files, including
//! CHD dumps, are left unmatched. Covers stay empty until a picture source exists.

use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::str::FromStr;

use quick_xml::escape::resolve_xml_entity;
use quick_xml::events::{BytesRef, Event};
use quick_xml::Reader;
use quick_xml::XmlVersion;

use crate::models::roms::{RomStatus, Status};
use crate::models::selection::{category_preference, machine_override, EmulationPolicy};
use crate::parse_categories;
use crate::RomCategories;

use super::super::enrich::{GameCondition, GameMetadata, Lookup, MetadataProvider, RomQuery};

pub struct ArcadeCatalog {
    games: HashMap<String, ArcadeGame>,
}

struct ArcadeGame {
    title: String,
    year: Option<String>,
    publisher: Option<String>,
    condition: GameCondition,
}

impl ArcadeCatalog {
    /// Read the machines once. Lookups after that do not touch the files.
    pub fn load(xml_path: &Path, catver_path: &Path) -> Result<Self, Box<dyn Error>> {
        let categories = parse_categories(&std::fs::read_to_string(catver_path)?, catver_path)?;
        let file = File::open(xml_path)
            .map_err(|error| format!("Cannot read MAME list {}: {error}", xml_path.display()))?;
        let games = parse_machines(BufReader::new(file), xml_path, &categories)?;
        Ok(Self { games })
    }

    fn lookup_name(&self, file_name: &str) -> Lookup {
        let Some(name) = machine_stem(file_name) else {
            return Lookup::Missing;
        };
        let Some(game) = self.games.get(&name) else {
            return Lookup::Missing;
        };
        Lookup::Found(GameMetadata {
            provider: "mame".into(),
            provider_id: name.clone(),
            title: game.title.clone(),
            year: game.year.clone(),
            publisher: game.publisher.clone(),
            thumbnail: None,
            playable: Some(game.condition.is_playable()),
            condition: Some(game.condition),
            link: Some(format!(
                "https://adb.arcadeitalia.net/dettaglio_mame.php?game_name={name}"
            )),
        })
    }
}

impl MetadataProvider for ArcadeCatalog {
    fn supports(&self, platform: &str) -> bool {
        platform == "arcade"
    }

    fn wants_content_identity(&self) -> bool {
        false
    }

    fn lookup(&self, platform: &str, rom: &RomQuery) -> Result<Lookup, Box<dyn Error>> {
        if platform != "arcade" {
            return Err(format!("Arcade enrichment is not implemented for {platform}").into());
        }
        Ok(self.lookup_name(rom.name))
    }
}

fn machine_stem(file_name: &str) -> Option<String> {
    let name = Path::new(file_name).file_name()?.to_str()?;
    let (stem, extension) = name.rsplit_once('.')?;
    if !extension.eq_ignore_ascii_case("zip") && !extension.eq_ignore_ascii_case("7z") {
        return None;
    }
    if stem.is_empty() {
        return None;
    }
    Some(stem.to_ascii_lowercase())
}

struct MachineHeader {
    name: String,
    is_bios: bool,
    is_device: bool,
    is_mechanical: bool,
}

struct MachineText {
    title: String,
    year: Option<String>,
    publisher: Option<String>,
    status: Option<RomStatus>,
}

fn parse_machines<R: BufRead>(
    reader: R,
    source: &Path,
    categories: &RomCategories,
) -> Result<HashMap<String, ArcadeGame>, Box<dyn Error>> {
    let mut xml = Reader::from_reader(reader);
    // Leave spaces around `&amp;` in place. The field is trimmed once it ends.
    let mut games = HashMap::new();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(event)) if is_local(&event, "machine") => {
                let header = machine_header(&event)?;
                let text = read_machine_body(&mut xml, source)?;
                insert_game(&mut games, header, text, categories, source)?;
            }
            Ok(Event::Empty(event)) if is_local(&event, "machine") => {
                let header = machine_header(&event)?;
                let text = MachineText {
                    title: header.name.clone(),
                    year: None,
                    publisher: None,
                    status: None,
                };
                insert_game(&mut games, header, text, categories, source)?;
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return Err(format!("Cannot parse MAME XML {}: {error}", source.display()).into());
            }
        }
    }
    Ok(games)
}

fn insert_game(
    games: &mut HashMap<String, ArcadeGame>,
    header: MachineHeader,
    text: MachineText,
    categories: &RomCategories,
    source: &Path,
) -> Result<(), Box<dyn Error>> {
    let name = header.name.to_ascii_lowercase();
    if games.contains_key(&name) {
        return Err(format!(
            "Machine '{name}' in {}: duplicate machine name",
            source.display()
        )
        .into());
    }
    let category = categories.get(&name).map(String::as_str);
    let condition = classify(&name, &header, text.status.as_ref(), category);
    games.insert(
        name,
        ArcadeGame {
            title: text.title,
            year: text.year,
            publisher: text.publisher,
            condition,
        },
    );
    Ok(())
}

fn classify(
    name: &str,
    header: &MachineHeader,
    status: Option<&RomStatus>,
    category: Option<&str>,
) -> GameCondition {
    if header.is_bios {
        return GameCondition::Bios;
    }
    if header.is_device
        || machine_override(name).is_some()
        || category.is_some_and(|category| category_preference(category).is_some())
    {
        return GameCondition::System;
    }
    if header.is_mechanical
        || !status.is_some_and(|status| EmulationPolicy::Playable.accepts(status))
    {
        return GameCondition::NotWorking;
    }
    GameCondition::Working
}

fn machine_header(event: &quick_xml::events::BytesStart) -> Result<MachineHeader, Box<dyn Error>> {
    let name = attr(event, "name").ok_or("MAME machine is missing a name")?;
    Ok(MachineHeader {
        name,
        is_bios: flag(event, "isbios"),
        is_device: flag(event, "isdevice"),
        is_mechanical: flag(event, "ismechanical"),
    })
}

fn read_machine_body<R: BufRead>(
    xml: &mut Reader<R>,
    source: &Path,
) -> Result<MachineText, Box<dyn Error>> {
    let mut depth = 1u32;
    let mut capture: Option<String> = None;
    let mut captured = String::new();
    let mut description = None;
    let mut year = None;
    let mut publisher = None;
    let mut status = None;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(event)) => {
                depth += 1;
                if depth == 2 && is_local(&event, "driver") {
                    status = Some(driver_status(&event)?);
                } else if depth == 2
                    && (is_local(&event, "description")
                        || is_local(&event, "year")
                        || is_local(&event, "manufacturer"))
                {
                    capture = Some(element_name(&event));
                    captured.clear();
                }
            }
            Ok(Event::Empty(event)) => {
                if depth == 1 && is_local(&event, "driver") {
                    status = Some(driver_status(&event)?);
                }
            }
            Ok(Event::Text(text)) if capture.is_some() => {
                captured.push_str(&text.xml_content(XmlVersion::Implicit1_0));
            }
            Ok(Event::GeneralRef(reference)) if capture.is_some() => {
                captured.push_str(&resolve_reference(&reference)?);
            }
            Ok(Event::End(_)) => {
                if depth == 2 {
                    if let Some(field) = capture.take() {
                        let value = std::mem::take(&mut captured);
                        let value = value.trim();
                        if !value.is_empty() {
                            match field.as_str() {
                                "description" => description = Some(value.to_string()),
                                "year" => year = Some(value.to_string()),
                                "manufacturer" => publisher = Some(value.to_string()),
                                _ => {}
                            }
                        }
                    }
                }
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            Ok(Event::Eof) => {
                return Err(format!("Unclosed machine in {}", source.display()).into());
            }
            Ok(_) => {}
            Err(error) => {
                return Err(format!("Cannot parse MAME XML {}: {error}", source.display()).into());
            }
        }
    }
    let title = description.unwrap_or_else(|| "Unknown".into());
    Ok(MachineText {
        title,
        year,
        publisher,
        status,
    })
}

fn driver_status(event: &quick_xml::events::BytesStart) -> Result<RomStatus, Box<dyn Error>> {
    let driver = required_status(event, "status")?;
    let emulation = required_status(event, "emulation")?;
    Ok(RomStatus { driver, emulation })
}

fn required_status(
    event: &quick_xml::events::BytesStart,
    name: &str,
) -> Result<Status, Box<dyn Error>> {
    let text = attr(event, name).ok_or_else(|| format!("MAME driver is missing {name}"))?;
    Status::from_str(&text).map_err(|_| format!("Unknown MAME driver {name} {text:?}").into())
}

fn resolve_reference(reference: &BytesRef) -> Result<String, Box<dyn Error>> {
    if let Some(ch) = reference.resolve_char_ref()? {
        return Ok(ch.to_string());
    }
    let name: &str = reference;
    resolve_xml_entity(name)
        .map(str::to_string)
        .ok_or_else(|| format!("Unknown XML entity &{name};").into())
}

fn is_local(event: &quick_xml::events::BytesStart, name: &str) -> bool {
    event.name().local_name().into_inner() == name
}

fn element_name(event: &quick_xml::events::BytesStart) -> String {
    event.name().local_name().into_inner().to_string()
}

fn attr(event: &quick_xml::events::BytesStart, name: &str) -> Option<String> {
    event.attributes().flatten().find_map(|attr| {
        if attr.key.local_name().into_inner() != name {
            return None;
        }
        let text = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .ok()?
            .trim()
            .to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    })
}

fn flag(event: &quick_xml::events::BytesStart, name: &str) -> bool {
    attr(event, name).is_some_and(|value| value == "yes")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> ArcadeCatalog {
        let dir = tempfile::tempdir().unwrap();
        let xml = dir.path().join("mame.xml");
        let catver = dir.path().join("catver.ini");
        std::fs::write(
            &xml,
            r#"<mame build="0.289">
                <machine name="pacman">
                    <description>Pac-Man</description>
                    <year>1980</year>
                    <manufacturer>Namco</manufacturer>
                    <driver status="good" emulation="good"/>
                </machine>
                <machine name="neogeo" isbios="yes">
                    <description>Neo-Geo</description>
                    <year>1990</year>
                    <manufacturer>SNK</manufacturer>
                    <driver status="good" emulation="good"/>
                </machine>
                <machine name="puckman">
                    <description>Puckman</description>
                    <year>1980</year>
                    <manufacturer>Namco</manufacturer>
                    <driver status="preliminary" emulation="good"/>
                </machine>
                <machine name="apple2" isdevice="no">
                    <description>Apple II</description>
                    <year>1977</year>
                    <manufacturer>Apple</manufacturer>
                    <driver status="good" emulation="good"/>
                </machine>
                <machine name="galaga">
                    <description>Galaga</description>
                    <year>1981</year>
                    <manufacturer>Namco</manufacturer>
                    <driver status="imperfect" emulation="imperfect"/>
                </machine>
                <machine name="mrchalgr">
                    <description>Mr. Challenger</description>
                    <year>1985</year>
                    <manufacturer>Sega</manufacturer>
                    <driver status="good" emulation="good"/>
                </machine>
            </mame>"#,
        )
        .unwrap();
        std::fs::write(
            &catver,
            "[Category]\npacman=Maze / Collect\nneogeo=System / BIOS\npuckman=Maze / Collect\napple2=Computer / Desktop\ngalaga=Shooter / Gallery\nmrchalgr=Maze / Collect\n",
        )
        .unwrap();
        ArcadeCatalog::load(&xml, &catver).unwrap()
    }

    fn found(catalog: &ArcadeCatalog, file_name: &str) -> GameMetadata {
        match catalog.lookup_name(file_name) {
            Lookup::Found(game) => game,
            Lookup::Missing => panic!("{file_name} was not matched"),
        }
    }

    #[test]
    fn zip_stem_matches_the_list_and_skips_other_files() {
        let catalog = catalog();
        let game = found(&catalog, "Pacman.ZIP");
        assert_eq!(game.title, "Pac-Man");
        assert_eq!(game.year.as_deref(), Some("1980"));
        assert_eq!(game.publisher.as_deref(), Some("Namco"));
        assert_eq!(game.playable, Some(true));
        assert_eq!(game.condition, Some(GameCondition::Working));
        assert_eq!(game.thumbnail, None);
        assert_eq!(
            game.link.as_deref(),
            Some("https://adb.arcadeitalia.net/dettaglio_mame.php?game_name=pacman")
        );
        assert!(matches!(catalog.lookup_name("pacman.chd"), Lookup::Missing));
        assert!(matches!(catalog.lookup_name("notes.txt"), Lookup::Missing));
    }

    #[test]
    fn enrich_names_a_zip_without_reading_it_as_an_archive() {
        use crate::library::{discover_library, enrich_library, LibraryDb};

        let catalog = catalog();
        let db_dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("pacman.zip"), b"not a zip").unwrap();
        let mut db = LibraryDb::open(&db_dir.path().join("library.sqlite")).unwrap();
        let library = db.create_library("arcade", "Arcade").unwrap();
        db.add_root(library, root.path()).unwrap();
        discover_library(&mut db, library).unwrap();
        let cache = tempfile::tempdir().unwrap();
        let stats = enrich_library(&mut db, library, cache.path(), &catalog).unwrap();
        assert_eq!(stats.enriched, 1);
        let file_id = db.files_for_root(db.roots(library).unwrap()[0].0).unwrap()[0].id;
        let game = db.game_for_file(file_id).unwrap().unwrap();
        assert_eq!(game.title, "Pac-Man");
        assert_eq!(game.playable, Some(true));
        assert!(game.thumb_path.is_none());
        assert!(game.link.unwrap().contains("game_name=pacman"));
    }

    #[test]
    fn conditions_follow_the_export_rules() {
        let catalog = catalog();
        assert_eq!(
            found(&catalog, "neogeo.zip").condition,
            Some(GameCondition::Bios)
        );
        assert_eq!(
            found(&catalog, "puckman.7z").condition,
            Some(GameCondition::NotWorking)
        );
        assert_eq!(
            found(&catalog, "apple2.zip").condition,
            Some(GameCondition::System)
        );
        let galaga = found(&catalog, "galaga.zip");
        assert_eq!(galaga.condition, Some(GameCondition::Working));
        assert_eq!(galaga.playable, Some(true));
        let override_name = found(&catalog, "mrchalgr.zip");
        assert_eq!(override_name.condition, Some(GameCondition::System));
        assert_eq!(override_name.playable, Some(false));
    }

    #[test]
    fn description_keeps_xml_entities() {
        let dir = tempfile::tempdir().unwrap();
        let xml = dir.path().join("mame.xml");
        let catver = dir.path().join("catver.ini");
        std::fs::write(
            &xml,
            r#"<mame build="0.289">
                <machine name="ddungeon">
                    <description>Dungeons &amp; Dragons</description>
                    <year>1992</year>
                    <manufacturer>Atari &#38; Capcom</manufacturer>
                    <driver status="good" emulation="good"/>
                </machine>
            </mame>"#,
        )
        .unwrap();
        std::fs::write(&catver, "[Category]\nddungeon=Fighter / Versus\n").unwrap();
        let catalog = ArcadeCatalog::load(&xml, &catver).unwrap();
        let game = found(&catalog, "ddungeon.zip");
        assert_eq!(game.title, "Dungeons & Dragons");
        assert_eq!(game.publisher.as_deref(), Some("Atari & Capcom"));
    }
}
