//! Download and cache MAME listxml and CatVer files for a given MAME version.

use std::error::Error;
use std::fs::{self, File};
use std::io::{copy, Read, Seek};
use std::path::{Path, PathBuf};
use std::time::Duration;

use zip::ZipArchive;

/// Paths to cached metadata files for one MAME release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MameMetadataPaths {
    pub mame_xml: PathBuf,
    pub catver: PathBuf,
}

/// Normalized MAME release (major is always 0 today; minor is the set number, e.g. 289).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MameVersion {
    pub major: u32,
    pub minor: u32,
}

impl MameVersion {
    /// GitHub release folder name, e.g. `mame0289`.
    pub fn release_slug(&self) -> String {
        format!("mame{:04}", self.minor)
    }

    /// Human-readable version string, e.g. `0.289`.
    pub fn display(&self) -> String {
        format!("{}.{}", self.major, self.minor)
    }
}

/// Parse user input such as `0.289`, `289`, or `mame0289`.
pub fn parse_mame_version(input: &str) -> Result<MameVersion, Box<dyn Error>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("MAME version cannot be empty.".into());
    }
    if let Some(rest) = trimmed
        .strip_prefix("mame")
        .or_else(|| trimmed.strip_prefix("MAME"))
    {
        let minor: u32 = rest
            .trim_start_matches('0')
            .parse()
            .map_err(|_| invalid_version(input))?;
        if minor == 0 && !rest.chars().all(|c| c == '0') {
            return Err(invalid_version(input));
        }
        return Ok(MameVersion { major: 0, minor });
    }
    let parts: Vec<&str> = trimmed.split('.').filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        [minor] => {
            let minor: u32 = minor.parse().map_err(|_| invalid_version(input))?;
            Ok(MameVersion { major: 0, minor })
        }
        [major, minor] => {
            let major: u32 = major.parse().map_err(|_| invalid_version(input))?;
            let minor: u32 = minor.parse().map_err(|_| invalid_version(input))?;
            if major != 0 {
                return Err(
                    format!("Unsupported MAME major version {major}; expected 0.xxx.").into(),
                );
            }
            Ok(MameVersion { major, minor })
        }
        [major, minor, patch] if *patch == "0" => {
            let major: u32 = major.parse().map_err(|_| invalid_version(input))?;
            let minor: u32 = minor.parse().map_err(|_| invalid_version(input))?;
            if major != 0 {
                return Err(
                    format!("Unsupported MAME major version {major}; expected 0.xxx.").into(),
                );
            }
            Ok(MameVersion { major, minor })
        }
        _ => Err(invalid_version(input)),
    }
}

fn invalid_version(input: &str) -> Box<dyn Error> {
    format!("Invalid MAME version {input:?}. Use a form like 0.289 or 289.").into()
}

/// Default metadata cache root (`…/roms-curator/metadata` on typical Unix layouts).
///
/// `ROMS_CURATOR_CACHE` and `--metadata-cache-dir` set this entire root (the path passed to
/// [`ensure_metadata`]), not the XDG cache base alone.
pub fn default_metadata_cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ROMS_CURATOR_CACHE") {
        return PathBuf::from(dir);
    }
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            return Path::new(&local).join("roms-curator").join("metadata");
        }
    }
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return Path::new(&xdg).join("roms-curator").join("metadata");
    }
    if let Ok(home) = std::env::var("HOME") {
        return Path::new(&home)
            .join(".cache")
            .join("roms-curator")
            .join("metadata");
    }
    PathBuf::from(".roms-curator-cache")
}

/// Ensure listxml and CatVer for `version` exist under `cache_root`, downloading when missing or when `refresh` is true.
pub fn ensure_metadata(
    version: &str,
    cache_root: &Path,
    refresh: bool,
) -> Result<MameMetadataPaths, Box<dyn Error>> {
    let version = parse_mame_version(version)?;
    let cache_dir = cache_root.join(format!("mame-{}", version.display()));
    fs::create_dir_all(&cache_dir)?;

    let slug = version.release_slug();
    let mame_xml = cache_dir.join(format!("{slug}.xml"));
    let catver = cache_dir.join("catver.ini");

    let agent = build_http_agent();

    if refresh || !mame_xml.is_file() {
        download_mame_listxml(&version, &mame_xml, &agent)?;
    }
    if refresh || !catver.is_file() {
        download_catver(&version, &catver, &agent)?;
    }

    mame_xml_matches_version(&mame_xml, &version)?;
    validate_cached_catver(&catver, &version)?;

    if !mame_xml.is_file() {
        return Err(format!("MAME XML not found after download: {}", mame_xml.display()).into());
    }
    if !catver.is_file() {
        return Err(format!("CatVer not found after download: {}", catver.display()).into());
    }

    Ok(MameMetadataPaths { mame_xml, catver })
}

fn mame_listxml_zip_url(version: &MameVersion) -> String {
    let slug = version.release_slug();
    format!("https://github.com/mamedev/mame/releases/download/{slug}/{slug}lx.zip")
}

