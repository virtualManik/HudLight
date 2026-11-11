//! Where Teams presence comes from: the Teams desktop client's log, or Microsoft Graph.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::config::Status;

// ---------------------------------------------------------------------------------------------
// The Teams client log
// ---------------------------------------------------------------------------------------------

/// The new Teams client writes its own presence into its log, as a line like
/// `... UserDataCrossCloudModule: BroadcastGlobalState: New Global State Event:
/// UserDataGlobalState ... { user id :..., availability: Busy, unread notification count: 0 }`.
/// Classic Teams wrote `StatusIndicatorStateService: Added Busy` into `logs.txt`.
pub fn parse_line(line: &str) -> Option<(Status, String)> {
    if let Some(i) = line.find("availability: ") {
        if !(line.contains("GlobalState") || line.contains("UserData")) {
            return None;
        }
        let rest = &line[i + "availability: ".len()..];
        let word: String = rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
        return Status::from_teams(&word).map(|s| (s, word));
    }
    for marker in ["StatusIndicatorStateService: Added ", "Setting the taskbar overlay icon - "] {
        if let Some(i) = line.find(marker) {
            let rest = &line[i + marker.len()..];
            let word: String = rest.chars().take_while(|c| c.is_ascii_alphabetic() || *c == ' ').collect();
            return Status::from_teams(word.trim()).map(|s| (s, word.trim().to_owned()));
        }
    }
    None
}

/// The newest presence in a chunk of log text.
pub fn last_status(text: &str) -> Option<(Status, String)> {
    text.lines().rev().find_map(parse_line)
}

/// Log folders to look in, newest client first.
fn log_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        if let Ok(entries) = std::fs::read_dir(local.join("Packages")) {
            for e in entries.flatten() {
                if e.file_name().to_string_lossy().starts_with("MSTeams_") {
                    dirs.push(e.path().join("LocalCache").join("Microsoft").join("MSTeams").join("Logs"));
                }
            }
        }
    }
    if let Some(roaming) = std::env::var_os("APPDATA").map(PathBuf::from) {
        dirs.push(roaming.join("Microsoft").join("Teams"));
    }
    dirs
}

/// The log Teams is writing now. New Teams names its files `MSTeams_yyyy-MM-dd_HH-mm-ss.NN.log`,
/// so the greatest name is the newest. Modification times can't be trusted: while Teams holds
/// the new file open, NTFS keeps its directory entry at its creation time, which ties with the
/// old file's last write. Classic Teams' single `logs.txt` is the fallback.
fn newest_log() -> Option<PathBuf> {
    let mut newest: Option<(String, PathBuf)> = None;
    let mut classic = None;
    for dir in log_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("MSTeams_") && name.ends_with(".log") {
                if newest.as_ref().is_none_or(|(n, _)| name > *n) {
                    newest = Some((name, e.path()));
                }
            } else if name == "logs.txt" {
                classic = Some(e.path());
            }
        }
    }
    newest.map(|(_, p)| p).or(classic)
}

/// The newest of these names, as `newest_log` picks it.
#[cfg(test)]
fn pick_newest<'a>(names: &[&'a str]) -> Option<&'a str> {
    names.iter().copied().filter(|n| n.starts_with("MSTeams_") && n.ends_with(".log")).max()
}

/// Follows the newest Teams log as it grows.
pub struct LogWatcher {
    file: Option<PathBuf>,
    offset: u64,
    /// A partial last line, held until its newline arrives.
    carry: String,
    /// The bytes just before `offset`. If they differ on the next read, the file was replaced
    /// in place (same name, maybe the same size) and is read afresh.
    anchor: Vec<u8>,
    /// When the log folders were last searched for a newer file.
    last_scan: Option<std::time::Instant>,
}

/// Teams starts a new file at about 2 MB; reading the tail of one is enough to find the latest
/// presence when the app starts.
const TAIL: u64 = 4 * 1024 * 1024;

impl LogWatcher {
    pub fn new() -> Self {
        LogWatcher { file: None, offset: 0, carry: String::new(), anchor: Vec::new(), last_scan: None }
    }

