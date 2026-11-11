//! Updates from GitHub Releases.
//!
//! A background thread asks GitHub for the latest release of [`REPO`] a minute after start and
//! every six hours after that. When the tag (`v1.2.3`) is newer than this build it reports
//! [`UpdateState::Available`], and the window shows it in the sidebar and offers it in a dialog.
//!
//! Installing downloads the release's `hudlight.exe`, checks its SHA-256 against GitHub's asset
//! digest (or a `hudlight.exe.sha256` asset), and swaps it in for the running exe. Windows lets a
//! running exe be renamed, so the old one moves aside to `hudlight.exe.old` and is deleted on the
//! next start. Then HudLight restarts into the new version.
//!
//! An update installs when you choose **Update now**, or by itself when "Install updates
//! automatically" is on and the window isn't showing (HudLight is in the tray), so it never
//! restarts under you.
//!
//! Releases come from `.github/workflows/release.yml`, which builds and publishes one for every
//! `v*` tag.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// The GitHub repository releases come from.
pub const REPO: &str = "virtualManik/HudLight";
/// The release asset that holds the app.
const ASSET: &str = "hudlight.exe";
const FIRST_CHECK: Duration = Duration::from_secs(60);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateState {
    Checking,
    UpToDate,
    /// A newer release is out, waiting for the go-ahead.
    Available(Available),
    Downloading { version: String },
    /// The new exe is in place; HudLight is restarting into it.
    Ready { version: String },
    /// Installing failed; the update is still on offer.
    Failed { update: Available, error: String },
    /// Checking failed.
    Error(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Available {
    pub version: String,
    /// The release notes, as written on GitHub.
    pub notes: String,
    /// The release's page on GitHub.
    pub url: String,
}

impl UpdateState {
    /// One line for the UI.
    pub fn label(&self) -> String {
        match self {
            UpdateState::Checking => "Checking for updates…".into(),
            UpdateState::UpToDate => format!("Up to date ({CURRENT})"),
            UpdateState::Available(a) => format!("Version {} is available", a.version),
            UpdateState::Downloading { version } => format!("Downloading {version}…"),
            UpdateState::Ready { version } => format!("{version} is installed. Restarting…"),
            UpdateState::Failed { error, .. } => format!("Update failed: {error}"),
            UpdateState::Error(e) => format!("Couldn't check for updates: {e}"),
        }
    }

    /// The update on offer, if any.
    pub fn available(&self) -> Option<&Available> {
        match self {
            UpdateState::Available(a) | UpdateState::Failed { update: a, .. } => Some(a),
            _ => None,
        }
    }
}

#[derive(Clone, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: String,
    assets: Vec<Asset>,
}

#[derive(Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    /// `sha256:<hex>`, which GitHub computes for uploaded assets.
    #[serde(default)]
    digest: Option<String>,
}

impl Release {
    fn version(&self) -> String {
        self.tag_name.trim_start_matches(['v', 'V']).to_owned()
    }

    fn offer(&self) -> Available {
        Available { version: self.version(), notes: self.body.clone().unwrap_or_default().trim().to_owned(), url: self.html_url.clone() }
    }
}

/// `1.2.3` from `v1.2.3` or `1.2.3`; pre-release and build suffixes are dropped.
fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let core = s.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let v = (parts.next()??, parts.next().unwrap_or(Some(0))?, parts.next().unwrap_or(Some(0))?);
    parts.next().is_none().then_some(v)
}

pub fn is_newer(tag: &str, current: &str) -> bool {
    matches!((parse_version(tag), parse_version(current)), (Some(t), Some(c)) if t > c)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .user_agent(&format!("HudLight/{CURRENT}"))
        .build()
}

/// The latest release when it's newer than this build.
fn newer_release(agent: &ureq::Agent) -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let release = match agent.get(&url).set("Accept", "application/vnd.github+json").call() {
        Ok(r) => r.into_json::<Release>().map_err(|e| format!("couldn't read the release: {e}"))?,
        // No release published yet.
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(e) => return Err(format!("couldn't reach GitHub: {e}")),
    };
    Ok((!release.draft && !release.prerelease && is_newer(&release.tag_name, CURRENT)).then_some(release))
}