fn catver_zip_url(version: &MameVersion) -> String {
    format!(
        "https://www.progettosnaps.net/download/?tipo=catver&file=pS_CatVer_{}.zip",
        version.minor
    )
}

fn download_mame_listxml(
    version: &MameVersion,
    destination: &Path,
    agent: &ureq::Agent,
) -> Result<(), Box<dyn Error>> {
    let url = mame_listxml_zip_url(version);
    let zip = download_url_to_temp(&url, agent)?;
    let slug = version.release_slug();
    let member_name = format!("{slug}.xml");
    extract_zip_member_from_path(zip.path(), &member_name, destination)?;
    mame_xml_matches_version(destination, version)?;
    Ok(())
}

fn download_catver(
    version: &MameVersion,
    destination: &Path,
    agent: &ureq::Agent,
) -> Result<(), Box<dyn Error>> {
    let url = catver_zip_url(version);
    let zip = download_url_to_temp(&url, agent)?;
    let staging = destination.with_extension("part");
    write_zip_member_from_path(zip.path(), "catver.ini", &staging)?;
    let contents = fs::read_to_string(&staging)?;
    if !catver_header_matches(&contents, version) {
        let _ = fs::remove_file(&staging);
        return Err(format!(
            "Downloaded CatVer does not match MAME {}.",
            version.display()
        )
        .into());
    }
    replace_file(&staging, destination)?;
    Ok(())
}

fn validate_cached_catver(path: &Path, version: &MameVersion) -> Result<(), Box<dyn Error>> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Cannot read cached CatVer {}: {error}", path.display()))?;
    if catver_header_matches(&contents, version) {
        Ok(())
    } else {
        Err(format!(
            "Cached CatVer at {} does not match MAME {}. Delete the file or use --metadata-refresh=true.",
            path.display(),
            version.display()
        )
        .into())
    }
}

fn mame_xml_matches_version(path: &Path, version: &MameVersion) -> Result<(), Box<dyn Error>> {
    let prefix = read_file_prefix(path, 65_536)?;
    let parsed = mame_build_from_prefix(&prefix)
        .map_err(|error| format!("Cannot read MAME build from {}: {error}", path.display()))?;
    if parsed.major == version.major && parsed.minor == version.minor {
        Ok(())
    } else {
        Err(format!(
            "MAME XML build {} does not match requested {} ({}). Delete the cached file or use --metadata-refresh=true.",
            parsed.display(),
            version.display(),
            path.display()
        )
        .into())
    }
}

fn read_file_prefix(path: &Path, max_bytes: usize) -> Result<String, Box<dyn Error>> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0_u8; max_bytes];
    let read = file.read(&mut buffer)?;
    buffer.truncate(read);
    Ok(String::from_utf8(buffer)?)
}

fn mame_build_from_prefix(prefix: &str) -> Result<MameVersion, Box<dyn Error>> {
    let marker = "build=\"";
    let start = prefix.find(marker).ok_or("missing mame build attribute")?;
    let value = &prefix[start + marker.len()..];
    let end = value.find('"').ok_or("unterminated mame build attribute")?;
    let build = &value[..end];
    let token = build.split_whitespace().next().unwrap_or(build);
    parse_mame_version(token).or_else(|_| parse_mame_version(build))
}

fn replace_file(staging: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    if destination.exists() {
        fs::remove_file(destination)?;
    }
    fs::rename(staging, destination)?;
    Ok(())
}

fn catver_header_version(line: &str) -> Option<MameVersion> {
    let line = line.trim();
    if !line.starts_with(';') {
        return None;
    }
    let marker = "catver.ini";
    let idx = line.to_ascii_lowercase().find(marker)?;
    let after_marker = line[idx + marker.len()..].trim();
    let token = after_marker
        .split(['/', ' '])
        .find(|part| !part.is_empty())?;
    parse_mame_version(token).ok()
}

fn catver_header_matches(contents: &str, version: &MameVersion) -> bool {
    contents.lines().take(20).any(|line| {
        catver_header_version(line)
            .is_some_and(|header| header.major == version.major && header.minor == version.minor)
    })
}

fn build_http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_read(Duration::from_secs(600))
        .timeout_connect(Duration::from_secs(60))
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
}

fn download_url_to_temp(
    url: &str,
    agent: &ureq::Agent,
) -> Result<tempfile::NamedTempFile, Box<dyn Error>> {
    let response = agent.get(url).call().map_err(|error| {
        format!(
            "HTTP GET {url} failed: {error}. Check the MAME version and your network connection."
        )
    })?;
    let status = response.status();
    if status != 200 {
        return Err(format!("HTTP GET {url} returned status {status}.").into());
    }
    let mut temp = tempfile::NamedTempFile::new()?;
    copy(&mut response.into_reader(), temp.as_file_mut())?;
    temp.as_file().sync_all()?;
    Ok(temp)
}

