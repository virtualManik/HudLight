//! HudLight: drives a VT DND Omega (Kuando Busylight) from Microsoft Teams presence.
//!
//! `hudlight` opens the window; `--minimized` starts in the tray. Rune's headless flags
//! (`--snap out.png --scene colors`, `--tree`, `--script`) render demo data without touching
//! the light, Teams or the settings file.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod autostart;
mod busylight;
mod config;
mod engine;
mod icons;
mod secret;
mod teams;
mod ui;

use config::{Config, Effect, Mode, Status};
use ui::{Page, Ui};

const SCENES: &[(&str, fn(&mut Ui))] = &[
    ("light", |_| {}),
    ("disconnected", |s| s.devices.clear()),
    ("manual", |s| {
        s.cfg.mode = Mode::Manual;
        s.cfg.manual.color = config::Rgb(0x00, 0x78, 0xD4);
        s.output = engine::Output { color: s.cfg.manual.color, effect: Effect::Solid, status: None };
    }),
    ("teams", |s| s.page = Page::Teams),
    ("graph", |s| {
        s.page = Page::Teams;
        s.cfg.source = config::Source::Graph;
        s.graph = engine::GraphState::Code { user_code: "F7K2-QX9M".into(), url: "https://microsoft.com/devicelogin".into() };
    }),
    ("colors", |s| s.page = Page::Colors),
    ("colors-open", |s| {
        s.page = Page::Colors;
        s.editing = Some(Status::Away);
    }),
    ("settings", |s| s.page = Page::Settings),
    ("help", |s| s.page = Page::Help),
    ("dark", |s| s.cfg.appearance = config::Appearance::Dark),
    ("dark-colors", |s| {
        s.cfg.appearance = config::Appearance::Dark;
        s.page = Page::Colors;
        s.editing = Some(Status::Available);
    }),
];

fn is_headless() -> bool {
    const FLAGS: &[&str] = &["--snap", "--tree", "--script", "--list-scenes", "--trace", "--video", "--out", "--first-pixels"];
    std::env::args().any(|a| FLAGS.contains(&a.as_str()))
}

fn main() {
    let headless = is_headless();
    if !headless && !rune::single_instance("com.cuescript.hudlight") {
        return;
    }
    let minimized = std::env::args().any(|a| a == "--minimized");
    let spec = rune::AppSpec::<Ui> {
        min_size: Some((900.0, 560.0)),
        scenes: SCENES,
        poll: Some(ui::poll),
        ..rune::AppSpec::new("HudLight", (960.0, 660.0), Ui::demo, ui::view)
    };
    let mut app = rune::App::new(spec).keep_running().on_close(ui::on_close);

    if headless {
        // Strip our own flag so Rune doesn't see it.
        app = app.args(std::env::args().skip(1).filter(|a| a != "--minimized").collect::<Vec<_>>());
        app.run();
        return;
    }

    // The window's title bar, taskbar and Alt+Tab icon (Explorer's comes from build.rs).
    if let Err(e) = rune::set_app_icon(include_bytes!("../assets/icon-256.png").as_slice()) {
        eprintln!("hudlight: {e}");
    }
    let cfg = Config::load();
    if minimized || cfg.start_minimized {
        app = app.windowless();
    }
    let handle = app.handle();
    let notify = handle.clone();
    let debug = std::env::var_os("HUDLIGHT_DEBUG").is_some();
    let engine = engine::Engine::start(cfg.clone(), move |u| {
        if debug {
            eprintln!("hudlight: {u:?}");
        }
        notify.update(move |s, _| s.apply(u));
    });
    let tx = engine.sender();
    let init_handle = handle.clone();
    app = app.args(std::env::args().skip(1).filter(|a| a != "--minimized").collect::<Vec<_>>()).init(move || {
        let mut s = Ui::new(cfg, Some(tx), Some(init_handle));
        s.refresh_tray();
        s
    });
    app.run();
    engine.stop();
}
