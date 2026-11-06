//! Streaming MAME listxml parser (PERF-02). Reads one `<machine>` at a time without a full DOM.

use std::error::Error;
use std::io::BufRead;
use std::path::Path;
use std::str::FromStr;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use quick_xml::XmlVersion;

use crate::models::roms::{
    Chd, ChdStatus, Feature, FeatureStatus, RomData, RomFile, RomStatus, UnfilteredRoms,
    UNKNOWN_CATEGORY,
};
use crate::models::selection::{category_preference, machine_override};
use crate::RomCategories;

/// Parsed listxml payload before categorization.
pub struct MameListXml {
    pub mame_build: String,
    pub machines: UnfilteredRoms,
    pub unknown_categories: Vec<String>,
    pub source: std::path::PathBuf,
}

pub fn parse<R: BufRead>(
    source: &Path,
    reader: R,
    categories: &RomCategories,
) -> Result<MameListXml, Box<dyn Error>> {
    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut mame_build: Option<String> = None;
    let mut machines = UnfilteredRoms::new();
    let mut unknown_categories = Vec::new();

    loop {
        buf.clear();
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if is_tag(&e, "mame") => {
                mame_build = Some(required_attr_on(&e, &mut xml, source, "mame", "build")?);
            }
            Ok(Event::Start(e)) if is_tag(&e, "machine") => {
                let machine =
                    parse_machine(&mut xml, &e, source, categories, &mut unknown_categories)?;
                if machines.contains_key(&machine.0) {
                    return Err(format!(
                        "Machine '{}' in {}: duplicate machine name",
                        machine.0,
                        source.display()
                    )
                    .into());
                }
                machines.insert(machine.0, machine.1);
            }
            Ok(Event::Empty(e)) if is_tag(&e, "machine") => {
                let machine =
                    parse_machine_leaf(&e, &mut xml, source, categories, &mut unknown_categories)?;
                if machines.contains_key(&machine.0) {
                    return Err(format!(
                        "Machine '{}' in {}: duplicate machine name",
                        machine.0,
                        source.display()
                    )
                    .into());
                }
                machines.insert(machine.0, machine.1);
            }
            Ok(Event::Eof) => break,
            Ok(Event::DocType(_)) | Ok(Event::Decl(_)) | Ok(Event::PI(_)) => {}
            Ok(_) => {}
            Err(error) => {
                return Err(format!("Cannot parse MAME XML {}: {error}", source.display()).into());
            }
        }
    }

    let mame_build = mame_build.ok_or_else(|| {
        format!(
            "{}: root element must be 'mame' with a build attribute",
            source.display()
        )
    })?;

    unknown_categories.sort();
    Ok(MameListXml {
        mame_build,
        machines,
        unknown_categories,
        source: source.to_path_buf(),
    })
}

fn parse_machine<R: BufRead>(
    reader: &mut Reader<R>,
    start: &BytesStart,
    source: &Path,
    categories: &RomCategories,
    unknown_categories: &mut Vec<String>,
) -> Result<(String, RomData), Box<dyn Error>> {
    let (name, mut data) =
        parse_machine_header(start, reader, source, categories, unknown_categories)?;

    let mut buf = Vec::new();
    loop {
        buf.clear();
        let event = reader.read_event_into(&mut buf);
        match event {
            Ok(Event::Start(e)) => match e.name().local_name().into_inner() {
                "driver" => data.status = Some(parse_driver(&e, reader, source, &name)?),
                "feature" => data
                    .features
                    .push(parse_feature(&e, reader, source, &name)?),
                "device_ref" => data.devices.push(required_machine_attr(
                    &e,
                    reader,
                    source,
                    &name,
                    "device_ref",
                    "name",
                )?),
                "rom" => data.roms.push(parse_rom(&e, reader, source, &name)?),
                "disk" => data.chd.push(parse_disk(&e, reader, source, &name)?),
                _ => skip_subtree(reader)?,
            },
            Ok(Event::Empty(e)) => match e.name().local_name().into_inner() {
                "driver" => data.status = Some(parse_driver(&e, reader, source, &name)?),
                "feature" => data
                    .features
                    .push(parse_feature(&e, reader, source, &name)?),
                "device_ref" => data.devices.push(required_machine_attr(
                    &e,
                    reader,
                    source,
                    &name,
                    "device_ref",
                    "name",
                )?),
                "rom" => data.roms.push(parse_rom(&e, reader, source, &name)?),
                "disk" => data.chd.push(parse_disk(&e, reader, source, &name)?),
                _ => {}
            },
            Ok(Event::End(e)) if e.name().local_name().into_inner() == "machine" => break,
            Ok(Event::Eof) => {
                return Err(format!(
                    "Machine '{name}' in {}: unclosed machine element",
                    source.display()
                )
                .into());
            }
            Ok(_) => {}
            Err(error) => {
                return Err(format!("Cannot parse MAME XML {}: {error}", source.display()).into());
            }
        }
    }

    Ok((name, data))
}

