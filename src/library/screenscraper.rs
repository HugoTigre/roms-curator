//! [ScreenScraper](https://www.screenscraper.fr/) lookup for SNES.
//!
//! Credentials come from the environment until the Library screen stores them:
//! `ROMS_CURATOR_SCREENSCRAPER_DEV_ID`, `ROMS_CURATOR_SCREENSCRAPER_DEV_PASSWORD`,
//! `ROMS_CURATOR_SCREENSCRAPER_USER`, `ROMS_CURATOR_SCREENSCRAPER_PASSWORD`.
//!
//! A normal developer account must send a checksum and `romtaille`. A zip is
//! identified by the inner ROM's CRC32 and uncompressed size. A loose ROM sends
//! its on-disk SHA1 and size. Requests wait on the member's `maxrequestspermin`
//! (or `maxrequestsperdmin`) after the first response that includes it.

use std::error::Error;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::enrich::{GameMetadata, Lookup, MetadataProvider, RomChecksum, RomQuery};

const JEU_INFOS: &str = "https://www.screenscraper.fr/api2/jeuInfos.php";
const SNES_SYSTEM_ID: u32 = 4;
/// Pace used until a response reports the member's per-minute allowance.
const FALLBACK_INTERVAL: Duration = Duration::from_secs(12);

/// Developer and member credentials. Both are required by ScreenScraper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScreenScraperCreds {
    pub dev_id: String,
    pub dev_password: String,
    pub user: String,
    pub password: String,
}

impl ScreenScraperCreds {
    /// Read the four `ROMS_CURATOR_SCREENSCRAPER_*` variables.
    pub fn from_env() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            dev_id: required_env("ROMS_CURATOR_SCREENSCRAPER_DEV_ID")?,
            dev_password: required_env("ROMS_CURATOR_SCREENSCRAPER_DEV_PASSWORD")?,
            user: required_env("ROMS_CURATOR_SCREENSCRAPER_USER")?,
            password: required_env("ROMS_CURATOR_SCREENSCRAPER_PASSWORD")?,
        })
    }

    /// Credentials saved by the Library screen, if the file exists.
    pub fn load_stored() -> Result<Option<Self>, Box<dyn Error>> {
        Self::load_stored_at(&super::paths::library_credentials_path()?)
    }

    pub fn load_stored_at(path: &Path) -> Result<Option<Self>, Box<dyn Error>> {
        if !path.is_file() {
            return Ok(None);
        }
        let text = fs::read_to_string(path).map_err(|error| {
            format!(
                "Cannot read ScreenScraper credentials {}: {error}",
                path.display()
            )
        })?;
        let creds = serde_json::from_str(&text).map_err(|error| {
            format!(
                "ScreenScraper credentials {} are not valid: {error}",
                path.display()
            )
        })?;
        Ok(Some(creds))
    }

    /// Write credentials for later enrichment. The file is only readable by this user.
    pub fn save_stored(&self) -> Result<(), Box<dyn Error>> {
        self.save_stored_at(&super::paths::library_credentials_path()?)
    }

    pub fn save_stored_at(&self, path: &Path) -> Result<(), Box<dyn Error>> {
        for (label, value) in [
            ("developer id", self.dev_id.as_str()),
            ("developer password", self.dev_password.as_str()),
            ("user", self.user.as_str()),
            ("password", self.password.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("ScreenScraper {label} is empty").into());
            }
        }
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_vec_pretty(self)?;
        write_private(path, &json)?;
        Ok(())
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn required_env(name: &str) -> Result<String, Box<dyn Error>> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(format!("Set {name} to use ScreenScraper").into()),
    }
}

struct Pace {
    interval: Duration,
    ready_at: Option<Instant>,
}

/// ScreenScraper client. Requests run one at a time, spaced by the member limit.
pub struct ScreenScraper {
    creds: ScreenScraperCreds,
    agent: ureq::Agent,
    pace: Mutex<Pace>,
}

