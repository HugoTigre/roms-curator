//! [ScreenScraper](https://www.screenscraper.fr/) lookup for SNES and PC.
//!
//! Arcade text stays on the local MAME list. Arcade covers call
//! [`ScreenScraper::cover_for`] with the MAME system id.
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
use super::pc_names::{pc_search_text, titles_match};
use super::prefs::{LibraryPrefs, ScreenScraperQuota};

const JEU_INFOS: &str = "https://www.screenscraper.fr/api2/jeuInfos.php";
const JEU_RECHERCHE: &str = "https://www.screenscraper.fr/api2/jeuRecherche.php";
const SNES_SYSTEM_ID: u32 = 4;
/// Microsoft Windows. DOS is 135 and Windows 3.x is 136.
const PC_WINDOWS_SYSTEM_ID: u32 = 138;
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
            ("dev username", self.dev_id.as_str()),
            ("dev password", self.dev_password.as_str()),
            ("member", self.user.as_str()),
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
    download_covers: bool,
    /// `snes` matches a checksum. `pc` matches a cleaned file name.
    platform: &'static str,
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
            download_covers: true,
            platform: "snes",
        }
    }

    /// Match Windows games by file name instead of a ROM checksum.
    pub fn for_pc(mut self) -> Self {
        self.platform = "pc";
        self
    }

    /// When false, lookups still store text and skip the cover download.
    pub fn download_covers(mut self, enabled: bool) -> Self {
        self.download_covers = enabled;
        self
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
        let Some(checksum) = rom.checksum else {
            return Err("ScreenScraper lookup is missing a ROM checksum".into());
        };
        let request = match checksum {
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
        remember_quota(&json);
        Ok(Some(json))
    }

    /// Box art for a ROM ScreenScraper already knows by checksum. A miss is
    /// `Ok(None)` so the caller can keep metadata from another source.
    pub(crate) fn cover_for(
        &self,
        system_id: u32,
        rom: &RomQuery,
    ) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        if !self.download_covers || rom.checksum.is_none() {
            return Ok(None);
        }
        let Some(json) = self.jeu_infos(system_id, rom)? else {
            return Ok(None);
        };
        let Lookup::Found(mut metadata) = parse_jeu_infos(&json)? else {
            return Ok(None);
        };
        let Some(url_bytes) = metadata.thumbnail.take() else {
            return Ok(None);
        };
        let url = String::from_utf8(url_bytes)
            .map_err(|_| "ScreenScraper cover URL is not UTF-8".to_string())?;
        self.download_thumbnail(&url)
    }

    fn jeu_recherche(&self, system_id: u32, query: &str) -> Result<Option<Value>, Box<dyn Error>> {
        self.wait_turn();
        let parsed = self.fetch_recherche(system_id, query);
        self.finish_request();
        parsed
    }

    fn fetch_recherche(
        &self,
        system_id: u32,
        query: &str,
    ) -> Result<Option<Value>, Box<dyn Error>> {
        let response = self
            .agent
            .get(JEU_RECHERCHE)
            .query("devid", &self.creds.dev_id)
            .query("devpassword", &self.creds.dev_password)
            .query("softname", "roms-curator")
            .query("output", "json")
            .query("ssid", &self.creds.user)
            .query("sspassword", &self.creds.password)
            .query("systemeid", &system_id.to_string())
            .query("recherche", query)
            .call();
        let parsed = read_jeu_response(response)?;
        if let Some(json) = &parsed {
            self.note_rate(json);
        }
        Ok(parsed)
    }

    fn jeu_by_id(
        &self,
        system_id: u32,
        game_id: &str,
        rom_name: &str,
    ) -> Result<Option<Value>, Box<dyn Error>> {
        self.wait_turn();
        let parsed = self.fetch_by_id(system_id, game_id, rom_name);
        self.finish_request();
        parsed
    }

    fn fetch_by_id(
        &self,
        system_id: u32,
        game_id: &str,
        rom_name: &str,
    ) -> Result<Option<Value>, Box<dyn Error>> {
        let response = self
            .agent
            .get(JEU_INFOS)
            .query("devid", &self.creds.dev_id)
            .query("devpassword", &self.creds.dev_password)
            .query("softname", "roms-curator")
            .query("output", "json")
            .query("ssid", &self.creds.user)
            .query("sspassword", &self.creds.password)
            .query("systemeid", &system_id.to_string())
            .query("gameid", game_id)
            .query("romtype", "rom")
            .query("romnom", rom_name)
            .call();
        let parsed = match response {
            Ok(response) => read_ok_json(response)?,
            Err(ureq::Error::Status(status, _response)) if blocks_scrape(status) => {
                return Err(rejected_request(status).into());
            }
            Err(ureq::Error::Status(_, _response)) => None,
            Err(ureq::Error::Transport(transport)) => {
                return Err(transport_failure("request", &transport).into());
            }
        };
        if let Some(json) = &parsed {
            self.note_rate(json);
        }
        Ok(parsed)
    }

    fn finish_lookup(&self, mut lookup: Lookup) -> Result<Lookup, Box<dyn Error>> {
        if let Lookup::Found(metadata) = &mut lookup {
            if self.download_covers {
                if let Some(url_bytes) = metadata.thumbnail.take() {
                    let url = String::from_utf8(url_bytes)
                        .map_err(|_| "ScreenScraper cover URL is not UTF-8".to_string())?;
                    metadata.thumbnail = self.download_thumbnail(&url)?;
                }
            } else {
                metadata.thumbnail = None;
            }
        }
        Ok(lookup)
    }

    fn lookup_pc(&self, rom: &RomQuery) -> Result<Lookup, Box<dyn Error>> {
        let Some(query) = pc_search_text(rom.name) else {
            super::trace::line(format!(
                "{}: the file name has nothing to search.",
                rom.name
            ));
            return Ok(Lookup::Missing);
        };
        let Some(json) = self.jeu_recherche(PC_WINDOWS_SYSTEM_ID, &query)? else {
            super::trace::line(format!(
                "{query}: ScreenScraper had no exact Windows title."
            ));
            return Ok(Lookup::Missing);
        };
        blocking_erreur(&json)?;
        let Some(jeu) = choose_pc_game(&json, &query) else {
            super::trace::line(format!(
                "{query}: ScreenScraper had no exact Windows title."
            ));
            return Ok(Lookup::Missing);
        };
        let mut lookup = metadata_from_jeu(jeu)?;
        let needs_cover = self.download_covers
            && matches!(&lookup, Lookup::Found(metadata) if metadata.thumbnail.is_none());
        if needs_cover {
            if let Some(game_id) = jeu_id(jeu) {
                if let Some(detailed) = self.jeu_by_id(PC_WINDOWS_SYSTEM_ID, &game_id, rom.name)? {
                    if let Lookup::Found(metadata) = parse_jeu_infos(&detailed)? {
                        lookup = Lookup::Found(metadata);
                    }
                }
            }
        }
        self.finish_lookup(lookup)
    }
}