fn expected_sha256(agent: &ureq::Agent, release: &Release, asset: &Asset) -> Result<String, String> {
    if let Some(hex) = asset.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")) {
        return Ok(hex.to_ascii_lowercase());
    }
    let sums = release.assets.iter().find(|a| a.name == format!("{ASSET}.sha256")).ok_or("the release has no checksum for hudlight.exe")?;
    let text = agent.get(&sums.browser_download_url).call().map_err(|e| e.to_string())?.into_string().map_err(|e| e.to_string())?;
    // `sha256sum` style: "<hex>  hudlight.exe", or just the hex.
    let hex = text.split_whitespace().next().unwrap_or_default().to_ascii_lowercase();
    if hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()) { Ok(hex) } else { Err("the release's checksum file is malformed".into()) }
}

fn download(agent: &ureq::Agent, url: &str, to: &Path) -> Result<String, String> {
    let resp = agent.get(url).call().map_err(|e| format!("download failed: {e}"))?;
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(to).map_err(|e| format!("couldn't save the update: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = std::io::Read::read(&mut reader, &mut buf).map_err(|e| format!("download failed: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        std::io::Write::write_all(&mut file, &buf[..n]).map_err(|e| format!("couldn't save the update: {e}"))?;
    }
    std::io::Write::flush(&mut file).map_err(|e| e.to_string())?;
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn old_path(exe: &Path) -> PathBuf {
    exe.with_extension("exe.old")
}

/// Puts `new` where the running exe is, keeping the old one aside until the next start.
fn swap_in(new: &Path, exe: &Path) -> Result<(), String> {
    let old = old_path(exe);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).map_err(|e| format!("couldn't replace {} ({e}). Is it in a protected folder?", exe.display()))?;
    if let Err(e) = std::fs::copy(new, exe) {
        let _ = std::fs::rename(&old, exe);
        return Err(format!("couldn't install the update: {e}"));
    }
    let _ = std::fs::remove_file(new);
    Ok(())
}

/// Downloads, checks and swaps in a release.
fn install(release: &Release) -> Result<(), String> {
    let agent = agent();
    let asset = release.assets.iter().find(|a| a.name.eq_ignore_ascii_case(ASSET)).ok_or("the release has no hudlight.exe")?;
    let want = expected_sha256(&agent, release, asset)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // Download next to the exe, so the swap is a rename on the same drive and a folder we
    // can't write to fails here, before anything changes.
    let staged = exe.with_extension("exe.new");
    let got = download(&agent, &asset.browser_download_url, &staged).inspect_err(|_| {
        let _ = std::fs::remove_file(&staged);
    })?;
    if got != want {
        let _ = std::fs::remove_file(&staged);
        return Err("the download didn't match its checksum".into());
    }
    swap_in(&staged, &exe)
}

/// Deletes what the last update left behind. Call at start.
pub fn clean_up() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_path(&exe));
        let _ = std::fs::remove_file(exe.with_extension("exe.new"));
    }
}

/// Whether this build should update itself: release builds only, unless `HUDLIGHT_NO_UPDATE`
/// is set. A dev build in `target\` would otherwise overwrite itself with a release.
pub fn enabled_for_this_build() -> bool {
    !cfg!(debug_assertions) && std::env::var_os("HUDLIGHT_NO_UPDATE").is_none()
}

/// What the updater needs from the app.
pub struct Hooks {
    /// The user's "Install updates automatically" setting at start.
    pub auto: bool,
    pub on_state: Box<dyn Fn(UpdateState) + Send>,
    /// Whether the window is on screen; HudLight only installs by itself when it isn't.
    pub window_visible: Box<dyn Fn() -> bool + Send>,
    /// Quit and start again (the new exe); `true` to come back in the tray.
    pub restart: Box<dyn Fn(bool) + Send>,
}

/// The UI's side of the updater thread.
pub struct Updater {
    auto: Arc<AtomicBool>,
    check_now: Arc<AtomicBool>,
    install_now: Arc<AtomicBool>,
}

