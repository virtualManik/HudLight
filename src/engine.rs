//! The background side: finds lights, follows Teams presence and drives the light. It owns no
//! UI; it reports what changed through `notify`, which the app turns into `AppHandle::update`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use crate::busylight::{self, Command, DeviceInfo, Lights};
use crate::config::{Config, Effect, Mode, Rgb, Source, Status, StatusStyle};
use crate::teams;

/// What the UI asks for.
#[derive(Debug)]
pub enum Cmd {
    Config(Box<Config>),
    /// Show this style instead of the presence while the user edits it; `None` stops.
    Preview(Option<StatusStyle>),
    /// Cycle through colours for a few seconds.
    Test,
    GraphSignIn,
    GraphSignOut,
    Quit,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GraphState {
    SignedOut,
    Starting,
    Code { user_code: String, url: String },
    SignedIn { name: String },
    Error(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Presence {
    pub status: Status,
    /// The words Teams or Graph used.
    pub raw: String,
    pub source: Source,
    pub at: SystemTime,
}

/// What the light is showing, before brightness and colour correction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Output {
    pub color: Rgb,
    pub effect: Effect,
    /// The status it stands for, when following Teams.
    pub status: Option<Status>,
}

/// What the engine tells the UI.
#[derive(Clone, Debug)]
pub enum Update {
    Devices(Vec<DeviceInfo>),
    Presence(Option<Presence>),
    TeamsRunning(Option<bool>),
    LogPath(Option<PathBuf>),
    Output(Output),
    Graph(GraphState),
}

enum Event {
    Cmd(Cmd),
    Log { found: Option<(Status, String)>, path: Option<PathBuf> },
    TeamsRunning(Option<bool>),
    GraphPresence(Option<(Status, String)>),
    Graph(GraphState),
}

pub struct Engine {
    tx: Sender<Event>,
    thread: Option<JoinHandle<()>>,
}

/// Sends commands from the UI thread.
#[derive(Clone)]
pub struct EngineTx(Sender<Event>);

impl EngineTx {
    pub fn send(&self, cmd: Cmd) {
        let _ = self.0.send(Event::Cmd(cmd));
    }
}

impl std::fmt::Debug for EngineTx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EngineTx")
    }
}

impl Engine {
    pub fn start(config: Config, notify: impl Fn(Update) + Send + 'static) -> Engine {
        let (tx, rx) = mpsc::channel();
        let use_log = Arc::new(AtomicBool::new(config.source == Source::TeamsClient));
        spawn_log_thread(tx.clone(), use_log.clone());
        let graph = spawn_graph_thread(tx.clone(), &config);
        let thread = std::thread::Builder::new()
            .name("hudlight-engine".into())
            .spawn(move || Core::new(config, Box::new(notify), use_log, graph).run(rx))
            .ok();
        Engine { tx, thread }
    }

    pub fn sender(&self) -> EngineTx {
        EngineTx(self.tx.clone())
    }

