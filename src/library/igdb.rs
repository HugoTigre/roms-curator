//! [IGDB](https://www.igdb.com/) is the first lookup for PC games.
//!
//! The login is a Twitch application Client ID and Client Secret. The client
//! exchanges those for an app token. A game is kept only when its name matches
//! the cleaned file name and the entry is on Windows (platform id 6).

use std::error::Error;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::enrich::{GameMetadata, Lookup, MetadataProvider, RomQuery};
use super::pc_names::{pc_search_text, titles_match};
use super::screenscraper::ScreenScraper;

const TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";
const GAMES_URL: &str = "https://api.igdb.com/v4/games";
/// IGDB platform id for Microsoft Windows.
const WINDOWS_PLATFORM_ID: i64 = 6;
/// Stay under IGDB's 4 requests per second.
const REQUEST_INTERVAL: Duration = Duration::from_millis(250);

/// Twitch application credentials. The secret never goes to the image host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IgdbCreds {
    pub client_id: String,
    pub client_secret: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: i64,
}

#[derive(Debug, Clone)]
struct CachedToken {
    access_token: String,
    expires_at: i64,
}

#[derive(Debug, Deserialize)]
struct IgdbGame {
    id: i64,
    name: String,
    slug: Option<String>,
    first_release_date: Option<i64>,
    cover: Option<IgdbCover>,
    platforms: Option<Vec<i64>>,
    category: Option<i64>,
    involved_companies: Option<Vec<IgdbInvolved>>,
}

#[derive(Debug, Deserialize)]
struct IgdbCover {
    image_id: String,
}

#[derive(Debug, Deserialize)]
struct IgdbInvolved {
    publisher: Option<bool>,
    company: Option<IgdbCompany>,
}

#[derive(Debug, Deserialize)]
struct IgdbCompany {
    name: String,
}

/// One IGDB hit shown for a manual map. The name is not required to match the file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct IgdbCandidate {
    pub id: i64,
    pub name: String,
    pub year: Option<String>,
    pub category: String,
}

struct Pace {
    ready_at: Option<Instant>,
}

/// IGDB client. Game queries run one at a time, at most four per second.
pub struct Igdb {
    creds: IgdbCreds,
    store_path: Option<PathBuf>,
    agent: ureq::Agent,
    token: Mutex<Option<CachedToken>>,
    pace: Mutex<Pace>,
    download_covers: bool,
}

/// IGDB first for PC. ScreenScraper is used when IGDB has no safe match.
pub struct PcSources {
    screenscraper: Option<ScreenScraper>,
    igdb: Option<Igdb>,
}

impl IgdbCreds {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
            access_token: None,
            expires_at: None,
        }
    }

    pub fn from_env() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            client_id: required_env("ROMS_CURATOR_IGDB_CLIENT_ID")?,
            client_secret: required_env("ROMS_CURATOR_IGDB_CLIENT_SECRET")?,
            access_token: None,
            expires_at: None,
        })
    }

    pub fn load_stored() -> Result<Option<Self>, Box<dyn Error>> {
        Self::load_stored_at(&super::paths::library_igdb_path()?)
    }

    pub fn load_stored_at(path: &Path) -> Result<Option<Self>, Box<dyn Error>> {
        if !path.is_file() {
            return Ok(None);
        }
        let text = fs::read_to_string(path)
            .map_err(|error| format!("Cannot read IGDB credentials {}: {error}", path.display()))?;
        let creds = serde_json::from_str(&text).map_err(|error| {
            format!("IGDB credentials {} are not valid: {error}", path.display())
        })?;
        Ok(Some(creds))
    }

    pub fn save_stored(&self) -> Result<(), Box<dyn Error>> {
        self.save_stored_at(&super::paths::library_igdb_path()?)
    }

    /// Save the Client ID and secret. A cached token for the same pair is kept.
    pub fn save_stored_at(&self, path: &Path) -> Result<(), Box<dyn Error>> {
        self.validate()?;
        let mut stored = Self {
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            access_token: None,
            expires_at: None,
        };
        if let Some(existing) = Self::load_stored_at(path)? {
            if existing.client_id == stored.client_id
                && existing.client_secret == stored.client_secret
            {
                stored.access_token = existing.access_token;
                stored.expires_at = existing.expires_at;
            }
        }
        stored.write_at(path)
    }

    fn validate(&self) -> Result<(), Box<dyn Error>> {
        for (label, value) in [
            ("Client ID", self.client_id.as_str()),
            ("Client Secret", self.client_secret.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("IGDB {label} is empty").into());
            }
        }
        Ok(())
    }

    fn write_at(&self, path: &Path) -> Result<(), Box<dyn Error>> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_vec_pretty(self)?;
        write_private(path, &json)
    }
}