impl ScreenScraper {
    pub fn new(creds: ScreenScraperCreds) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(30))
            .timeout_read(Duration::from_secs(60))
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .build();
        Self {
            creds,
            agent,
            pace: Mutex::new(Pace {
                interval: FALLBACK_INTERVAL,
                ready_at: None,
            }),
        }
    }

    fn wait_turn(&self) {
        let pace = self.pace.lock().expect("pace");
        if let Some(ready_at) = pace.ready_at {
            let now = Instant::now();
            if ready_at > now {
                std::thread::sleep(ready_at - now);
            }
        }
    }

    fn finish_request(&self) {
        let mut pace = self.pace.lock().expect("pace");
        pace.ready_at = Some(Instant::now() + pace.interval);
    }

    fn note_rate(&self, json: &Value) {
        let Some(per_minute) = requests_per_minute(json) else {
            return;
        };
        let mut pace = self.pace.lock().expect("pace");
        pace.interval = Duration::from_secs(60) / per_minute;
    }

    fn jeu_infos(&self, system_id: u32, rom: &RomQuery) -> Result<Option<Value>, Box<dyn Error>> {
        self.wait_turn();
        let parsed = self.fetch_jeu(system_id, rom);
        self.finish_request();
        parsed
    }

    fn fetch_jeu(&self, system_id: u32, rom: &RomQuery) -> Result<Option<Value>, Box<dyn Error>> {
        let request = self
            .agent
            .get(JEU_INFOS)
            .query("devid", &self.creds.dev_id)
            .query("devpassword", &self.creds.dev_password)
            .query("softname", "roms-curator")
            .query("output", "json")
            .query("ssid", &self.creds.user)
            .query("sspassword", &self.creds.password)
            .query("systemeid", &system_id.to_string())
            .query("romtype", "rom")
            .query("romnom", rom.name)
            .query("romtaille", &rom.size.to_string());
        let request = match rom.checksum {
            RomChecksum::Sha1(sha1) => request.query("sha1", sha1),
            RomChecksum::Crc32(crc) => request.query("crc", crc),
        };
        let response = request.call();
        let response = match response {
            Ok(response) => response,
            Err(ureq::Error::Status(status, _response)) => {
                return match classify_jeu_http(status) {
                    JeuHttp::Missing => Ok(None),
                    JeuHttp::Ok | JeuHttp::Rejected(_) => Err(rejected_request(status).into()),
                };
            }
            Err(ureq::Error::Transport(transport)) => {
                return Err(transport_failure("request", &transport).into());
            }
        };
        match classify_jeu_http(response.status()) {
            JeuHttp::Missing => return Ok(None),
            JeuHttp::Rejected(status) => return Err(rejected_request(status).into()),
            JeuHttp::Ok => {}
        }
        let text = response
            .into_string()
            .map_err(|_| "ScreenScraper returned a body that is not text".to_string())?;
        let json: Value =
            serde_json::from_str(&text).map_err(|error| format!("ScreenScraper JSON: {error}"))?;
        self.note_rate(&json);
        Ok(Some(json))
    }
}

fn rejected_request(status: u16) -> String {
    format!("ScreenScraper rejected the request (HTTP {status})")
}

/// `kind` only. The Display of a ureq error includes the request URL, which
/// carries the developer and member passwords.
fn transport_failure(what: &str, transport: &ureq::Transport) -> String {
    format!("ScreenScraper {what} failed ({})", transport.kind())
}

/// How `jeuInfos` treats an HTTP status. ureq 2 returns non-2xx as `Error::Status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JeuHttp {
    Ok,
    /// ScreenScraper uses 404 when the ROM is not in the database.
    Missing,
    Rejected(u16),
}

fn classify_jeu_http(status: u16) -> JeuHttp {
    match status {
        200 => JeuHttp::Ok,
        404 => JeuHttp::Missing,
        other => JeuHttp::Rejected(other),
    }
}

/// A missing cover still saves the game. Quota, auth, and server errors leave
/// the file queued so the thumbnail can be fetched later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoverHttp {
    Image,
    Missing,
    Retry(u16),
}

fn classify_cover_http(status: u16) -> CoverHttp {
    match status {
        200 => CoverHttp::Image,
        401 | 403 | 423 | 426 | 429 | 430 | 431 => CoverHttp::Retry(status),
        500..=599 => CoverHttp::Retry(status),
        _ => CoverHttp::Missing,
    }
}

impl MetadataProvider for ScreenScraper {
    fn supports(&self, platform: &str) -> bool {
        platform == "snes"
    }

    fn lookup(&self, platform: &str, rom: &RomQuery) -> Result<Lookup, Box<dyn Error>> {
        if platform != "snes" {
            return Err(
                format!("ScreenScraper enrichment is not implemented for {platform}").into(),
            );
        }
        let Some(json) = self.jeu_infos(SNES_SYSTEM_ID, rom)? else {
            return Ok(Lookup::Missing);
        };
        let mut lookup = parse_jeu_infos(&json)?;
        if let Lookup::Found(metadata) = &mut lookup {
            if let Some(url_bytes) = metadata.thumbnail.take() {
                let url = String::from_utf8(url_bytes)
                    .map_err(|_| "ScreenScraper cover URL is not UTF-8".to_string())?;
                metadata.thumbnail = self.download_thumbnail(&url)?;
            }
        }
        Ok(lookup)
    }
}