fn extract_zip_member_from_path(
    zip_path: &Path,
    member_name: &str,
    destination: &Path,
) -> Result<(), Box<dyn Error>> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let staging = destination.with_extension("part");
    write_zip_member_from_archive(&mut archive, member_name, &staging)?;
    replace_file(&staging, destination)
}

fn write_zip_member_from_path(
    zip_path: &Path,
    member_name: &str,
    output: &Path,
) -> Result<(), Box<dyn Error>> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    write_zip_member_from_archive(&mut archive, member_name, output)
}

fn write_zip_member_from_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    member_name: &str,
    output: &Path,
) -> Result<(), Box<dyn Error>> {
    let mut found = false;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let name = file.name()?.replace('\\', "/");
        if Path::new(&name).file_name().and_then(|n| n.to_str()) != Some(member_name) {
            continue;
        }
        found = true;
        let parent = output
            .parent()
            .ok_or_else(|| format!("No parent directory for {}", output.display()))?;
        fs::create_dir_all(parent)?;
        {
            let mut out = File::create(output)?;
            copy(&mut file, &mut out)?;
            out.sync_all()?;
        }
        break;
    }
    if !found {
        return Err(format!("Archive from download did not contain {member_name}.").into());
    }
    Ok(())
}

#[cfg(test)]
fn extract_zip_member(
    zip_bytes: &[u8],
    member_name: &str,
    destination: &Path,
) -> Result<(), Box<dyn Error>> {
    let reader = std::io::Cursor::new(zip_bytes);
    let mut archive = ZipArchive::new(reader)?;
    let staging = destination.with_extension("part");
    write_zip_member_from_archive(&mut archive, member_name, &staging)?;
    replace_file(&staging, destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_accepts_common_forms() {
        assert_eq!(
            parse_mame_version("0.289").unwrap(),
            MameVersion {
                major: 0,
                minor: 289
            }
        );
        assert_eq!(
            parse_mame_version("289").unwrap(),
            MameVersion {
                major: 0,
                minor: 289
            }
        );
        assert_eq!(
            parse_mame_version("mame0289").unwrap(),
            MameVersion {
                major: 0,
                minor: 289
            }
        );
        assert_eq!(
            parse_mame_version("0.244.0").unwrap(),
            MameVersion {
                major: 0,
                minor: 244
            }
        );
    }

    #[test]
    fn release_slug_zero_pads_minor() {
        let v = MameVersion {
            major: 0,
            minor: 44,
        };
        assert_eq!(v.release_slug(), "mame0044");
    }

    #[test]
    fn build_download_urls() {
        let v = MameVersion {
            major: 0,
            minor: 289,
        };
        assert_eq!(
            mame_listxml_zip_url(&v),
            "https://github.com/mamedev/mame/releases/download/mame0289/mame0289lx.zip"
        );
        assert_eq!(
            catver_zip_url(&v),
            "https://www.progettosnaps.net/download/?tipo=catver&file=pS_CatVer_289.zip"
        );
    }

    #[test]
    fn catver_header_rejects_shorter_version_prefix() {
        let header = ";; catver.ini 0.244 / 31-May-22 / MAME 0.244 ;;";
        let requested = MameVersion {
            major: 0,
            minor: 24,
        };
        assert!(!catver_header_matches(header, &requested));
        assert!(catver_header_matches(
            header,
            &MameVersion {
                major: 0,
                minor: 244
            }
        ));
    }

    #[test]
    fn mame_build_from_listxml_prefix() {
        let prefix = r#"<?xml version="1.0"?><mame build="0.244 (mame0244)""#;
        let parsed = mame_build_from_prefix(prefix).unwrap();
        assert_eq!(
            parsed,
            MameVersion {
                major: 0,
                minor: 244
            }
        );
    }

    #[test]
    fn extract_zip_member_finds_file_by_basename() {
        let sample = fs::read("tests/resources/catver_sample.ini").unwrap();
        let zip_bytes = test_zip_with_member("catver.ini", &sample);
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("extracted.ini");
        extract_zip_member(&zip_bytes, "catver.ini", &out).unwrap();
        assert_eq!(
            fs::read_to_string(&out).unwrap(),
            String::from_utf8(sample).unwrap()
        );
    }

    #[test]
    fn extract_zip_member_finds_nested_path_by_basename() {
        let sample = b"[Category]\nnested=Shooter\n";
        let zip_bytes = test_zip_with_member("pack/catver.ini", sample);
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("catver.ini");
        extract_zip_member(&zip_bytes, "catver.ini", &out).unwrap();
        assert!(fs::read_to_string(&out).unwrap().contains("nested=Shooter"));
    }

    fn test_zip_with_member(path_in_archive: &str, contents: &[u8]) -> Vec<u8> {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::CompressionMethod;

        let mut zip_bytes = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            writer.start_file(path_in_archive, options).unwrap();
            writer.write_all(contents).unwrap();
            writer.finish().unwrap();
        }
        zip_bytes
    }
}
