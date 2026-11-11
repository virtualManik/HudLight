//! The window: app state, what the engine reports, the tray, and the root view.

mod pages;
mod widgets;

use std::path::PathBuf;
use std::time::Duration;

use rune::{AppHandle, Cx, El, ScrollSpec, TextEdit, Tray, TrayEvent, TrayIcon, TrayItem, TraySpec, Tx, col, row};

use crate::busylight::DeviceInfo;
use crate::config::{Appearance, Config, Mode, Rgb, Status, StatusStyle};
use crate::engine::{Cmd, EngineTx, GraphState, Output, Presence, Update};
use crate::update::{Available, UpdateState, Updater};
use widgets::Pal;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Light,
    Teams,
    Colors,
    Settings,
    Help,
}

/// Which colour the hex field edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Status(Status),
    Manual,
}

pub struct Ui {
    pub page: Page,
    pub cfg: Config,
    pub devices: Vec<DeviceInfo>,
    pub presence: Option<Presence>,
    pub teams_running: Option<bool>,
    pub log_path: Option<PathBuf>,
    pub output: Output,
    pub graph: GraphState,
    /// The status whose colour editor is open on the Colors page.
    pub editing: Option<Status>,
    pub hex_edit: TextEdit,
    hex_target: Option<Target>,
    hex_seen: String,
    /// The colour the hex field last showed or applied; anything else changing it refreshes the field.
    hex_color: Rgb,
    pub client_id: TextEdit,
    pub tenant: TextEdit,
    pub message: Option<String>,
    /// The background updater; `None` in dev builds, snapshots and tests.
    pub updater: Option<Updater>,
    /// What the updater last reported.
    pub update: Option<UpdateState>,
    /// Whether the update dialog is open.
    pub update_dialog: bool,
    /// The version "Later" was pressed for, so finding it again doesn't reopen the dialog.
    update_dismissed: Option<String>,
    engine: Option<EngineTx>,
    handle: Option<AppHandle<Ui>>,
    tray: Option<Tray>,
    tray_color: Option<Rgb>,
    /// A new tray colour waiting to hold still before the icon is rebuilt.
    tray_pending: Option<(Rgb, std::time::Instant)>,
    preview_sent: Option<StatusStyle>,
    save_due: Option<f64>,
    age_tick: f64,
}

impl Ui {
    pub fn new(cfg: Config, engine: Option<EngineTx>, handle: Option<AppHandle<Ui>>) -> Ui {
        Ui {
            page: Page::Light,
            client_id: TextEdit::new(cfg.graph_client_id.clone()),
            tenant: TextEdit::new(cfg.graph_tenant.clone()),
            cfg,
            devices: Vec::new(),
            presence: None,
            teams_running: None,
            log_path: None,
            output: Output { color: Rgb::OFF, effect: crate::config::Effect::Off, status: None },
            graph: GraphState::SignedOut,
            editing: None,
            hex_edit: TextEdit::new(""),
            hex_target: None,
            hex_seen: String::new(),
            hex_color: Rgb::OFF,
            message: None,
            updater: None,
            update: None,
            update_dialog: false,
            update_dismissed: None,
            engine,
            handle,
            tray: None,
            tray_color: None,
            tray_pending: None,
            preview_sent: None,
            save_due: None,
            age_tick: 0.0,
        }
    }

    /// For headless snapshots and tests: no engine, no tray, nothing on disk.
    pub fn demo() -> Ui {
        let mut s = Ui::new(Config::default(), None, None);
        s.devices = vec![DeviceInfo {
            product: "VT DND Omega".into(),
            manufacturer: "Plenom A/S".into(),
            model: "Busylight Omega",
            vid: 0x27BB,
            pid: 0x3BCD,
            release: 0x65CD,
            serial: String::new(),
        }];
        s.presence = Some(Presence {
            status: Status::Busy,
            raw: "Busy".into(),
            source: crate::config::Source::TeamsClient,
            at: std::time::SystemTime::now(),
        });
        s.teams_running = Some(true);
        s.log_path = Some(PathBuf::from(
            r"C:\Users\you\AppData\Local\Packages\MSTeams_8wekyb3d8bbwe\LocalCache\Microsoft\MSTeams\Logs\MSTeams_2026-11-10_09-14-02.00.log",
        ));
        let st = s.cfg.style(Status::Busy);
        s.output = Output { color: st.color, effect: st.effect, status: Some(Status::Busy) };
        s
    }