impl Updater {
    pub fn set_auto(&self, on: bool) {
        self.auto.store(on, Ordering::Release);
    }

    pub fn check_now(&self) {
        self.check_now.store(true, Ordering::Release);
    }

    /// Installs the available update and restarts.
    pub fn install_now(&self) {
        self.install_now.store(true, Ordering::Release);
    }
}

/// Starts the updater thread.
pub fn spawn(hooks: Hooks) -> Updater {
    let updater = Updater {
        auto: Arc::new(AtomicBool::new(hooks.auto)),
        check_now: Arc::new(AtomicBool::new(false)),
        install_now: Arc::new(AtomicBool::new(false)),
    };
    let flags = (updater.auto.clone(), updater.check_now.clone(), updater.install_now.clone());
    let _ = std::thread::Builder::new().name("hudlight-update".into()).spawn(move || run(hooks, &flags.0, &flags.1, &flags.2));
    updater
}

fn run(hooks: Hooks, auto: &AtomicBool, check_now: &AtomicBool, install_now: &AtomicBool) {
    let mut next_check = Instant::now() + FIRST_CHECK;
    let mut pending: Option<Release> = None;
    loop {
        std::thread::sleep(Duration::from_secs(1));

        if let Some(release) = pending.clone() {
            let asked = install_now.swap(false, Ordering::AcqRel);
            let hidden = !(hooks.window_visible)();
            if asked || (hidden && auto.load(Ordering::Acquire)) {
                let version = release.version();
                (hooks.on_state)(UpdateState::Downloading { version: version.clone() });
                match install(&release) {
                    Ok(()) => {
                        (hooks.on_state)(UpdateState::Ready { version });
                        // Let the window show "Restarting…" for a moment.
                        std::thread::sleep(Duration::from_millis(800));
                        (hooks.restart)(hidden);
                        return;
                    }
                    Err(error) => {
                        (hooks.on_state)(UpdateState::Failed { update: release.offer(), error });
                        if !asked {
                            // Don't retry by itself every second; the next check offers it again.
                            pending = None;
                        }
                    }
                }
                continue;
            }
        } else {
            install_now.store(false, Ordering::Release);
        }

        let forced = check_now.swap(false, Ordering::AcqRel);
        if !forced && Instant::now() < next_check {
            continue;
        }
        next_check = Instant::now() + CHECK_EVERY;
        if pending.is_none() {
            (hooks.on_state)(UpdateState::Checking);
        }
        match newer_release(&agent()) {
            Ok(Some(release)) => {
                // Offer it again only when it's new, so a dismissed dialog stays dismissed.
                if pending.as_ref().map(|p| p.tag_name.as_str()) != Some(release.tag_name.as_str()) || forced {
                    (hooks.on_state)(UpdateState::Available(release.offer()));
                }
                pending = Some(release);
            }
            Ok(None) => {
                pending = None;
                (hooks.on_state)(UpdateState::UpToDate);
            }
            Err(e) if pending.is_none() => (hooks.on_state)(UpdateState::Error(e)),
            // Keep offering what we already found.
            Err(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("v0.1.1", "0.1.0"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("v0.0.9", "0.1.0"));
        assert!(!is_newer("nightly", "0.1.0"));
        assert_eq!(parse_version("v1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3.4"), None);
    }

    #[test]
    fn swap_keeps_the_old_exe_aside() {
        let dir = std::env::temp_dir().join(format!("hudlight-swap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("hudlight.exe");
        let new = dir.join("hudlight.exe.new");
        std::fs::write(&exe, b"old").unwrap();
        std::fs::write(&new, b"new").unwrap();
        swap_in(&new, &exe).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert_eq!(std::fs::read(old_path(&exe)).unwrap(), b"old");
        assert!(!new.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_release_offers_its_version_and_notes() {
        let json = r#"{"tag_name":"v1.2.0","html_url":"https://example/r","body":"  Fixes  ","assets":[]}"#;
        let r: Release = serde_json::from_str(json).unwrap();
        assert_eq!(r.offer(), Available { version: "1.2.0".into(), notes: "Fixes".into(), url: "https://example/r".into() });
    }
}