/// Parse a `jeuInfos.php` JSON document.
///
/// `GameMetadata.thumbnail` holds the cover URL until the client downloads it.
pub(crate) fn parse_jeu_infos(json: &Value) -> Result<Lookup, Box<dyn Error>> {
    let response = json.get("response").unwrap_or(json);
    if let Some(error) = response.get("erreur").and_then(Value::as_str) {
        let folded = error.to_ascii_lowercase();
        if folded.contains("introuvable") || folded.contains("not found") {
            return Ok(Lookup::Missing);
        }
        return Err(format!("ScreenScraper: {error}").into());
    }
    let Some(jeu) = response.get("jeu") else {
        return Ok(Lookup::Missing);
    };
    if jeu.is_null() {
        return Ok(Lookup::Missing);
    }
    let Some(title) = preferred_text(jeu.get("noms"), &["wor", "us", "eu", "ss", "jp"])
        .or_else(|| jeu.get("nom").and_then(Value::as_str).map(str::to_string))
    else {
        return Ok(Lookup::Missing);
    };
    let year = preferred_text(jeu.get("dates"), &["us", "wor", "eu", "ss", "jp"])
        .and_then(|text| year_from(&text));
    let publisher = jeu
        .get("editeur")
        .and_then(|value| {
            value
                .get("text")
                .and_then(Value::as_str)
                .or_else(|| value.as_str())
        })
        .map(str::to_string);
    let provider_id = jeu
        .get("id")
        .map(|id| match id {
            Value::String(text) => text.clone(),
            Value::Number(number) => number.to_string(),
            _ => String::new(),
        })
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| "unknown".into());
    let thumbnail = cover_url(jeu.get("medias"));
    Ok(Lookup::Found(GameMetadata {
        provider: "screenscraper".into(),
        provider_id,
        title,
        year,
        publisher,
        thumbnail: thumbnail.map(|url| url.into_bytes()),
    }))
}

/// Member allowance from `ssuser`. The live field is `maxrequestspermin`; the
/// written API doc still says `maxrequestsperdmin`.
fn requests_per_minute(json: &Value) -> Option<u32> {
    let response = json.get("response").unwrap_or(json);
    let user = response.get("ssuser").or_else(|| json.get("ssuser"))?;
    for key in ["maxrequestspermin", "maxrequestsperdmin"] {
        let Some(value) = user.get(key) else {
            continue;
        };
        let parsed = match value {
            Value::String(text) => text.parse::<u32>().ok(),
            Value::Number(number) => number
                .as_u64()
                .and_then(|number| u32::try_from(number).ok()),
            _ => None,
        };
        if parsed.is_some_and(|limit| limit > 0) {
            return parsed;
        }
    }
    None
}

fn year_from(text: &str) -> Option<String> {
    let digits: String = text
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .take(4)
        .collect();
    if digits.len() == 4 {
        Some(digits)
    } else {
        None
    }
}