    /// False in headless snapshots and tests, which must not touch the registry, files or Explorer.
    pub fn live(&self) -> bool {
        self.engine.is_some()
    }

    /// Writes pending settings now (quitting can't wait for the debounce).
    pub fn flush_save(&mut self) {
        if self.save_due.take().is_some() && self.live() {
            self.cfg.save();
        }
    }

    /// Ends the colour editor's preview, so Teams (or the chosen mode) drives the light again.
    pub fn stop_preview(&mut self) {
        self.editing = None;
        if self.preview_sent.take().is_some() {
            self.send(Cmd::Preview(None));
        }
    }

    fn send(&self, cmd: Cmd) {
        if let Some(e) = &self.engine {
            e.send(cmd);
        }
    }

    /// Settings changed: tell the engine now, save shortly.
    pub fn commit(&mut self) {
        self.send(Cmd::Config(Box::new(self.cfg.clone())));
        if self.save_due.is_none() {
            self.save_due = Some(f64::NEG_INFINITY);
        }
    }

    pub fn set_mode(&mut self, m: Mode) {
        self.stop_preview();
        self.cfg.mode = m;
        self.commit();
        self.rebuild_tray_menu();
    }

    pub fn color_of(&self, t: Target) -> Rgb {
        match t {
            Target::Status(st) => self.cfg.style(st).color,
            Target::Manual => self.cfg.manual.color,
        }
    }

    pub fn set_color(&mut self, t: Target, c: Rgb) {
        match t {
            Target::Status(st) => self.cfg.style_mut(st).color = c,
            Target::Manual => self.cfg.manual.color = c,
        }
        if self.hex_target == Some(t) {
            self.hex_seen = c.hex();
            self.hex_color = c;
            self.hex_edit.replace_text(c.hex());
        }
        self.commit();
    }

    pub fn set_effect(&mut self, t: Target, e: crate::config::Effect) {
        match t {
            Target::Status(st) => self.cfg.style_mut(st).effect = e,
            Target::Manual => self.cfg.manual.effect = e,
        }
        self.commit();
    }

    pub fn test_light(&mut self) {
        self.send(Cmd::Test);
    }

    pub fn graph_sign_in(&mut self) {
        self.apply_graph_fields();
        self.send(Cmd::GraphSignIn);
    }

    pub fn graph_sign_out(&mut self) {
        self.send(Cmd::GraphSignOut);
    }

    pub fn set_auto_update(&mut self, on: bool) {
        self.cfg.auto_update = on;
        if let Some(u) = &self.updater {
            u.set_auto(on);
        }
        self.commit();
    }

    pub fn check_for_update(&mut self) {
        if let Some(u) = &self.updater {
            self.update = Some(UpdateState::Checking);
            u.check_now();
        }
    }

    pub fn set_update_state(&mut self, state: UpdateState) {
        let offered_before = self.available_update().map(|a| a.version.clone());
        match &state {
            // A newly found update opens the dialog, unless it was put off already.
            UpdateState::Available(a) if self.update_dismissed.as_deref() != Some(a.version.as_str()) => self.update_dialog = true,
            // HudLight is about to restart into the new version.
            UpdateState::Ready { .. } => {
                self.stop_preview();
                self.flush_save();
            }
            _ => {}
        }
        self.update = Some(state);
        if self.available_update().map(|a| a.version.clone()) != offered_before {
            self.rebuild_tray_menu();
        }
    }

    /// The update on offer, if any.
    pub fn available_update(&self) -> Option<&Available> {
        self.update.as_ref().and_then(UpdateState::available)
    }

    /// Whether an update is downloading or installed and about to restart.
    pub fn updating(&self) -> bool {
        matches!(self.update, Some(UpdateState::Downloading { .. } | UpdateState::Ready { .. }))
    }