impl Igdb {
    pub fn new(creds: IgdbCreds) -> Self {
        Self::with_store(creds, None)
    }

    /// Client that writes a refreshed token back to the saved credentials file.
    pub fn stored(creds: IgdbCreds) -> Result<Self, Box<dyn Error>> {
        let path = super::paths::library_igdb_path()?;
        Ok(Self::with_store(creds, Some(path)))
    }

    pub fn download_covers(mut self, enabled: bool) -> Self {
        self.download_covers = enabled;
        self
    }

    fn with_store(creds: IgdbCreds, store_path: Option<PathBuf>) -> Self {
        let token = match (&creds.access_token, creds.expires_at) {
            (Some(access_token), Some(expires_at)) => Some(CachedToken {
                access_token: access_token.clone(),
                expires_at,
            }),
            _ => None,
        };
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
            store_path,
            agent,
            token: Mutex::new(token),
            pace: Mutex::new(Pace { ready_at: None }),
            download_covers: true,
        }
    }

    pub(crate) fn lookup_pc(&self, rom: &RomQuery) -> Result<Lookup, Box<dyn Error>> {
        let Some(query) = pc_search_text(rom.name) else {
            return Ok(Lookup::Missing);
        };
        let games = self.games(&search_body(&query))?;
        let Some(game) = choose_igdb_game(&games, &query) else {
            return Ok(Lookup::Missing);
        };
        Ok(Lookup::Found(self.metadata_for(game)?))
    }

    /// Search results for a person to pick. Unlike [`Self::lookup_pc`], a near name is kept.
    pub fn search_candidates(&self, query: &str) -> Result<Vec<IgdbCandidate>, Box<dyn Error>> {
        let query = query.trim();
        if query.is_empty() {
            return Err("Type a name to search.".into());
        }
        let games = self.games(&browse_body(query))?;
        Ok(games.iter().map(candidate_from).collect())
    }

    /// Metadata for the IGDB game the user chose.
    pub fn metadata_by_id(&self, id: i64) -> Result<GameMetadata, Box<dyn Error>> {
        let games = self.games(&by_id_body(id))?;
        let Some(game) = games.first() else {
            return Err("IGDB has no game with that id.".into());
        };
        self.metadata_for(game)
    }

    fn metadata_for(&self, game: &IgdbGame) -> Result<GameMetadata, Box<dyn Error>> {
        let thumbnail = if self.download_covers {
            match game
                .cover
                .as_ref()
                .and_then(|cover| cover_url(&cover.image_id))
            {
                Some(url) => self.download_cover(&url)?,
                None => None,
            }
        } else {
            None
        };
        Ok(GameMetadata {
            provider: "igdb".into(),
            provider_id: game.id.to_string(),
            title: game.name.clone(),
            year: game.first_release_date.and_then(year_from_unix),
            publisher: publisher(game),
            thumbnail,
            playable: None,
            link: game.slug.as_deref().and_then(game_link),
            condition: None,
        })
    }

    fn games(&self, body: &str) -> Result<Vec<IgdbGame>, Box<dyn Error>> {
        match self.games_once(body)? {
            GamesOnce::Found(games) => Ok(games),
            GamesOnce::Unauthorized => {
                self.clear_token();
                match self.games_once(body)? {
                    GamesOnce::Found(games) => Ok(games),
                    GamesOnce::Unauthorized => {
                        Err("IGDB rejected the Client ID or Client Secret (HTTP 401)".into())
                    }
                }
            }
        }
    }

    fn games_once(&self, body: &str) -> Result<GamesOnce, Box<dyn Error>> {
        self.wait_turn();
        let token = self.access_token()?;
        let response = self
            .agent
            .post(GAMES_URL)
            .set("Client-ID", &self.creds.client_id)
            .set("Authorization", &format!("Bearer {token}"))
            .set("Accept", "application/json")
            .send_string(body);
        self.finish_request();
        let response = match response {
            Ok(response) => response,
            Err(ureq::Error::Status(401, _response)) => return Ok(GamesOnce::Unauthorized),
            Err(ureq::Error::Status(status, _response)) => {
                return Err(rejected(status).into());
            }
            Err(ureq::Error::Transport(transport)) => {
                return Err(format!("IGDB request failed ({})", transport.kind()).into());
            }
        };
        if response.status() == 401 {
            return Ok(GamesOnce::Unauthorized);
        }
        if response.status() != 200 {
            return Err(rejected(response.status()).into());
        }
        let text = response
            .into_string()
            .map_err(|_| "IGDB returned a body that is not text".to_string())?;
        Ok(GamesOnce::Found(parse_games(&text)?))
    }

    fn access_token(&self) -> Result<String, Box<dyn Error>> {
        let mut cached = self.token.lock().expect("igdb token");
        if let Some(token) = cached.as_ref() {
            if token.expires_at > now_unix() + 60 {
                return Ok(token.access_token.clone());
            }
        }
        let fresh = self.fetch_token()?;
        let access_token = fresh.access_token.clone();
        self.persist_token(&fresh);
        *cached = Some(fresh);
        Ok(access_token)
    }

    fn clear_token(&self) {
        *self.token.lock().expect("igdb token") = None;
    }

    fn fetch_token(&self) -> Result<CachedToken, Box<dyn Error>> {
        let response = self
            .agent
            .post(TOKEN_URL)
            .query("client_id", &self.creds.client_id)
            .query("client_secret", &self.creds.client_secret)
            .query("grant_type", "client_credentials")
            .call();
        let response = match response {
            Ok(response) => response,
            Err(ureq::Error::Status(status, _response)) => {
                return Err(rejected(status).into());
            }
            Err(ureq::Error::Transport(transport)) => {
                return Err(format!("IGDB token request failed ({})", transport.kind()).into());
            }
        };
        if response.status() != 200 {
            return Err(rejected(response.status()).into());
        }
        let text = response
            .into_string()
            .map_err(|_| "IGDB token response is not text".to_string())?;
        let parsed: TokenResponse =
            serde_json::from_str(&text).map_err(|error| format!("IGDB token JSON: {error}"))?;
        if parsed.access_token.is_empty() || parsed.expires_in <= 0 {
            return Err("IGDB token response has no access token".into());
        }
        Ok(CachedToken {
            access_token: parsed.access_token,
            expires_at: now_unix().saturating_add(parsed.expires_in),
        })
    }

    fn persist_token(&self, token: &CachedToken) {
        let Some(path) = &self.store_path else {
            return;
        };
        let Ok(Some(mut creds)) = IgdbCreds::load_stored_at(path) else {
            return;
        };
        if creds.client_id != self.creds.client_id
            || creds.client_secret != self.creds.client_secret
        {
            return;
        }
        creds.access_token = Some(token.access_token.clone());
        creds.expires_at = Some(token.expires_at);
        let _ = creds.write_at(path);
    }

    fn download_cover(&self, url: &str) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        let response = self.agent.get(url).call();
        let response = match response {
            Ok(response) => response,
            Err(ureq::Error::Status(status, _response)) => {
                return if status == 401 || status == 429 || (500..600).contains(&status) {
                    Err(rejected(status).into())
                } else {
                    Ok(None)
                };
            }
            Err(ureq::Error::Transport(transport)) => {
                return Err(format!("IGDB cover request failed ({})", transport.kind()).into());
            }
        };
        if response.status() != 200 {
            return if response.status() == 429 || (500..600).contains(&response.status()) {
                Err(rejected(response.status()).into())
            } else {
                Ok(None)
            };
        }
        let mut bytes = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut bytes)
            .map_err(|error| format!("IGDB cover read failed: {error}"))?;
        if bytes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(bytes))
        }
    }

    fn wait_turn(&self) {
        let pace = self.pace.lock().expect("igdb pace");
        if let Some(ready_at) = pace.ready_at {
            let now = Instant::now();
            if ready_at > now {
                std::thread::sleep(ready_at - now);
            }
        }
    }

    fn finish_request(&self) {
        let mut pace = self.pace.lock().expect("igdb pace");
        pace.ready_at = Some(Instant::now() + REQUEST_INTERVAL);
    }
}