fn preferred_text(value: Option<&Value>, regions: &[&str]) -> Option<String> {
    let entries = value?.as_array()?;
    for region in regions {
        if let Some(text) = entries.iter().find_map(|entry| {
            let entry_region = entry.get("region").and_then(Value::as_str)?;
            if !entry_region.eq_ignore_ascii_case(region) {
                return None;
            }
            entry
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_string)
        }) {
            return Some(text);
        }
    }
    entries.iter().find_map(|entry| {
        entry
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn cover_url(medias: Option<&Value>) -> Option<String> {
    let entries = medias?.as_array()?;
    let preferred = ["box-2D", "box-2D-us", "box-texture", "sstitle", "ss"];
    for kind in preferred {
        if let Some(url) = entries.iter().find_map(|entry| {
            let media_type = entry.get("type").and_then(Value::as_str)?;
            if !media_type.eq_ignore_ascii_case(kind) {
                return None;
            }
            entry.get("url").and_then(Value::as_str).map(str::to_string)
        }) {
            return Some(url);
        }
    }
    None
}

impl ScreenScraper {
    fn download_thumbnail(&self, url: &str) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        self.wait_turn();
        let downloaded = self.fetch_thumbnail(url);
        self.finish_request();
        downloaded
    }

    fn fetch_thumbnail(&self, url: &str) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        let response = self
            .agent
            .get(url)
            .query("devid", &self.creds.dev_id)
            .query("devpassword", &self.creds.dev_password)
            .query("softname", "roms-curator")
            .query("ssid", &self.creds.user)
            .query("sspassword", &self.creds.password)
            .call();
        let response = match response {
            Ok(response) => response,
            Err(ureq::Error::Status(status, _response)) => {
                return match classify_cover_http(status) {
                    CoverHttp::Missing => Ok(None),
                    CoverHttp::Image | CoverHttp::Retry(_) => Err(rejected_request(status).into()),
                };
            }
            Err(ureq::Error::Transport(transport)) => {
                return Err(transport_failure("cover download", &transport).into());
            }
        };
        match classify_cover_http(response.status()) {
            CoverHttp::Missing => return Ok(None),
            CoverHttp::Retry(status) => return Err(rejected_request(status).into()),
            CoverHttp::Image => {}
        }
        let mut bytes = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut bytes)
            .map_err(|error| format!("ScreenScraper cover read failed: {error}"))?;
        if bytes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(bytes))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_title_year_publisher_and_cover_url() {
        let json = json!({
            "response": {
                "jeu": {
                    "id": "3",
                    "noms": [
                        {"region": "jp", "text": "Super Metroid"},
                        {"region": "us", "text": "Super Metroid"}
                    ],
                    "dates": [{"region": "us", "text": "1994-04-18"}],
                    "editeur": {"id": "1", "text": "Nintendo"},
                    "medias": [
                        {"type": "ss", "url": "https://example.test/shot.jpg"},
                        {"type": "box-2D", "url": "https://example.test/box.png"}
                    ]
                }
            }
        });
        let Lookup::Found(game) = parse_jeu_infos(&json).unwrap() else {
            panic!("expected a game");
        };
        assert_eq!(game.title, "Super Metroid");
        assert_eq!(game.year.as_deref(), Some("1994"));
        assert_eq!(game.publisher.as_deref(), Some("Nintendo"));
        assert_eq!(game.provider_id, "3");
        assert_eq!(
            game.thumbnail.as_deref(),
            Some(b"https://example.test/box.png".as_slice())
        );
    }

    #[test]
    fn missing_game_is_not_an_error() {
        let json = json!({"response": {"erreur": "Jeu introuvable"}});
        assert!(matches!(parse_jeu_infos(&json).unwrap(), Lookup::Missing));
    }

    #[test]
    fn http_404_is_a_miss_and_quota_statuses_are_rejected() {
        assert_eq!(classify_jeu_http(200), JeuHttp::Ok);
        assert_eq!(classify_jeu_http(404), JeuHttp::Missing);
        for status in [400, 401, 403, 429, 430, 431] {
            assert_eq!(classify_jeu_http(status), JeuHttp::Rejected(status));
        }
    }

    #[test]
    fn cover_http_retries_quota_and_skips_a_missing_image() {
        assert_eq!(classify_cover_http(200), CoverHttp::Image);
        assert_eq!(classify_cover_http(404), CoverHttp::Missing);
        assert_eq!(classify_cover_http(400), CoverHttp::Missing);
        for status in [401, 403, 423, 426, 429, 430, 431, 500, 503] {
            assert_eq!(classify_cover_http(status), CoverHttp::Retry(status));
        }
    }

    #[test]
    fn credentials_round_trip_without_writing_the_user_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screenscraper.json");
        let creds = ScreenScraperCreds {
            dev_id: "dev".into(),
            dev_password: "dev-secret".into(),
            user: "member".into(),
            password: "member-secret".into(),
        };
        creds.save_stored_at(&path).unwrap();
        assert_eq!(
            ScreenScraperCreds::load_stored_at(&path).unwrap(),
            Some(creds)
        );
        let blank = ScreenScraperCreds {
            dev_id: " ".into(),
            dev_password: "x".into(),
            user: "x".into(),
            password: "x".into(),
        };
        assert!(blank.save_stored_at(&path).is_err());
    }

    #[test]
    fn reads_either_per_minute_field() {
        let live = json!({"response": {"ssuser": {"maxrequestspermin": "10"}}});
        let documented = json!({"ssuser": {"maxrequestsperdmin": 5}});
        assert_eq!(requests_per_minute(&live), Some(10));
        assert_eq!(requests_per_minute(&documented), Some(5));
        assert_eq!(requests_per_minute(&json!({})), None);
    }
}