    /// "Update now": download, install and restart.
    pub fn install_update(&mut self) {
        let Some(version) = self.available_update().map(|a| a.version.clone()) else { return };
        if let Some(u) = &self.updater {
            u.install_now();
            self.update = Some(UpdateState::Downloading { version });
        }
    }

    /// "Later": close the dialog; the sidebar keeps offering the update.
    pub fn dismiss_update(&mut self) {
        self.update_dialog = false;
        self.update_dismissed = self.available_update().map(|a| a.version.clone());
    }

    pub fn open_update_dialog(&mut self) {
        if self.available_update().is_some() || self.updating() {
            self.update_dialog = true;
        }
    }

    pub fn apply_graph_fields(&mut self) {
        let id = self.client_id.text.trim().to_owned();
        let tenant = self.tenant.text.trim().to_owned();
        let tenant = if tenant.is_empty() { "organizations".to_owned() } else { tenant };
        if id != self.cfg.graph_client_id || tenant != self.cfg.graph_tenant {
            self.cfg.graph_client_id = id;
            self.cfg.graph_tenant = tenant;
            self.commit();
        }
    }

    pub fn set_autostart(&mut self, on: bool) {
        let result = if self.live() { crate::autostart::set(on) } else { Ok(()) };
        match result {
            Ok(()) => {
                self.cfg.start_with_windows = on;
                self.message = None;
                self.commit();
            }
            Err(e) => self.message = Some(format!("Couldn't change start with Windows: {e}")),
        }
    }

    pub fn apply(&mut self, u: Update) {
        match u {
            Update::Devices(d) => {
                self.devices = d;
                self.refresh_tray();
            }
            Update::Presence(p) => self.presence = p,
            Update::TeamsRunning(r) => self.teams_running = r,
            Update::LogPath(p) => self.log_path = p,
            Update::Output(o) => {
                self.output = o;
                self.refresh_tray();
            }
            Update::Graph(g) => self.graph = g,
        }
    }

    /// The colour the open editor is changing, if any.
    fn active_target(&self) -> Option<Target> {
        match self.page {
            Page::Colors => self.editing.map(Target::Status),
            Page::Light if self.cfg.mode == Mode::Manual => Some(Target::Manual),
            _ => None,
        }
    }

    // --- Tray ------------------------------------------------------------------------------

    fn tray_menu(&self) -> Vec<TrayItem> {
        let mark = |m: Mode, label: &str| if self.cfg.mode == m { format!("✓ {label}") } else { format!("    {label}") };
        let mut items = vec![
            TrayItem::action("open", "Open HudLight"),
            TrayItem::Separator,
            TrayItem::action("mode-teams", mark(Mode::Teams, "Follow Teams status")),
            TrayItem::action("mode-manual", mark(Mode::Manual, "Manual colour")),
            TrayItem::action("mode-off", mark(Mode::Off, "Light off")),
            TrayItem::Separator,
        ];
        if let Some(a) = self.available_update() {
            items.push(TrayItem::action("show-update", format!("Update to {}…", a.version)));
        }
        items.push(TrayItem::action("quit", "Quit"));
        items
    }

    fn rebuild_tray_menu(&self) {
        if let Some(t) = &self.tray {
            t.set_menu(self.tray_menu());
        }
    }

    fn tooltip(&self) -> String {
        let what = match (self.cfg.mode, self.output.status) {
            (Mode::Off, _) => "Light off".to_owned(),
            (Mode::Manual, _) => format!("Manual colour {}", self.cfg.manual.color.hex()),
            (Mode::Teams, Some(st)) => format!("Teams: {}", st.label()),
            (Mode::Teams, None) => "Following Teams".to_owned(),
        };
        let dev = if self.devices.is_empty() { "no light connected" } else { "light connected" };
        format!("HudLight — {what} ({dev})")
    }