enum GamesOnce {
    Found(Vec<IgdbGame>),
    Unauthorized,
}

impl PcSources {
    pub fn new(screenscraper: Option<ScreenScraper>, igdb: Option<Igdb>) -> Self {
        Self {
            screenscraper,
            igdb,
        }
    }
}

impl MetadataProvider for PcSources {
    fn supports(&self, platform: &str) -> bool {
        platform == "pc"
    }

    fn wants_content_identity(&self) -> bool {
        false
    }

    fn lookup(&self, platform: &str, rom: &RomQuery) -> Result<Lookup, Box<dyn Error>> {
        if platform != "pc" {
            return Err(format!("PC enrichment is not implemented for {platform}").into());
        }
        match (&self.igdb, &self.screenscraper) {
            (Some(igdb), screenscraper) => match igdb.lookup_pc(rom)? {
                Lookup::Found(metadata) => Ok(Lookup::Found(metadata)),
                Lookup::Missing => {
                    let Some(screenscraper) = screenscraper else {
                        super::trace::line(format!(
                            "{}: IGDB had no exact Windows title. ScreenScraper credentials are not saved.",
                            rom.name
                        ));
                        return Ok(Lookup::Missing);
                    };
                    super::trace::line(format!(
                        "{}: IGDB had no exact Windows title. Trying ScreenScraper.",
                        rom.name
                    ));
                    screenscraper.lookup(platform, rom)
                }
            },
            (None, Some(screenscraper)) => {
                super::trace::line(format!(
                    "{}: IGDB credentials are not saved. Trying ScreenScraper.",
                    rom.name
                ));
                screenscraper.lookup(platform, rom)
            }
            (None, None) => {
                Err("Save IGDB or ScreenScraper credentials before scraping PC.".into())
            }
        }
    }
}