    pub fn path(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    /// Reads what was appended since last time; returns the newest presence in it.
    pub fn poll(&mut self) -> Option<(Status, String)> {
        let rescan = self.last_scan.is_none_or(|t| t.elapsed() > Duration::from_secs(5));
        if rescan || self.file.is_none() {
            self.last_scan = Some(std::time::Instant::now());
            let newest = newest_log();
            if newest != self.file {
                // A new or rotated file: read its tail, the presence may be near its end.
                self.file = newest;
                self.reset();
                let found = self.read_new(true);
                if found.is_some() {
                    return found;
                }
                // A freshly rotated file may not repeat the presence; keep the old one.
                return None;
            }
        }
        self.read_new(false)
    }

    fn reset(&mut self) {
        self.offset = 0;
        self.carry.clear();
        self.anchor.clear();
    }

    /// Whether the bytes before `offset` are still the ones we read last time.
    fn anchor_holds(&self, f: &mut File) -> bool {
        if self.anchor.is_empty() {
            return true;
        }
        let start = self.offset - self.anchor.len() as u64;
        let mut now = vec![0u8; self.anchor.len()];
        f.seek(SeekFrom::Start(start)).is_ok() && f.read_exact(&mut now).is_ok() && now == self.anchor
    }

    fn read_new(&mut self, from_tail: bool) -> Option<(Status, String)> {
        let path = self.file.clone()?;
        let mut f = File::open(&path).ok()?;
        let len = f.metadata().ok()?.len();
        let mut from_tail = from_tail;
        if len < self.offset || !self.anchor_holds(&mut f) {
            // Truncated or replaced: start over, and find the latest presence in what's there.
            self.reset();
            from_tail = true;
        }
        if from_tail && len > TAIL {
            self.offset = len - TAIL;
        }
        if len == self.offset {
            return None;
        }
        f.seek(SeekFrom::Start(self.offset)).ok()?;
        let mut bytes = Vec::with_capacity((len - self.offset) as usize);
        f.take(len - self.offset).read_to_end(&mut bytes).ok()?;
        self.offset += bytes.len() as u64;
        // The old anchor and these bytes are contiguous in the file; keep the last 64.
        self.anchor.extend_from_slice(&bytes[bytes.len().saturating_sub(64)..]);
        let excess = self.anchor.len().saturating_sub(64);
        self.anchor.drain(..excess);
        let mut text = std::mem::take(&mut self.carry);
        text.push_str(&String::from_utf8_lossy(&bytes));
        match text.rfind('\n') {
            Some(i) => {
                self.carry = text[i + 1..].to_owned();
                text.truncate(i);
            }
            None => {
                self.carry = text;
                return None;
            }
        }
        last_status(&text)
    }
}

/// Whether a Teams client process is running (new `ms-teams.exe` or classic `Teams.exe`).
#[cfg(windows)]
pub fn teams_running() -> Option<bool> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
    Some(text.contains("\"ms-teams.exe\"") || text.contains("\"teams.exe\""))
}

#[cfg(not(windows))]
pub fn teams_running() -> Option<bool> {
    None
}

// ---------------------------------------------------------------------------------------------
// Microsoft Graph
// ---------------------------------------------------------------------------------------------

pub const SCOPES: &str = "Presence.Read User.Read offline_access";

#[derive(Debug, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(default = "five")]
    pub interval: u64,
}

fn five() -> u64 {
    5
}

#[derive(Debug, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct OAuthError {
    error: String,
    #[serde(default)]
    error_description: String,
}

#[derive(Debug)]
pub enum PollResult {
    Pending,
    SlowDown,
    Done(Tokens),
    Failed(String),
}

fn authority(tenant: &str) -> String {
    let t = if tenant.trim().is_empty() { "organizations" } else { tenant.trim() };
    format!("https://login.microsoftonline.com/{t}/oauth2/v2.0")
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build()
}

fn read_error(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            match serde_json::from_str::<OAuthError>(&body) {
                Ok(err) => {
                    let first = err.error_description.lines().next().unwrap_or("").to_owned();
                    format!("{} ({code}): {first}", err.error)
                }
                Err(_) => format!("HTTP {code}"),
            }
        }
        ureq::Error::Transport(t) => t.to_string(),
    }
}