fn read_jeu_response(
    response: Result<ureq::Response, ureq::Error>,
) -> Result<Option<Value>, Box<dyn Error>> {
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
    read_ok_json(response)
}

fn read_ok_json(response: ureq::Response) -> Result<Option<Value>, Box<dyn Error>> {
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
    remember_quota(&json);
    Ok(Some(json))
}

/// A ScreenScraper `erreur` other than a miss fails the lookup.
///
/// Treating it as a miss would clear a game that was already saved.
fn blocking_erreur(json: &Value) -> Result<(), Box<dyn Error>> {
    let response = json.get("response").unwrap_or(json);
    let Some(error) = response.get("erreur").and_then(Value::as_str) else {
        return Ok(());
    };
    let folded = error.to_ascii_lowercase();
    if folded.contains("introuvable") || folded.contains("not found") {
        return Ok(());
    }
    Err(format!("ScreenScraper: {error}").into())
}

fn blocks_scrape(status: u16) -> bool {
    matches!(status, 401 | 403 | 423 | 426 | 429 | 430 | 431) || (500..600).contains(&status)
}

fn choose_pc_game<'a>(json: &'a Value, query: &str) -> Option<&'a Value> {
    let response = json.get("response").unwrap_or(json);
    if let Some(error) = response.get("erreur").and_then(Value::as_str) {
        let folded = error.to_ascii_lowercase();
        if folded.contains("introuvable") || folded.contains("not found") {
            return None;
        }
    }
    jeu_candidates(response).into_iter().find(|jeu| {
        jeu_titles(jeu)
            .iter()
            .any(|title| titles_match(query, title))
    })
}