fn search_body(query: &str) -> String {
    format!(
        "search \"{}\";\nfields name,slug,first_release_date,cover.image_id,platforms,involved_companies.publisher,involved_companies.company.name;\nwhere platforms = ({WINDOWS_PLATFORM_ID});\nlimit 20;",
        apicalypse_string(query)
    )
}

fn browse_body(query: &str) -> String {
    format!(
        "search \"{}\";\nfields name,first_release_date,category;\nlimit 10;",
        apicalypse_string(query)
    )
}

fn by_id_body(id: i64) -> String {
    format!(
        "fields name,slug,first_release_date,cover.image_id,platforms,involved_companies.publisher,involved_companies.company.name;\nwhere id = {id};\nlimit 1;"
    )
}

fn apicalypse_string(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other if other.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", other as u32));
            }
            other => escaped.push(other),
        }
    }
    escaped
}

fn candidate_from(game: &IgdbGame) -> IgdbCandidate {
    IgdbCandidate {
        id: game.id,
        name: game.name.clone(),
        year: game.first_release_date.and_then(year_from_unix),
        category: category_label(game.category),
    }
}

fn category_label(category: Option<i64>) -> String {
    match category {
        Some(0) => "Main game",
        Some(1) => "DLC",
        Some(2) => "Expansion",
        Some(3) => "Bundle",
        Some(4) => "Standalone expansion",
        Some(5) => "Mod",
        Some(8) => "Remake",
        Some(9) => "Remaster",
        Some(10) => "Expanded game",
        Some(11) => "Port",
        Some(13) => "Pack",
        _ => "Game",
    }
    .to_string()
}

fn choose_igdb_game<'a>(games: &'a [IgdbGame], query: &str) -> Option<&'a IgdbGame> {
    games.iter().find(|game| {
        game.platforms
            .as_ref()
            .is_some_and(|platforms| platforms.contains(&WINDOWS_PLATFORM_ID))
            && titles_match(query, &game.name)
    })
}

fn parse_games(text: &str) -> Result<Vec<IgdbGame>, Box<dyn Error>> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("IGDB JSON: {error}"))?;
    if let Some(message) = value.get("message").and_then(|message| message.as_str()) {
        return Err(format!("IGDB: {message}").into());
    }
    serde_json::from_value(value).map_err(|error| format!("IGDB JSON: {error}").into())
}