pub fn start_device_code(client_id: &str, tenant: &str) -> Result<DeviceCode, String> {
    agent()
        .post(&format!("{}/devicecode", authority(tenant)))
        .send_form(&[("client_id", client_id), ("scope", SCOPES)])
        .map_err(read_error)?
        .into_json::<DeviceCode>()
        .map_err(|e| e.to_string())
}

pub fn poll_device_code(client_id: &str, tenant: &str, device_code: &str) -> PollResult {
    let res = agent().post(&format!("{}/token", authority(tenant))).send_form(&[
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ("client_id", client_id),
        ("device_code", device_code),
    ]);
    match res {
        Ok(r) => match r.into_json::<Tokens>() {
            Ok(t) => PollResult::Done(t),
            Err(e) => PollResult::Failed(e.to_string()),
        },
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            match serde_json::from_str::<OAuthError>(&body) {
                Ok(e) if e.error == "authorization_pending" => PollResult::Pending,
                Ok(e) if e.error == "slow_down" => PollResult::SlowDown,
                Ok(e) if e.error == "expired_token" => PollResult::Failed("The sign-in code expired. Try again.".into()),
                Ok(e) if e.error == "authorization_declined" => PollResult::Failed("Sign-in was declined.".into()),
                Ok(e) => PollResult::Failed(format!("{}: {}", e.error, e.error_description.lines().next().unwrap_or(""))),
                Err(_) => PollResult::Failed(format!("HTTP {code}")),
            }
        }
        Err(e) => PollResult::Failed(read_error(e)),
    }
}

pub fn refresh(client_id: &str, tenant: &str, refresh_token: &str) -> Result<Tokens, String> {
    agent()
        .post(&format!("{}/token", authority(tenant)))
        .send_form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("scope", SCOPES),
        ])
        .map_err(read_error)?
        .into_json::<Tokens>()
        .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
struct PresenceJson {
    #[serde(default)]
    availability: String,
    #[serde(default)]
    activity: String,
}

/// Graph's activity is more specific than its availability (a call, a presentation, out of
/// office), so it wins where it names one of those.
pub fn presence_status(availability: &str, activity: &str) -> Option<Status> {
    match Status::from_teams(activity) {
        Some(s @ (Status::InCall | Status::Presenting | Status::OutOfOffice | Status::DoNotDisturb | Status::BeRightBack)) => Some(s),
        _ => Status::from_teams(availability),
    }
}

pub fn get_presence(access_token: &str) -> Result<(Status, String), GraphError> {
    let res = agent()
        .get("https://graph.microsoft.com/v1.0/me/presence")
        .set("Authorization", &format!("Bearer {access_token}"))
        .call();
    match res {
        Ok(r) => {
            let p: PresenceJson = r.into_json().map_err(|e| GraphError::Other(e.to_string()))?;
            let status = presence_status(&p.availability, &p.activity).unwrap_or(Status::Unknown);
            Ok((status, format!("{} / {}", p.availability, p.activity)))
        }
        Err(ureq::Error::Status(401, _)) => Err(GraphError::Unauthorized),
        Err(e) => Err(GraphError::Other(read_error(e))),
    }
}

#[derive(Debug, Deserialize)]
struct Me {
    #[serde(rename = "displayName", default)]
    display_name: String,
    #[serde(rename = "userPrincipalName", default)]
    upn: String,
}

pub fn get_me(access_token: &str) -> Option<String> {
    let me: Me = agent()
        .get("https://graph.microsoft.com/v1.0/me?$select=displayName,userPrincipalName")
        .set("Authorization", &format!("Bearer {access_token}"))
        .call()
        .ok()?
        .into_json()
        .ok()?;
    Some(if me.display_name.is_empty() { me.upn } else { me.display_name })
}

#[derive(Debug)]
pub enum GraphError {
    Unauthorized,
    Other(String),
}

fn token_path() -> PathBuf {
    crate::config::Config::dir().join("graph-token")
}