fn jeu_candidates(response: &Value) -> Vec<&Value> {
    let Some(jeux) = response.get("jeux").or_else(|| response.get("jeu")) else {
        return Vec::new();
    };
    match jeux {
        Value::Array(items) => items.iter().collect(),
        Value::Object(map) if map.contains_key("id") || map.contains_key("noms") => vec![jeux],
        Value::Object(map) => map.values().collect(),
        _ => Vec::new(),
    }
}

fn jeu_titles(jeu: &Value) -> Vec<String> {
    let mut titles = Vec::new();
    if let Some(text) = jeu.get("nom").and_then(Value::as_str) {
        titles.push(text.to_string());
    }
    if let Some(entries) = jeu.get("noms").and_then(Value::as_array) {
        for entry in entries {
            if let Some(text) = entry.get("text").and_then(Value::as_str) {
                titles.push(text.to_string());
            }
        }
    }
    titles
}

fn jeu_id(jeu: &Value) -> Option<String> {
    match jeu.get("id")? {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
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
        platform == self.platform
    }

    fn wants_content_identity(&self) -> bool {
        self.platform != "pc"
    }

    fn lookup(&self, platform: &str, rom: &RomQuery) -> Result<Lookup, Box<dyn Error>> {
        if platform != self.platform {
            return Err(
                format!("ScreenScraper enrichment is not implemented for {platform}").into(),
            );
        }
        if self.platform == "pc" {
            return self.lookup_pc(rom);
        }
        let Some(json) = self.jeu_infos(SNES_SYSTEM_ID, rom)? else {
            return Ok(Lookup::Missing);
        };
        self.finish_lookup(parse_jeu_infos(&json)?)
    }
}

/// Parse a `jeuInfos.php` JSON document.
///
/// `GameMetadata.thumbnail` holds the cover URL until the client downloads it.
pub(crate) fn parse_jeu_infos(json: &Value) -> Result<Lookup, Box<dyn Error>> {
    let response = json.get("response").unwrap_or(json);
    blocking_erreur(json)?;
    if let Some(error) = response.get("erreur").and_then(Value::as_str) {
        let folded = error.to_ascii_lowercase();
        if folded.contains("introuvable") || folded.contains("not found") {
            return Ok(Lookup::Missing);
        }
    }
    let Some(jeu) = response.get("jeu") else {
        return Ok(Lookup::Missing);
    };
    if jeu.is_null() {
        return Ok(Lookup::Missing);
    }
    metadata_from_jeu(jeu)
}

fn metadata_from_jeu(jeu: &Value) -> Result<Lookup, Box<dyn Error>> {
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
        playable: None,
        link: None,
        condition: None,
    }))
}

/// Member allowance from `ssuser`. The live field is `maxrequestspermin`; the
/// written API doc still says `maxrequestsperdmin`.
fn requests_per_minute(json: &Value) -> Option<u32> {
    user_number(json, &["maxrequestspermin", "maxrequestsperdmin"]).filter(|limit| *limit > 0)
}

/// Read the allowance ScreenScraper returns on `ssuser`. Absent fields stay unknown.
pub(crate) fn quota_from_response(json: &Value) -> ScreenScraperQuota {
    ScreenScraperQuota {
        max_per_minute: user_number(json, &["maxrequestspermin", "maxrequestsperdmin"]),
        max_per_day: user_number(json, &["maxrequestsperday"]),
        used_today: user_number(json, &["requeststoday"]),
        max_misses_per_day: user_number(json, &["maxrequestskoperday"]),
        misses_today: user_number(json, &["requestskotoday"]),
        max_threads: user_number(json, &["maxthreads"]),
        max_download_kbps: user_number(json, &["maxdownloadspeed"]),
    }
}

fn remember_quota(json: &Value) {
    let incoming = quota_from_response(json);
    if incoming == ScreenScraperQuota::default() {
        return;
    }
    let Ok(mut prefs) = LibraryPrefs::load() else {
        return;
    };
    prefs.screenscraper = merge_quota(prefs.screenscraper, incoming);
    let _ = prefs.save();
}