    /// The tray icon shows the light's colour; its icon is fixed once made, so a new colour
    /// makes a new tray (rare: only when the status changes).
    pub fn refresh_tray(&mut self) {
        let Some(handle) = self.handle.clone() else { return };
        let color = if self.output.effect == crate::config::Effect::Off { Rgb::OFF } else { self.output.color };
        if self.tray.is_some() && self.tray_color == Some(color) {
            self.tray_pending = None;
            if let Some(t) = &self.tray {
                t.set_tooltip(self.tooltip());
            }
            return;
        }
        if self.tray.is_some() {
            // Rebuilding the icon flickers and closes an open tray menu: wait until the colour
            // has held for a second (poll finishes it).
            match self.tray_pending {
                Some((c, since)) if c == color && since.elapsed() >= Duration::from_secs(1) => {}
                Some((c, _)) if c == color => return,
                _ => {
                    self.tray_pending = Some((color, std::time::Instant::now()));
                    if let Some(t) = &self.tray {
                        t.set_tooltip(self.tooltip());
                    }
                    return;
                }
            }
        }
        self.tray_pending = None;
        let spec = TraySpec::new("hudlight", "HudLight", tray_icon(color)).tooltip(self.tooltip()).menu(self.tray_menu());
        let h = handle.clone();
        match Tray::new(spec, move |e| on_tray(&h, e)) {
            Ok(t) => {
                self.tray = Some(t);
                self.tray_color = Some(color);
            }
            Err(e) => eprintln!("hudlight: no tray icon: {e}"),
        }
    }
}

fn on_tray(h: &AppHandle<Ui>, e: TrayEvent) {
    match e {
        TrayEvent::Activate { .. } => h.open(),
        TrayEvent::Menu(id) => match id.as_str() {
            "open" => h.open(),
            "mode-teams" => h.update(|s, _| s.set_mode(Mode::Teams)),
            "mode-manual" => h.update(|s, _| s.set_mode(Mode::Manual)),
            "mode-off" => h.update(|s, _| s.set_mode(Mode::Off)),
            "show-update" => {
                h.update(|s, _| s.open_update_dialog());
                h.open();
            }
            "quit" => h.update(|s, tx| {
                s.stop_preview();
                s.flush_save();
                tx.quit();
            }),
            _ => {}
        },
        _ => {}
    }
}

/// A 32×32 disc in `color`, or a grey ring when the light is off.
pub fn tray_icon(color: Rgb) -> TrayIcon {
    let n = 32u32;
    let mut px = vec![0u8; (n * n * 4) as usize];
    let (cx, r) = (15.5f32, 13.5f32);
    for y in 0..n {
        for x in 0..n {
            let d = ((x as f32 - cx).powi(2) + (y as f32 - cx).powi(2)).sqrt();
            let cover = (r + 0.5 - d).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let i = ((y * n + x) * 4) as usize;
            let (rgb, a) = if color.is_off() {
                // A ring, 2.5 px wide.
                let ring = (d - (r - 2.5)).clamp(0.0, 1.0) * cover;
                ([150u8, 150, 150], ring)
            } else {
                let edge = d > r - 1.5;
                let c = if edge { color.scale(0.7) } else { color };
                ([c.0, c.1, c.2], cover)
            };
            px[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], (a * 255.0).round() as u8]);
        }
    }
    TrayIcon::rgba(n, n, px)
}

// --- Hooks ---------------------------------------------------------------------------------