    /// Turns the light off and waits (briefly) for the engine to finish.
    pub fn stop(mut self) {
        let _ = self.tx.send(Event::Cmd(Cmd::Quit));
        if let Some(t) = self.thread.take() {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !t.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if t.is_finished() {
                let _ = t.join();
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------

fn spawn_log_thread(tx: Sender<Event>, enabled: Arc<AtomicBool>) {
    let _ = std::thread::Builder::new().name("teams-log".into()).spawn(move || {
        let mut watcher = teams::LogWatcher::new();
        let mut last_path = None;
        let mut last_running_check: Option<Instant> = None;
        loop {
            if enabled.load(Ordering::Relaxed) {
                let found = watcher.poll();
                let path = watcher.path().map(PathBuf::from);
                if found.is_some() || path != last_path {
                    last_path = path.clone();
                    if tx.send(Event::Log { found, path }).is_err() {
                        return;
                    }
                }
                if last_running_check.is_none_or(|t| t.elapsed() > Duration::from_secs(10)) {
                    last_running_check = Some(Instant::now());
                    if tx.send(Event::TeamsRunning(teams::teams_running())).is_err() {
                        return;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(750));
        }
    });
}

enum GraphCmd {
    Config { client_id: String, tenant: String, poll: u32, enabled: bool },
    SignIn,
    SignOut,
}

enum Auth {
    SignedOut,
    Waiting { code: teams::DeviceCode, next: Instant, until: Instant },
    /// `errored` is set while the UI shows a request error, so the next success clears it.
    SignedIn { name: String, access: String, refresh: Option<String>, expires: Instant, next: Instant, errored: bool, failures: u32 },
}

/// A refusal no retry will fix (revoked or expired grant, wrong client); anything else, like no
/// network yet at login, is worth retrying.
fn is_permanent(error: &str) -> bool {
    ["invalid_grant", "invalid_client", "unauthorized_client", "interaction_required", "(400)", "(401)"].iter().any(|k| error.contains(k))
}

fn spawn_graph_thread(events: Sender<Event>, config: &Config) -> Sender<GraphCmd> {
    let (tx, rx) = mpsc::channel::<GraphCmd>();
    let mut client_id = config.graph_client_id.clone();
    let mut tenant = config.graph_tenant.clone();
    let mut poll = Duration::from_secs(u64::from(config.graph_poll_seconds));
    let mut enabled = config.source == Source::Graph;
    let _ = std::thread::Builder::new().name("teams-graph".into()).spawn(move || {
        let send = |e: Event| events.send(e).is_ok();
        // Resume a saved sign-in without asking again.
        let mut auth = Auth::SignedOut;
        let mut saved = teams::load_refresh_token();
        let mut announced = false;
        // Restoring a saved sign-in retries with backoff while the network isn't up yet.
        let mut retry_at: Option<Instant> = None;
        let mut backoff = Duration::from_secs(15);
        loop {
            let restore_due = retry_at.is_none_or(|t| Instant::now() >= t);
            if saved.is_some() && enabled && !client_id.is_empty() && matches!(auth, Auth::SignedOut) && restore_due {
                let token = saved.take().unwrap_or_default();
                match teams::refresh(&client_id, &tenant, &token) {
                    Ok(t) => {
                        retry_at = None;
                        backoff = Duration::from_secs(15);
                        teams::save_refresh_token(t.refresh_token.as_deref().or(Some(&token)));
                        let name = teams::get_me(&t.access_token).unwrap_or_else(|| "Signed in".into());
                        if !send(Event::Graph(GraphState::SignedIn { name: name.clone() })) {
                            return;
                        }
                        auth = Auth::SignedIn {
                            name,
                            refresh: t.refresh_token.or(Some(token)),
                            expires: Instant::now() + Duration::from_secs(t.expires_in.saturating_sub(120)),
                            access: t.access_token,
                            next: Instant::now(),
                            errored: false,
                            failures: 0,
                        };
                    }
                    Err(e) if is_permanent(&e) => {
                        retry_at = None;
                        teams::save_refresh_token(None);
                        if !send(Event::Graph(GraphState::Error(format!("Sign in again: {e}")))) {
                            return;
                        }
                    }
                    Err(e) => {
                        saved = Some(token);
                        retry_at = Some(Instant::now() + backoff);
                        let secs = backoff.as_secs();
                        backoff = (backoff * 2).min(Duration::from_secs(300));
                        if !send(Event::Graph(GraphState::Error(format!("Couldn't restore the sign-in, retrying in {secs} s: {e}")))) {
                            return;
                        }
                    }
                }
            } else if !announced && matches!(auth, Auth::SignedOut) {
                announced = true;
                let state = if saved.is_some() { GraphState::SignedIn { name: "Saved sign-in".into() } } else { GraphState::SignedOut };
                if !send(Event::Graph(state)) {
                    return;
                }
            }

            let wait = match &auth {
                Auth::SignedOut => match retry_at {
                    Some(t) if saved.is_some() && enabled => t.saturating_duration_since(Instant::now()),
                    _ => Duration::from_secs(3600),
                },
                Auth::Waiting { next, .. } | Auth::SignedIn { next, .. } => next.saturating_duration_since(Instant::now()),
            };
            match rx.recv_timeout(wait) {
                Ok(GraphCmd::Config { client_id: c, tenant: t, poll: p, enabled: e }) => {
                    if c != client_id || t != tenant {
                        auth = Auth::SignedOut;
                    }
                    client_id = c;
                    tenant = t;
                    poll = Duration::from_secs(u64::from(p));
                    enabled = e;
                    continue;
                }
                Ok(GraphCmd::SignIn) => {
                    retry_at = None;
                    if client_id.trim().is_empty() {
                        send(Event::Graph(GraphState::Error("Enter your app registration's client ID first.".into())));
                        continue;
                    }
                    send(Event::Graph(GraphState::Starting));
                    match teams::start_device_code(client_id.trim(), &tenant) {
                        Ok(code) => {
                            send(Event::Graph(GraphState::Code { user_code: code.user_code.clone(), url: code.verification_uri.clone() }));
                            auth = Auth::Waiting {
                                next: Instant::now() + Duration::from_secs(code.interval),
                                until: Instant::now() + Duration::from_secs(code.expires_in),
                                code,
                            };
                        }
                        Err(e) => {
                            send(Event::Graph(GraphState::Error(e)));
                        }
                    }
                    continue;
                }
                Ok(GraphCmd::SignOut) => {
                    auth = Auth::SignedOut;
                    saved = None;
                    retry_at = None;
                    teams::save_refresh_token(None);
                    send(Event::Graph(GraphState::SignedOut));
                    send(Event::GraphPresence(None));
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {}
            }

            // A deadline came due.
            match &mut auth {
                Auth::SignedOut => {}
                Auth::Waiting { code, next, until } => {
                    if Instant::now() > *until {
                        auth = Auth::SignedOut;
                        send(Event::Graph(GraphState::Error("The sign-in code expired. Try again.".into())));
                        continue;
                    }
                    match teams::poll_device_code(client_id.trim(), &tenant, &code.device_code) {
                        teams::PollResult::Pending => *next = Instant::now() + Duration::from_secs(code.interval),
                        teams::PollResult::SlowDown => {
                            code.interval += 5;
                            *next = Instant::now() + Duration::from_secs(code.interval);
                        }
                        teams::PollResult::Done(t) => {
                            teams::save_refresh_token(t.refresh_token.as_deref());
                            let name = teams::get_me(&t.access_token).unwrap_or_else(|| "Signed in".into());
                            send(Event::Graph(GraphState::SignedIn { name: name.clone() }));
                            auth = Auth::SignedIn {
                                name,
                                expires: Instant::now() + Duration::from_secs(t.expires_in.saturating_sub(120)),
                                access: t.access_token,
                                refresh: t.refresh_token,
                                next: Instant::now(),
                                errored: false,
                                failures: 0,
                            };
                        }
                        teams::PollResult::Failed(e) => {
                            auth = Auth::SignedOut;
                            send(Event::Graph(GraphState::Error(e)));
                        }
                    }
                }
                Auth::SignedIn { name, access, refresh, expires, next, errored, failures } => {
                    *next = Instant::now() + poll;
                    if !enabled {
                        continue;
                    }
                    if Instant::now() > *expires {
                        let Some(r) = refresh.clone() else {
                            auth = Auth::SignedOut;
                            send(Event::Graph(GraphState::Error("Signed out: the session ended.".into())));
                            continue;
                        };
                        match teams::refresh(client_id.trim(), &tenant, &r) {
                            Ok(t) => {
                                if t.refresh_token.is_some() {
                                    teams::save_refresh_token(t.refresh_token.as_deref());
                                    *refresh = t.refresh_token;
                                }
                                *access = t.access_token;
                                *expires = Instant::now() + Duration::from_secs(t.expires_in.saturating_sub(120));
                            }
                            Err(e) if is_permanent(&e) => {
                                auth = Auth::SignedOut;
                                teams::save_refresh_token(None);
                                send(Event::Graph(GraphState::Error(format!("Signed out, sign in again: {e}"))));
                                send(Event::GraphPresence(None));
                                continue;
                            }
                            Err(e) => {
                                // Network trouble: try again next round.
                                *errored = true;
                                send(Event::Graph(GraphState::Error(format!("Couldn't refresh the sign-in: {e}"))));
                                continue;
                            }
                        }
                    }
                    match teams::get_presence(access) {
                        Ok(p) => {
                            *failures = 0;
                            if std::mem::take(errored) {
                                send(Event::Graph(GraphState::SignedIn { name: name.clone() }));
                            }
                            send(Event::GraphPresence(Some(p)));
                        }
                        Err(teams::GraphError::Unauthorized) => *expires = Instant::now(),
                        Err(teams::GraphError::Other(e)) => {
                            *errored = true;
                            *failures += 1;
                            if *failures == 3 {
                                send(Event::GraphPresence(None));
                            }
                            send(Event::Graph(GraphState::Error(format!("Presence request failed: {e}"))));
                        }
                    }
                }
            }
        }
    });
    tx
}

// ---------------------------------------------------------------------------------------------

/// Turns the effective style into a device command at time `t` (seconds), applying brightness
/// and colour correction. Pulse is drawn here; blink runs on the light's own clock.
pub fn command_for(color: Rgb, effect: Effect, cfg: &Config, t: f32) -> Command {
    if effect == Effect::Off || color.is_off() {
        return Command::off();
    }
    let correct = |c: Rgb| if cfg.gamma_correction { busylight::degamma(c) } else { c };
    let base = color.scale(cfg.brightness);
    match effect {
        Effect::Solid | Effect::Off => Command::solid(correct(base)),
        Effect::Blink => {
            let half = ((cfg.blink_period * 5.0).round() as u8).max(1);
            Command { color: correct(base), on_time: half, off_time: half }
        }
        Effect::Pulse => {
            let phase = (t / cfg.pulse_period.max(0.1)).fract();
            let wave = 0.5 - 0.5 * (phase * std::f32::consts::TAU).cos();
            Command::solid(correct(base.scale(0.06 + 0.94 * wave)))
        }
    }
}

/// A blinking command drawn by the host at time `t`, for lights without their own blink timer.
fn soft_blink(cmd: Command, t: f32) -> Command {
    if cmd.on_time == 0 && cmd.off_time == 0 {
        return cmd;
    }
    let on = f32::from(cmd.on_time) / 10.0;
    let period = on + f32::from(cmd.off_time) / 10.0;
    if t % period < on { Command::solid(cmd.color) } else { Command::off() }
}

fn hue(h: f32) -> Rgb {
    let f = |n: f32| {
        let k = (n + h * 6.0) % 6.0;
        let v = 1.0 - (k.min(4.0 - k).clamp(0.0, 1.0));
        (v * 255.0).round() as u8
    };
    Rgb(f(5.0), f(3.0), f(1.0))
}

struct Core {
    cfg: Config,
    notify: Box<dyn Fn(Update) + Send>,
    use_log: Arc<AtomicBool>,
    graph: Sender<GraphCmd>,
    lights: Lights,
    log_presence: Option<Presence>,
    graph_presence: Option<Presence>,
    teams_running: Option<bool>,
    /// Teams briefly logs `PresenceUnknown` (sign-in refreshes, reconnects); only a lasting one
    /// is shown.
    unknown_since: Option<Instant>,
    preview: Option<StatusStyle>,
    test_until: Option<Instant>,
    started: Instant,
    last_cmd: Option<Command>,
    last_keep_alive: Instant,
    last_scan: Option<Instant>,
    last_output: Option<Output>,
    last_presence: Option<Presence>,
}

impl Core {
    fn new(cfg: Config, notify: Box<dyn Fn(Update) + Send>, use_log: Arc<AtomicBool>, graph: Sender<GraphCmd>) -> Self {
        Core {
            cfg,
            notify,
            use_log,
            graph,
            lights: Lights::new(),
            log_presence: None,
            graph_presence: None,
            teams_running: None,
            unknown_since: None,
            preview: None,
            test_until: None,
            started: Instant::now(),
            last_cmd: None,
            last_keep_alive: Instant::now(),
            last_scan: None,
            last_output: None,
            last_presence: None,
        }
    }

    fn presence(&self) -> Option<Presence> {
        match self.cfg.source {
            Source::TeamsClient => {
                if self.teams_running == Some(false) {
                    return Some(Presence {
                        status: Status::Offline,
                        raw: "Teams isn't running".into(),
                        source: Source::TeamsClient,
                        at: self.log_presence.as_ref().map_or_else(SystemTime::now, |p| p.at),
                    });
                }
                self.log_presence.clone()
            }
            Source::Graph => self.graph_presence.clone(),
        }
    }

    fn output(&self) -> Output {
        if let Some(p) = self.preview {
            return Output { color: p.color, effect: p.effect, status: None };
        }
        match self.cfg.mode {
            Mode::Off => Output { color: Rgb::OFF, effect: Effect::Off, status: None },
            Mode::Manual => Output { color: self.cfg.manual.color, effect: self.cfg.manual.effect, status: None },
            Mode::Teams => {
                let status = self.presence().map_or(Status::Unknown, |p| p.status);
                let st = self.cfg.style(status);
                Output { color: st.color, effect: st.effect, status: Some(status) }
            }
        }
    }

    fn handle(&mut self, e: Event) -> bool {
        match e {
            Event::Cmd(Cmd::Quit) => return false,
            Event::Cmd(Cmd::Config(c)) => {
                let graph_changed = c.graph_client_id != self.cfg.graph_client_id
                    || c.graph_tenant != self.cfg.graph_tenant
                    || c.graph_poll_seconds != self.cfg.graph_poll_seconds
                    || c.source != self.cfg.source;
                self.cfg = *c;
                self.use_log.store(self.cfg.source == Source::TeamsClient, Ordering::Relaxed);
                if graph_changed {
                    let _ = self.graph.send(GraphCmd::Config {
                        client_id: self.cfg.graph_client_id.trim().to_owned(),
                        tenant: self.cfg.graph_tenant.clone(),
                        poll: self.cfg.graph_poll_seconds,
                        enabled: self.cfg.source == Source::Graph,
                    });
                }
                // Brightness or gamma may have changed what the same colour sends.
                self.last_cmd = None;
            }
            Event::Cmd(Cmd::Preview(p)) => self.preview = p,
            Event::Cmd(Cmd::Test) => self.test_until = Some(Instant::now() + Duration::from_secs(4)),
            Event::Cmd(Cmd::GraphSignIn) => {
                let _ = self.graph.send(GraphCmd::SignIn);
            }
            Event::Cmd(Cmd::GraphSignOut) => {
                let _ = self.graph.send(GraphCmd::SignOut);
            }
            Event::Log { found, path } => {
                (self.notify)(Update::LogPath(path));
                if let Some((status, raw)) = found {
                    let known_before = self.log_presence.as_ref().is_some_and(|p| p.status != Status::Unknown);
                    if status == Status::Unknown && known_before {
                        self.unknown_since.get_or_insert_with(Instant::now);
                    } else {
                        self.unknown_since = None;
                        self.log_presence = Some(Presence { status, raw, source: Source::TeamsClient, at: SystemTime::now() });
                    }
                }
            }
            Event::TeamsRunning(r) => {
                if r != self.teams_running {
                    self.teams_running = r;
                    (self.notify)(Update::TeamsRunning(r));
                }
            }
            Event::GraphPresence(p) => {
                self.graph_presence = p.map(|(status, raw)| Presence { status, raw, source: Source::Graph, at: SystemTime::now() });
            }
            Event::Graph(g) => (self.notify)(Update::Graph(g)),
        }
        true
    }

    fn tick(&mut self) {
        let now = Instant::now();
        if self.unknown_since.is_some_and(|t| now - t > Duration::from_secs(15)) {
            self.unknown_since = None;
            self.log_presence = Some(Presence { status: Status::Unknown, raw: "PresenceUnknown".into(), source: Source::TeamsClient, at: SystemTime::now() });
        }
        let scan_every = if self.lights.is_empty() { Duration::from_secs(2) } else { Duration::from_secs(6) };
        if self.last_scan.is_none_or(|t| now - t > scan_every) {
            self.last_scan = Some(now);
            if self.lights.scan() {
                self.last_cmd = None;
                (self.notify)(Update::Devices(self.lights.infos()));
            }
        }

        let presence = self.presence();
        if presence.as_ref().map(|p| (p.status, &p.raw, p.source)) != self.last_presence.as_ref().map(|p| (p.status, &p.raw, p.source)) {
            self.last_presence = presence.clone();
            (self.notify)(Update::Presence(presence));
        }

        let t = (now - self.started).as_secs_f32();
        let (out, cmd) = match self.test_until {
            Some(until) if now < until => {
                let c = hue((t * 0.5).fract());
                (Output { color: c, effect: Effect::Solid, status: None }, Command::solid(c.scale(self.cfg.brightness)))
            }
            _ => {
                self.test_until = None;
                let out = self.output();
                (out, command_for(out.color, out.effect, &self.cfg, t))
            }
        };
        if self.last_output != Some(out) && self.test_until.is_none() {
            self.last_output = Some(out);
            (self.notify)(Update::Output(out));
        }

        if self.lights.is_empty() {
            return;
        }
        let cmd = if self.lights.hardware_blink() { cmd } else { soft_blink(cmd, t) };
        let mut dropped = false;
        if self.last_cmd != Some(cmd) {
            dropped |= self.lights.send(cmd, self.cfg.pwm_100);
            self.last_cmd = Some(cmd);
            self.last_keep_alive = now;
        } else {
            let keep_alive = now - self.last_keep_alive > Duration::from_secs(10);
            if keep_alive {
                self.last_keep_alive = now;
            }
            dropped |= self.lights.service(keep_alive);
        }
        if dropped {
            self.last_cmd = None;
            (self.notify)(Update::Devices(self.lights.infos()));
        }
    }

    fn run(mut self, rx: Receiver<Event>) {
        loop {
            // Pulsing needs smooth frames, and so do host-drawn blinks and VT commands waiting
            // out their gap; otherwise wake rarely.
            let effect = self.output().effect;
            let animating = self.test_until.is_some()
                || effect == Effect::Pulse
                || (effect == Effect::Blink && !self.lights.hardware_blink());
            let wait = if animating { Duration::from_millis(40) } else { Duration::from_millis(250) };
            match rx.recv_timeout(wait) {
                Ok(e) => {
                    if !self.handle(e) {
                        break;
                    }
                    while let Ok(e) = rx.try_recv() {
                        if !self.handle(e) {
                            self.shutdown();
                            return;
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.tick();
        }
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.lights.turn_off();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blink_runs_on_the_lights_clock() {
        let cfg = Config { gamma_correction: false, blink_period: 1.0, ..Config::default() };
        let c = command_for(Rgb(255, 0, 0), Effect::Blink, &cfg, 0.0);
        assert_eq!((c.on_time, c.off_time), (5, 5));
    }

    #[test]
    fn soft_blink_alternates_on_and_off() {
        let cfg = Config { gamma_correction: false, blink_period: 1.0, ..Config::default() };
        let c = command_for(Rgb(255, 0, 0), Effect::Blink, &cfg, 0.0);
        assert_eq!(soft_blink(c, 0.2), Command::solid(Rgb(255, 0, 0)));
        assert_eq!(soft_blink(c, 0.7), Command::off());
        assert_eq!(soft_blink(c, 1.2), Command::solid(Rgb(255, 0, 0)));
        let solid = Command::solid(Rgb(1, 2, 3));
        assert_eq!(soft_blink(solid, 0.7), solid);
    }

    #[test]
    fn pulse_breathes_between_dim_and_full() {
        let cfg = Config { gamma_correction: false, pulse_period: 2.0, ..Config::default() };
        let low = command_for(Rgb(200, 0, 0), Effect::Pulse, &cfg, 0.0).color.0;
        let high = command_for(Rgb(200, 0, 0), Effect::Pulse, &cfg, 1.0).color.0;
        assert!(low < 20 && high == 200, "{low} {high}");
    }

    #[test]
    fn brightness_scales_and_off_is_dark() {
        let cfg = Config { gamma_correction: false, brightness: 0.5, ..Config::default() };
        assert_eq!(command_for(Rgb(200, 100, 0), Effect::Solid, &cfg, 0.0).color, Rgb(100, 50, 0));
        assert_eq!(command_for(Rgb(200, 100, 0), Effect::Off, &cfg, 0.0), Command::off());
    }

    #[test]
    fn only_refusals_are_permanent() {
        assert!(is_permanent("invalid_grant (400): AADSTS70000"));
        assert!(!is_permanent("Dns Failed: resolve dns name 'login.microsoftonline.com:443'"));
        assert!(!is_permanent("HTTP 503"));
    }

    #[test]
    fn hue_wheel_hits_primaries() {
        assert_eq!(hue(0.0), Rgb(255, 0, 0));
        assert_eq!(hue(1.0 / 3.0), Rgb(0, 255, 0));
    }
}