/// Keep saved allowance fields this response did not include.
fn merge_quota(saved: ScreenScraperQuota, incoming: ScreenScraperQuota) -> ScreenScraperQuota {
    ScreenScraperQuota {
        max_per_minute: incoming.max_per_minute.or(saved.max_per_minute),
        max_per_day: incoming.max_per_day.or(saved.max_per_day),
        used_today: incoming.used_today.or(saved.used_today),
        max_misses_per_day: incoming.max_misses_per_day.or(saved.max_misses_per_day),
        misses_today: incoming.misses_today.or(saved.misses_today),
        max_threads: incoming.max_threads.or(saved.max_threads),
        max_download_kbps: incoming.max_download_kbps.or(saved.max_download_kbps),
    }
}

fn user_number(json: &Value, keys: &[&str]) -> Option<u32> {
    let response = json.get("response").unwrap_or(json);
    let user = response.get("ssuser").or_else(|| json.get("ssuser"))?;
    for key in keys {
        let Some(value) = user.get(*key) else {
            continue;
        };
        let parsed = match value {
            Value::String(text) => text.parse::<u32>().ok(),
            Value::Number(number) => number
                .as_u64()
                .and_then(|number| u32::try_from(number).ok()),
            _ => None,
        };
        if parsed.is_some() {
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

    #[test]
    fn pc_search_keeps_only_an_obvious_title() {
        let json = json!({
            "response": {
                "jeux": [
                    {"id": "9", "noms": [{"region": "wor", "text": "Return to Monkey Island"}]},
                    {"id": "4", "noms": [{"region": "ss", "text": "Sea of Stars"}]}
                ]
            }
        });
        let chosen = choose_pc_game(&json, "Sea of Stars").unwrap();
        assert_eq!(chosen.get("id").and_then(Value::as_str), Some("4"));
        assert!(choose_pc_game(&json, "Return").is_none());
    }

    #[test]
    fn quota_erreur_fails_the_lookup_instead_of_a_miss() {
        let quota = json!({"response": {"erreur": "API closed for the day"}});
        let error = blocking_erreur(&quota).unwrap_err().to_string();
        assert!(error.contains("API closed for the day"), "{error}");
        assert!(blocking_erreur(&json!({"response": {"erreur": "Jeu introuvable"}})).is_ok());
    }

    #[test]
    fn quota_update_keeps_fields_this_response_omits() {
        let saved = ScreenScraperQuota {
            max_per_minute: Some(12),
            max_per_day: Some(20000),
            used_today: Some(4),
            max_misses_per_day: Some(2000),
            misses_today: Some(1),
            max_threads: Some(1),
            max_download_kbps: Some(128),
        };
        let incoming = quota_from_response(&json!({
            "response": {"ssuser": {"maxrequestspermin": "30"}}
        }));
        let merged = merge_quota(saved, incoming);
        assert_eq!(merged.max_per_minute, Some(30));
        assert_eq!(merged.max_per_day, Some(20000));
        assert_eq!(merged.used_today, Some(4));
        assert_eq!(merged.max_misses_per_day, Some(2000));
        assert_eq!(merged.misses_today, Some(1));
        assert_eq!(merged.max_threads, Some(1));
        assert_eq!(merged.max_download_kbps, Some(128));
    }

    #[test]
    fn quota_leaves_missing_fields_unknown() {
        let json = json!({
            "response": {
                "ssuser": {
                    "maxrequestspermin": "10",
                    "maxrequestsperday": 20000,
                    "requeststoday": "4"
                }
            }
        });
        let quota = quota_from_response(&json);
        assert_eq!(quota.max_per_minute, Some(10));
        assert_eq!(quota.max_per_day, Some(20000));
        assert_eq!(quota.used_today, Some(4));
        assert_eq!(quota.max_misses_per_day, None);
        assert_eq!(quota.misses_today, None);
        assert_eq!(quota.max_threads, None);
        assert_eq!(quota.max_download_kbps, None);
        assert_eq!(
            quota_from_response(&json!({})),
            ScreenScraperQuota::default()
        );
    }
}