pub fn poll(s: &mut Ui, now: f64, tx: &mut Tx) -> bool {
    let mut changed = false;

    // Keep the hex field on whichever colour is being edited, and apply what's typed in it.
    let target = s.active_target();
    if target != s.hex_target {
        s.hex_target = target;
        let color = target.map_or(Rgb::OFF, |t| s.color_of(t));
        let hex = target.map(|_| color.hex()).unwrap_or_default();
        s.hex_seen = hex.clone();
        s.hex_color = color;
        s.hex_edit = TextEdit::new(hex);
        changed = true;
    } else if let Some(t) = target {
        let color = s.color_of(t);
        if s.hex_edit.text != s.hex_seen {
            s.hex_seen = s.hex_edit.text.clone();
            if let Some(c) = Rgb::parse(&s.hex_seen) {
                s.hex_color = c;
                if c != color {
                    match t {
                        Target::Status(st) => s.cfg.style_mut(st).color = c,
                        Target::Manual => s.cfg.manual.color = c,
                    }
                    s.commit();
                    changed = true;
                }
            }
        } else if color != s.hex_color {
            // Changed elsewhere (a preset button, reset): show the new colour.
            s.hex_color = color;
            s.hex_seen = color.hex();
            s.hex_edit = TextEdit::new(color.hex());
            changed = true;
        }
    }

    // A tray recolour that has waited long enough.
    if let Some((_, since)) = s.tray_pending {
        if since.elapsed() >= Duration::from_secs(1) {
            s.refresh_tray();
        } else {
            tx.wake_after(Duration::from_secs(1).saturating_sub(since.elapsed()));
        }
    }

    // While a status is open in the editor, the light shows it.
    let preview = match (s.page, s.editing) {
        (Page::Colors, Some(st)) => Some(s.cfg.style(st)),
        _ => None,
    };
    if preview != s.preview_sent {
        s.preview_sent = preview;
        s.send(Cmd::Preview(preview));
    }

    // Save at most every half second while a slider moves.
    if let Some(due) = s.save_due {
        if due == f64::NEG_INFINITY {
            s.save_due = Some(now + 0.5);
            tx.wake_after(Duration::from_millis(500));
        } else if now >= due {
            s.save_due = None;
            if s.engine.is_some() {
                s.cfg.save();
            }
        } else {
            tx.wake_after(Duration::from_secs_f64(due - now));
        }
    }

    // "Updated 2 min ago" ages while the Teams page shows.
    if s.page == Page::Teams {
        if now - s.age_tick >= 15.0 {
            s.age_tick = now;
            changed = true;
        }
        tx.wake_after(Duration::from_secs(15));
    }
    changed
}

pub fn on_close(s: &mut Ui, tx: &mut Tx) {
    // A hidden window must not keep the editor's preview on the light.
    s.stop_preview();
    s.flush_save();
    if !s.cfg.close_to_tray || s.tray.is_none() {
        tx.quit();
    }
}

