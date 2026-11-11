//! Drives the real view headlessly: clicks land on elements by key, as a person's would.

use rune::{Env, Runtime};

use super::*;
use crate::config::{Effect, Rgb, Source};

fn rt() -> Runtime<Ui> {
    let mut rt = Runtime::new(Ui::demo(), view, (960.0, 660.0), Env { system_dark: false, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn click(rt: &mut Runtime<Ui>, key: &str) {
    // Scroll it into view first, as a person would.
    let target = key.split('/').next().unwrap_or(key).to_owned();
    rt.update(move |_, tx| tx.scroll_into_view(target.clone()));
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

#[test]
fn the_rail_switches_pages() {
    let mut rt = rt();
    for (key, page) in [("nav-teams", Page::Teams), ("nav-colors", Page::Colors), ("nav-settings", Page::Settings), ("nav-help", Page::Help), ("nav-light", Page::Light)] {
        click(&mut rt, key);
        assert_eq!(rt.state.page, page, "after {key}");
    }
}

#[test]
fn manual_mode_shows_a_colour_editor_that_sets_the_colour() {
    let mut rt = rt();
    assert!(rt.find("manual").is_none());
    click(&mut rt, "mode-seg/Manual");
    assert_eq!(rt.state.cfg.mode, Mode::Manual);
    assert!(rt.find("manual").is_some(), "the manual editor appears");
    click(&mut rt, "preset-5");
    assert_eq!(rt.state.cfg.manual.color, Rgb(0xFF, 0, 0));
    click(&mut rt, "effect/Blink");
    assert_eq!(rt.state.cfg.manual.effect, Effect::Blink);
    click(&mut rt, "mode-seg/Off");
    assert_eq!(rt.state.cfg.mode, Mode::Off);
}

#[test]
fn a_status_row_opens_its_editor_and_presets_recolour_it() {
    let mut rt = rt();
    click(&mut rt, "nav-colors");
    click(&mut rt, "row-away");
    assert_eq!(rt.state.editing, Some(Status::Away));
    click(&mut rt, "preset-8");
    assert_eq!(rt.state.cfg.style(Status::Away).color, Rgb(0x00, 0x78, 0xD4));
    click(&mut rt, "effect/Pulse");
    assert_eq!(rt.state.cfg.style(Status::Away).effect, Effect::Pulse);
    click(&mut rt, "teams-default");
    assert_eq!(rt.state.cfg.style(Status::Away).color, Status::Away.teams_color());
    // Another row takes over; pressing it again closes it.
    click(&mut rt, "row-busy");
    assert_eq!(rt.state.editing, Some(Status::Busy));
    click(&mut rt, "row-busy");
    assert_eq!(rt.state.editing, None);
}

#[test]
fn the_vivid_preset_and_reset_cover_every_status() {
    let mut rt = rt();
    click(&mut rt, "nav-colors");
    click(&mut rt, "preset-vivid");
    assert_eq!(rt.state.cfg.style(Status::Available).color, Rgb(0, 0xFF, 0));
    click(&mut rt, "preset-reset");
    assert!(Status::ALL.iter().all(|s| rt.state.cfg.style(*s) == s.default_style()));
}

#[test]
fn dragging_the_red_slider_changes_only_red() {
    let mut rt = rt();
    click(&mut rt, "nav-colors");
    click(&mut rt, "row-available");
    let before = rt.state.cfg.style(Status::Available).color;
    let (x, y) = rt.find("ch-r/slider").unwrap();
    // The slider is 220 wide around its centre: press near the left end, drag to the right end.
    rt.pointer_down(x - 100.0, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_move(x + 120.0, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x + 120.0, y);
    rt.frame(1.0 / 60.0);
    let after = rt.state.cfg.style(Status::Available).color;
    assert_eq!((after.0, after.1, after.2), (255, before.1, before.2));
}

#[test]
fn settings_controls_change_settings() {
    let mut rt = rt();
    click(&mut rt, "nav-settings");
    let gamma = rt.state.cfg.gamma_correction;
    click(&mut rt, "gamma");
    assert_eq!(rt.state.cfg.gamma_correction, !gamma);
    click(&mut rt, "appearance/Dark");
    assert_eq!(rt.state.cfg.appearance, Appearance::Dark);
    let (x, y) = rt.find("brightness/slider").unwrap();
    rt.pointer_down(x - 105.0, y);
    rt.pointer_up(x - 105.0, y);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.cfg.brightness < 0.1, "{}", rt.state.cfg.brightness);
    // Last, as reaching it scrolls the page.
    assert!(rt.state.cfg.auto_update);
    click(&mut rt, "auto-update");
    assert!(!rt.state.cfg.auto_update);
}

#[test]
fn the_source_switch_shows_graph_sign_in() {
    let mut rt = rt();
    click(&mut rt, "nav-teams");
    assert!(rt.find("client").is_some());
    click(&mut rt, "source-seg/Microsoft Graph");
    assert_eq!(rt.state.cfg.source, Source::Graph);
    assert!(rt.find("client-id").is_some());
    assert!(rt.find("sign-in").is_some());
}

#[test]
fn the_hex_field_applies_what_is_typed() {
    let mut rt = rt();
    click(&mut rt, "nav-colors");
    click(&mut rt, "row-busy");
    // poll syncs the field to the open status, then applies a typed value.
    rt.update(|s, tx| {
        poll(s, 0.0, tx);
    });
    assert_eq!(rt.state.hex_edit.text, rt.state.cfg.style(Status::Busy).color.hex());
    rt.update(|s, tx| {
        s.hex_edit.replace_text("#123456");
        poll(s, 0.1, tx);
    });
    assert_eq!(rt.state.cfg.style(Status::Busy).color, Rgb(0x12, 0x34, 0x56));
}

#[test]
fn tray_icons_are_well_formed() {
    let lit = tray_icon(Rgb(255, 0, 0));
    let off = tray_icon(Rgb::OFF);
    assert_ne!(lit, off);
}

#[test]
fn preset_buttons_refresh_the_open_hex_field() {
    let mut rt = rt();
    click(&mut rt, "nav-colors");
    click(&mut rt, "row-busy");
    rt.update(|s, tx| {
        poll(s, 0.0, tx);
        s.hex_edit.replace_text("#123456");
        poll(s, 0.1, tx);
    });
    click(&mut rt, "preset-vivid");
    rt.update(|s, tx| {
        poll(s, 0.2, tx);
    });
    assert_eq!(rt.state.hex_edit.text, "#FF0000");
    // Typing a partial value is left alone until it parses.
    rt.update(|s, tx| {
        s.hex_edit.replace_text("#12");
        poll(s, 0.3, tx);
        poll(s, 0.4, tx);
    });
    assert_eq!(rt.state.hex_edit.text, "#12");
}

#[test]
fn closing_the_window_ends_the_editor_preview() {
    let mut rt = rt();
    click(&mut rt, "nav-colors");
    click(&mut rt, "row-away");
    rt.update(|s, tx| {
        poll(s, 0.0, tx);
    });
    assert!(rt.state.preview_sent.is_some());
    rt.update(|s, tx| on_close(s, tx));
    assert_eq!(rt.state.editing, None);
    assert!(rt.state.preview_sent.is_none());
    rt.update(|s, tx| {
        poll(s, 0.1, tx);
    });
    assert!(rt.state.preview_sent.is_none(), "a hidden window's poll doesn't bring it back");
}

#[test]
fn headless_autostart_toggle_never_touches_the_registry() {
    let mut rt = rt();
    click(&mut rt, "nav-settings");
    click(&mut rt, "autostart");
    // Demo state has no engine, so this only flips the setting.
    assert!(rt.state.cfg.start_with_windows);
    assert!(rt.state.message.is_none());
}

fn offer(version: &str) -> crate::update::UpdateState {
    crate::update::UpdateState::Available(crate::update::Available {
        version: version.into(),
        notes: "## What's Changed\n* Blink for VT lights by @someone in https://github.com/x/y/pull/1\n\n**Full Changelog**: https://x".into(),
        url: String::new(),
    })
}

#[test]
fn a_found_update_shows_a_dialog_and_stays_in_the_sidebar_after_later() {
    let mut rt = rt();
    assert!(rt.find("side-update").is_none());
    rt.update(|s, _| s.set_update_state(offer("9.0.0")));
    rt.frame(1.0 / 60.0);
    assert!(rt.find("update-dialog").is_some(), "the dialog opens");
    assert!(rt.find("side-update").is_some(), "the sidebar shows it");

    click(&mut rt, "update-later");
    assert!(rt.find("update-dialog").is_none(), "Later closes it");
    assert!(rt.find("side-update").is_some(), "and the sidebar keeps it");

    // Finding the same version again doesn't nag; the sidebar reopens it.
    rt.update(|s, _| s.set_update_state(offer("9.0.0")));
    rt.frame(1.0 / 60.0);
    assert!(rt.find("update-dialog").is_none());
    click(&mut rt, "side-update");
    assert!(rt.find("update-dialog").is_some());

    // A newer one than the one put off asks again.
    click(&mut rt, "update-later");
    rt.update(|s, _| s.set_update_state(offer("9.1.0")));
    rt.frame(1.0 / 60.0);
    assert!(rt.find("update-dialog").is_some());
}

#[test]
fn release_notes_lose_their_markdown() {
    assert_eq!(super::pages::plain_notes("## What's Changed\n* Blink for VT lights by @someone in https://github.com/x/y/pull/1\n\n**Full Changelog**: https://x"), "What's Changed\n•  Blink for VT lights");
}