/// The refresh token, kept beside the config and readable only by this Windows user.
pub fn load_refresh_token() -> Option<String> {
    let raw = std::fs::read(token_path()).ok()?;
    let plain = crate::secret::unprotect(&raw)?;
    String::from_utf8(plain).ok().filter(|s| !s.is_empty())
}

pub fn save_refresh_token(token: Option<&str>) {
    let path = token_path();
    match token {
        Some(t) => {
            let _ = std::fs::create_dir_all(crate::config::Config::dir());
            if let Some(sealed) = crate::secret::protect(t.as_bytes()) {
                let _ = std::fs::write(path, sealed);
            }
        }
        None => {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEW_TEAMS: &str = "2024-04-01T16:11:01.694594-04:00 0x00007b24 <INFO> native_modules::UserDataCrossCloudModule: BroadcastGlobalState: New Global State Event: UserDataGlobalState total number of users: 1 { user id :abc, availability: DoNotDisturb, unread notification count: 0 }";

    #[test]
    fn reads_new_teams_presence() {
        assert_eq!(parse_line(NEW_TEAMS).map(|(s, _)| s), Some(Status::DoNotDisturb));
    }

    #[test]
    fn reads_classic_teams_presence() {
        let l = "Mon Apr 01 2024 10:00:00 GMT-0400 <1234> -- info -- StatusIndicatorStateService: Added BeRightBack (current state: Available -> BeRightBack)";
        assert_eq!(parse_line(l).map(|(s, _)| s), Some(Status::BeRightBack));
    }

    #[test]
    fn ignores_other_lines_and_takes_the_newest() {
        assert_eq!(parse_line("availability: Busy, but not a global state"), None);
        let text = format!("{NEW_TEAMS}\nnoise\n{}", NEW_TEAMS.replace("DoNotDisturb", "Away"));
        assert_eq!(last_status(&text).map(|(s, _)| s), Some(Status::Away));
    }

    #[test]
    fn the_newest_log_is_the_greatest_name() {
        let names = ["MSTeams_2026-11-10_09-14-02.00.log", "MSTeams_2026-11-10_13-55-29.00.log", "MSTeams_2026-11-10_13-55-29.01.log", "logs.txt"];
        assert_eq!(pick_newest(&names), Some("MSTeams_2026-11-10_13-55-29.01.log"));
    }

    #[test]
    fn graph_activity_wins_when_specific() {
        assert_eq!(presence_status("Busy", "InAMeeting"), Some(Status::InCall));
        assert_eq!(presence_status("Busy", "Busy"), Some(Status::Busy));
        assert_eq!(presence_status("Away", "OutOfOffice"), Some(Status::OutOfOffice));
        assert_eq!(presence_status("DoNotDisturb", "Presenting"), Some(Status::Presenting));
    }

    #[test]
    fn the_watcher_follows_appends() {
        let dir = std::env::temp_dir().join(format!("blt-watch-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("MSTeams_test.log");
        std::fs::write(&path, format!("{NEW_TEAMS}\n")).unwrap();
        let mut w = LogWatcher { file: Some(path.clone()), offset: 0, carry: String::new(), anchor: Vec::new(), last_scan: Some(std::time::Instant::now()) };
        assert_eq!(w.poll().map(|(s, _)| s), Some(Status::DoNotDisturb));
        assert_eq!(w.poll(), None);
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        // Half a line first: nothing until its newline lands.
        let away = NEW_TEAMS.replace("DoNotDisturb", "Away");
        let (a, b) = away.split_at(40);
        write!(f, "{a}").unwrap();
        assert_eq!(w.poll(), None);
        writeln!(f, "{b}").unwrap();
        assert_eq!(w.poll().map(|(s, _)| s), Some(Status::Away));
        drop(f);
        // Replaced in place with a file of the same length: noticed, and read afresh.
        let replaced = std::fs::read_to_string(&path).unwrap().replace("availability: Away", "availability: Busy");
        std::fs::write(&path, replaced).unwrap();
        assert_eq!(w.poll().map(|(s, _)| s), Some(Status::Busy));
        let _ = std::fs::remove_dir_all(dir);
    }
}