fn parse_machine_leaf<R: BufRead>(
    start: &BytesStart,
    reader: &mut Reader<R>,
    source: &Path,
    categories: &RomCategories,
    unknown_categories: &mut Vec<String>,
) -> Result<(String, RomData), Box<dyn Error>> {
    parse_machine_header(start, reader, source, categories, unknown_categories)
}

fn parse_machine_header<R: BufRead>(
    start: &BytesStart,
    reader: &mut Reader<R>,
    source: &Path,
    categories: &RomCategories,
    unknown_categories: &mut Vec<String>,
) -> Result<(String, RomData), Box<dyn Error>> {
    let name = required_attr_on(start, reader, source, "machine", "name")?;
    let category = match categories.get(name.as_str()) {
        Some(category) => category.clone(),
        None => {
            unknown_categories.push(name.clone());
            UNKNOWN_CATEGORY.to_string()
        }
    };

    let is_bios = flag_attr(start, "isbios");
    let is_device = flag_attr(start, "isdevice");
    let is_mechanical = optional_attr(start, reader, "ismechanical")?.as_deref() == Some("yes");
    let clone_of = optional_attr(start, reader, "cloneof")?;
    let rom_of = optional_attr(start, reader, "romof")?;

    let is_system = is_device || machine_override(name.as_str()).is_some() || {
        categories
            .get(name.as_str())
            .is_some_and(|category| category_preference(category).is_some())
    };

    Ok((
        name,
        RomData {
            status: None,
            is_bios,
            is_system,
            is_mechanical,
            features: Vec::new(),
            clone_of,
            rom_of,
            devices: Vec::new(),
            roms: Vec::new(),
            chd: Vec::new(),
            category,
        },
    ))
}