fn publisher(game: &IgdbGame) -> Option<String> {
    let companies = game.involved_companies.as_ref()?;
    let names: Vec<(bool, String)> = companies
        .iter()
        .filter_map(|company| {
            let name = company.company.as_ref()?.name.clone();
            Some((company.publisher.unwrap_or(false), name))
        })
        .collect();
    names
        .iter()
        .find(|(is_publisher, _)| *is_publisher)
        .or_else(|| names.first())
        .map(|(_, name)| name.clone())
}

fn year_from_unix(seconds: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(seconds, 0).map(|time| time.format("%Y").to_string())
}

fn cover_url(image_id: &str) -> Option<String> {
    if image_id.is_empty()
        || !image_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    Some(format!(
        "https://images.igdb.com/igdb/image/upload/t_cover_big/{image_id}.jpg"
    ))
}

fn game_link(slug: &str) -> Option<String> {
    if slug.is_empty()
        || !slug
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        return None;
    }
    Some(format!("https://www.igdb.com/games/{slug}"))
}

fn rejected(status: u16) -> String {
    format!("IGDB rejected the request (HTTP {status})")
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn required_env(name: &str) -> Result<String, Box<dyn Error>> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(format!("Set {name} to use IGDB").into()),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn game(name: &str, platforms: &[i64]) -> IgdbGame {
        IgdbGame {
            id: 1,
            name: name.into(),
            slug: None,
            first_release_date: Some(1_692_835_200),
            cover: None,
            platforms: Some(platforms.to_vec()),
            category: None,
            involved_companies: None,
        }
    }

    #[test]
    fn keeps_a_windows_title_and_skips_a_longer_or_other_platform_name() {
        let games = vec![
            game("Return to Monkey Island", &[WINDOWS_PLATFORM_ID]),
            game("Blasphemous 2", &[130]),
            game("Blasphemous 2", &[WINDOWS_PLATFORM_ID, 130]),
        ];
        let chosen = choose_igdb_game(&games, "Blasphemous 2").unwrap();
        assert_eq!(
            chosen.platforms.as_deref(),
            Some(&[WINDOWS_PLATFORM_ID, 130][..])
        );
        assert!(choose_igdb_game(&games, "Return").is_none());
        assert!(choose_igdb_game(
            &[game("Blasphemous II", &[WINDOWS_PLATFORM_ID])],
            "Blasphemous 2"
        )
        .is_none());
        assert_eq!(
            apicalypse_string("line\n\"next\"\r\t"),
            "line\\n\\\"next\\\"\\r\\t"
        );
        assert!(!apicalypse_string("a\nb").contains('\n'));
        assert_eq!(year_from_unix(1_692_835_200).as_deref(), Some("2023"));
        assert_eq!(category_label(Some(0)), "Main game");
        assert_eq!(category_label(Some(3)), "Bundle");
    }

    #[test]
    fn credentials_round_trip_keeps_a_token_for_the_same_secret() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("igdb.json");
        let creds = IgdbCreds {
            client_id: "client".into(),
            client_secret: "secret".into(),
            access_token: Some("token".into()),
            expires_at: Some(10),
        };
        creds.write_at(&path).unwrap();
        IgdbCreds {
            client_id: "client".into(),
            client_secret: "secret".into(),
            access_token: None,
            expires_at: None,
        }
        .save_stored_at(&path)
        .unwrap();
        let loaded = IgdbCreds::load_stored_at(&path).unwrap().unwrap();
        assert_eq!(loaded.access_token.as_deref(), Some("token"));
        IgdbCreds {
            client_id: "client".into(),
            client_secret: "other".into(),
            access_token: None,
            expires_at: None,
        }
        .save_stored_at(&path)
        .unwrap();
        let replaced = IgdbCreds::load_stored_at(&path).unwrap().unwrap();
        assert_eq!(replaced.access_token, None);
        assert!(IgdbCreds {
            client_id: " ".into(),
            client_secret: "secret".into(),
            access_token: None,
            expires_at: None,
        }
        .save_stored_at(&path)
        .is_err());
    }
}