pub fn view(s: &Ui, cx: &mut Cx<Ui>) -> El {
    let dark = match s.cfg.appearance {
        Appearance::System => cx.system_dark(),
        Appearance::Light => false,
        Appearance::Dark => true,
    };
    let p = Pal::new(dark);
    use crate::icons;
    use rune::{el, text};
    let go = |page: Page| move |s: &mut Ui| s.page = page;
    let brand = row()
        .key("brand")
        .gap(10.0)
        .center_y()
        .px(8.0)
        .pb(22.0)
        .child(widgets::logo(34.0, cx.env().scale))
        .child(
            col()
                .gap(1.0)
                .child(text("HudLight").font(widgets::BRAND).color(p.text))
                .child(text("Busylight for Teams").font(widgets::SMALL).color(p.text2)),
        );
    let nav = col()
        .key("rail")
        .role("tablist")
        .label("Sections")
        .w_full()
        .gap(2.0)
        .focus_group()
        .child(widgets::nav_item(p, "nav-light", "Light", &icons::LIGHT, s.page == Page::Light, go(Page::Light)))
        .child(widgets::nav_item(p, "nav-teams", "Teams", &icons::TEAMS, s.page == Page::Teams, go(Page::Teams)))
        .child(widgets::nav_item(p, "nav-colors", "Colors", &icons::PALETTE, s.page == Page::Colors, go(Page::Colors)))
        .child(widgets::nav_item(p, "nav-settings", "Settings", &icons::SLIDERS, s.page == Page::Settings, go(Page::Settings)))
        .child(widgets::nav_item(p, "nav-help", "Help", &icons::HELP, s.page == Page::Help, go(Page::Help)));
    // What the light is doing, from any page.
    let (now, lit) = match (s.cfg.mode, s.output.status) {
        (Mode::Off, _) => ("Light off".to_owned(), false),
        (Mode::Manual, _) => ("Manual colour".to_owned(), s.output.effect != crate::config::Effect::Off),
        (Mode::Teams, Some(st)) => (st.label().to_owned(), s.output.effect != crate::config::Effect::Off),
        (Mode::Teams, None) => ("Waiting for Teams".to_owned(), false),
    };
    let device = match s.devices.first() {
        Some(d) if s.devices.len() > 1 => format!("{} +{}", d.product, s.devices.len() - 1),
        Some(d) => d.product.clone(),
        None => "No light connected".to_owned(),
    };
    let status_card = row()
        .key("now-card")
        .w_full()
        .padding(12.0)
        .gap(11.0)
        .center_y()
        .radius(10.0)
        .fill(p.card)
        .border(1.0, p.border)
        .shadow(1.0, 2.0, p.shadow)
        .child(widgets::orb(p, "side-orb", s.output.color, lit, 12.0))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(1.0)
                .child(text(now).font(widgets::BODY_MED).color(p.text).truncate())
                .child(text(device).font(widgets::SMALL).color(p.text2).truncate()),
        );
    // An update on offer, or on its way in, above the status card.
    let update_card = s.update.as_ref().and_then(|u| {
        let (title, sub) = match u {
            UpdateState::Available(a) => ("Update available".to_owned(), format!("Version {}", a.version)),
            UpdateState::Failed { .. } => ("Update failed".to_owned(), "Click to try again".to_owned()),
            UpdateState::Downloading { version } => ("Updating…".to_owned(), format!("Downloading {version}")),
            UpdateState::Ready { .. } => ("Restarting…".to_owned(), "Update installed".to_owned()),
            _ => return None,
        };
        Some(
            row()
                .key("side-update")
                .role("button")
                .label(format!("{title}: {sub}"))
                .focusable(true)
                .focus_ring(2.0, 0.0, p.ring.alpha(0.45))
                .w_full()
                .mb(8.0)
                .padding(12.0)
                .gap(11.0)
                .center_y()
                .radius(10.0)
                .fill(p.primary)
                .shadow(1.0, 2.0, p.shadow)
                .hover(rune::Color::WHITE.alpha(0.08))
                .press(rune::Color::BLACK.alpha(0.08))
                .cursor(rune::Cursor::Pointer)
                .on_press(|s: &mut Ui| s.open_update_dialog())
                .child(rune::icon(&icons::DOWNLOAD, 17.0).color(p.primary_text).a11y_hidden())
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(1.0)
                        .child(text(title).font(widgets::BODY_MED).color(p.primary_text).truncate().a11y_hidden())
                        .child(text(sub).font(widgets::SMALL).color(p.primary_text.alpha(0.85)).truncate().a11y_hidden()),
                ),
        )
    });
    let rail = col()
        .key("sidebar")
        .w(224.0)
        .h_full()
        .px(12.0)
        .pt(20.0)
        .pb(12.0)
        .fill(p.sidebar)
        .border_sides([0.0, 1.0, 0.0, 0.0], if p.dark { rune::Color::BLACK } else { p.border.alpha(0.6) })
        .child(brand)
        .child(text("MENU").font(widgets::LABEL).color(p.text2).px(10.0).pb(8.0))
        .child(nav)
        .child(el().grow(1.0))
        .children(update_card)
        .child(status_card);
    let page = match s.page {
        Page::Light => pages::light(s, p),
        Page::Teams => pages::teams(s, p),
        Page::Colors => pages::colors(s, p),
        Page::Settings => pages::settings(s, p),
        Page::Help => pages::help(s, p),
    };
    let main = col()
        .key("main")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .min_h(0.0)
        .fill(p.bg)
        .child(
            col()
                .key("scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec { stick_bottom: false })
                .child(col().key("page").w_full().max_w(880.0).px(32.0).pt(30.0).pb(40.0).child(page)),
        );
    let dialog = if s.update_dialog { pages::update_dialog(s, p) } else { None };
    row().key("window").w_full().h_full().fill(p.bg).child(rail).child(main).children(dialog)
}

#[cfg(test)]
mod tests;