/// Skip an element subtree after its opening `Start` has already been consumed.
///
/// With default `quick-xml` settings, self-closing tags emit a single `Event::Empty`
/// and do not add nesting depth; only `Start`/`End` pairs adjust `depth`. Do not
/// decrement on `Empty` at the skipped element's depth or the reader exits before
/// its closing `End` (see `<input><control/></input>` in `listxml_sample.xml`).
fn skip_subtree<R: BufRead>(reader: &mut Reader<R>) -> Result<(), Box<dyn Error>> {
    let mut depth = 1usize;
    let mut buf = Vec::new();
    while depth > 0 {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(_)) => depth -= 1,
            Ok(Event::Eof) => return Err("unexpected end of XML while skipping element".into()),
            Ok(_) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn parse_driver<R: BufRead>(
    element: &BytesStart,
    reader: &mut Reader<R>,
    source: &Path,
    machine: &str,
) -> Result<RomStatus, Box<dyn Error>> {
    let driver = required_machine_attr(element, reader, source, machine, "driver", "status")?;
    let emulation = required_machine_attr(element, reader, source, machine, "driver", "emulation")?;
    Ok(RomStatus {
        driver: parse_enum(source, machine, "driver status", &driver)?,
        emulation: parse_enum(source, machine, "driver emulation", &emulation)?,
    })
}

fn parse_feature<R: BufRead>(
    element: &BytesStart,
    reader: &mut Reader<R>,
    source: &Path,
    machine: &str,
) -> Result<Feature, Box<dyn Error>> {
    let typ = required_machine_attr(element, reader, source, machine, "feature", "type")?;
    let own = optional_attr(element, reader, "status")?;
    let overall = optional_attr(element, reader, "overall")?;
    let status = match (own.as_deref(), overall.as_deref()) {
        (None, None) => {
            return Err(machine_error(
                source,
                machine,
                "feature is missing required attribute 'status'".to_string(),
            ));
        }
        (Some(value), None) => parse_enum(source, machine, "feature status", value)?,
        (None, Some(value)) => parse_enum(source, machine, "feature overall", value)?,
        (Some(own), Some(overall)) => {
            let own = parse_enum(source, machine, "feature status", own)?;
            let overall = parse_enum(source, machine, "feature overall", overall)?;
            worse_feature_status(own, overall)
        }
    };
    Ok(Feature { typ, status })
}

fn parse_disk<R: BufRead>(
    element: &BytesStart,
    reader: &mut Reader<R>,
    source: &Path,
    machine: &str,
) -> Result<Chd, Box<dyn Error>> {
    let name = required_machine_attr(element, reader, source, machine, "disk", "name")?;
    Ok(Chd {
        name: name.clone(),
        status: dump_status(element, reader, source, machine, "disk status")?,
        optional: optional_flag(element, reader, source, machine, "disk", &name)?,
        merge: optional_attr(element, reader, "merge")?,
        sha1: optional_attr(element, reader, "sha1")?,
    })
}

fn parse_rom<R: BufRead>(
    element: &BytesStart,
    reader: &mut Reader<R>,
    source: &Path,
    machine: &str,
) -> Result<RomFile, Box<dyn Error>> {
    let name = required_machine_attr(element, reader, source, machine, "rom", "name")?;
    let size = match optional_attr(element, reader, "size")? {
        None => None,
        Some(value) => Some(value.parse::<u64>().map_err(|_| {
            machine_error(
                source,
                machine,
                format!("rom '{name}' has unknown size '{value}'"),
            )
        })?),
    };
    let optional = optional_flag(element, reader, source, machine, "rom", &name)?;
    Ok(RomFile {
        name,
        size,
        crc: optional_attr(element, reader, "crc")?,
        sha1: optional_attr(element, reader, "sha1")?,
        merge: optional_attr(element, reader, "merge")?,
        bios: optional_attr(element, reader, "bios")?,
        status: dump_status(element, reader, source, machine, "rom status")?,
        optional,
    })
}

fn is_tag(element: &BytesStart, name: &str) -> bool {
    element.name().local_name().into_inner() == name
}

fn required_attr_on<R: BufRead>(
    element: &BytesStart,
    reader: &mut Reader<R>,
    source: &Path,
    element_name: &str,
    attribute: &str,
) -> Result<String, Box<dyn Error>> {
    match optional_attr(element, reader, attribute)? {
        Some(value) => Ok(value),
        None => Err(format!(
            "{}: {element_name} is missing required attribute '{attribute}'",
            source.display()
        )
        .into()),
    }
}

fn required_machine_attr<R: BufRead>(
    element: &BytesStart,
    reader: &mut Reader<R>,
    source: &Path,
    machine: &str,
    element_name: &str,
    attribute: &str,
) -> Result<String, Box<dyn Error>> {
    match optional_attr(element, reader, attribute)? {
        Some(value) => Ok(value),
        None => Err(machine_error(
            source,
            machine,
            format!("{element_name} is missing required attribute '{attribute}'"),
        )),
    }
}

fn optional_attr<R: BufRead>(
    element: &BytesStart,
    _reader: &mut Reader<R>,
    name: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    for attr in element.attributes() {
        let attr = attr.map_err(|error| error.to_string())?;
        if attr.key.local_name().into_inner() != name {
            continue;
        }
        let decoded = attr.normalized_value(XmlVersion::Implicit1_0)?;
        let text = decoded.trim();
        if text.is_empty() {
            return Ok(None);
        }
        return Ok(Some(text.to_string()));
    }
    Ok(None)
}

fn flag_attr(element: &BytesStart, name: &str) -> bool {
    element
        .attributes()
        .flatten()
        .any(|attr| attr.key.local_name().into_inner() == name && attr.value.as_ref() == "yes")
}

fn dump_status<R: BufRead>(
    element: &BytesStart,
    reader: &mut Reader<R>,
    source: &Path,
    machine: &str,
    attribute: &str,
) -> Result<ChdStatus, Box<dyn Error>> {
    match optional_attr(element, reader, "status")? {
        None => Ok(ChdStatus::Good),
        Some(value) => parse_enum(source, machine, attribute, &value),
    }
}

fn optional_flag<R: BufRead>(
    element: &BytesStart,
    reader: &mut Reader<R>,
    source: &Path,
    machine: &str,
    element_name: &str,
    name: &str,
) -> Result<bool, Box<dyn Error>> {
    match optional_attr(element, reader, "optional")? {
        None => Ok(false),
        Some(value) if value.eq_ignore_ascii_case("yes") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("no") => Ok(false),
        Some(value) => Err(machine_error(
            source,
            machine,
            format!("{element_name} '{name}' has unknown optional value '{value}'"),
        )),
    }
}

fn machine_error(source: &Path, machine: &str, detail: String) -> Box<dyn Error> {
    format!("Machine '{machine}' in {}: {detail}", source.display()).into()
}

fn parse_enum<T: FromStr>(
    source: &Path,
    machine: &str,
    attribute: &str,
    value: &str,
) -> Result<T, Box<dyn Error>> {
    T::from_str(value)
        .map_err(|_| machine_error(source, machine, format!("unknown {attribute} '{value}'")))
}

fn worse_feature_status(own: FeatureStatus, overall: FeatureStatus) -> FeatureStatus {
    match (own, overall) {
        (FeatureStatus::Unemulated, _) | (_, FeatureStatus::Unemulated) => {
            FeatureStatus::Unemulated
        }
        _ => FeatureStatus::Imperfect,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Cursor;

    use super::*;

    #[test]
    fn skip_subtree_preserves_following_machines() {
        let xml = r#"<mame build="fixture">
            <machine name="with_input">
                <input players="1"><control type="joy" buttons="1"/></input>
                <driver status="good" emulation="good"/>
            </machine>
            <machine name="next"><driver status="good" emulation="good"/></machine>
        </mame>"#;
        let categories: HashMap<String, String> = HashMap::new();
        let parsed = parse(
            Path::new("fixture.xml"),
            Cursor::new(xml.as_bytes()),
            &categories,
        )
        .unwrap();
        assert!(parsed.machines.contains_key("with_input"));
        assert!(parsed.machines.contains_key("next"));
    }
}
