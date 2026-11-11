//! The five pages. Each is a header and a stack of cards.

use rune::{El, col, el, icon, row, text};

use super::widgets::{self as w, Pal, Tone, Variant};
use super::{Target, Ui};
use crate::config::{Appearance, Effect, Mode, Rgb, Source, Status};
use crate::engine::GraphState;
use crate::icons;

const PRESETS: [Rgb; 12] = [
    Rgb(0x6B, 0xB7, 0x00),
    Rgb(0xC4, 0x31, 0x4B),
    Rgb(0xFF, 0xAA, 0x44),
    Rgb(0xB4, 0x00, 0x9E),
    Rgb(0x00, 0xFF, 0x00),
    Rgb(0xFF, 0x00, 0x00),
    Rgb(0xFF, 0xE6, 0x00),
    Rgb(0xFF, 0x6A, 0x00),
    Rgb(0x00, 0x78, 0xD4),
    Rgb(0x00, 0xB7, 0xC3),
    Rgb(0xFF, 0x4F, 0xA3),
    Rgb(0xFF, 0xFF, 0xFF),
];

fn ago(at: std::time::SystemTime) -> String {
    let secs = at.elapsed().map(|d| d.as_secs()).unwrap_or(0);
    match secs {
        0..=44 => "just now".into(),
        45..=89 => "a minute ago".into(),
        90..=3599 => format!("{} min ago", (secs + 30) / 60),
        _ => format!("{} h ago", secs / 3600),
    }
}

fn effect_text(e: Effect) -> &'static str {
    match e {
        Effect::Solid => "solid",
        Effect::Blink => "blinking",
        Effect::Pulse => "pulsing",
        Effect::Off => "off",
    }
}

// --- Light ---------------------------------------------------------------------------------

pub fn light(s: &Ui, p: Pal) -> El {
    let (status_line, sub, tone, mode_badge) = match (s.cfg.mode, s.output.status) {
        (Mode::Off, _) => ("Off".to_owned(), "The light is switched off.".to_owned(), Tone::Neutral, "Off"),
        (Mode::Manual, _) => (s.cfg.manual.color.hex(), format!("A colour you picked, {}.", effect_text(s.cfg.manual.effect)), Tone::Brand, "Manual"),
        (Mode::Teams, Some(st)) => (st.label().to_owned(), format!("From your Microsoft Teams status, {}.", effect_text(s.cfg.style(st).effect)), Tone::Brand, "Following Teams"),
        (Mode::Teams, None) => ("Waiting for Teams".to_owned(), "Open Teams and your status shows here.".to_owned(), Tone::Neutral, "Following Teams"),
    };
    let lit = s.output.effect != Effect::Off;
    let showing = w::card(p, "showing").child(
        row()
            .w_full()
            .padding(24.0)
            .gap(22.0)
            .center_y()
            .child(el().size(76.0, 76.0).center().child(w::orb(p, "orb", s.output.color, lit, 60.0)))
            .child(
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(6.0)
                    .child(text("NOW SHOWING").font(w::LABEL).color(p.text2))
                    .child(text(status_line).key("now").font(w::BIG).color(p.text))
                    .child(text(sub).font(w::BODY).color(p.text2).wrap()),
            )
            .child(
                col()
                    .items_end()
                    .gap(12.0)
                    .child(w::badge(p, tone, mode_badge))
                    .child(w::button_icon(p, "test", "Test light", Some(&icons::ZAP), Variant::Outline, |s, _| s.test_light())),
            ),
    );

    let device = match s.devices.first() {
        Some(d) => {
            let mut list = col()
                .w_full()
                .child(w::kv(p, "Product", w::mono(p, d.product.clone())))
                .child(w::kv(p, "Model", w::mono(p, d.model)))
                .child(w::kv(p, "Firmware", w::mono(p, format!("0x{:X}", d.release))))
                .child(w::kv(p, "USB ID", w::mono(p, format!("{:04X}:{:04X}", d.vid, d.pid))));
            if s.devices.len() > 1 {
                list = list.child(w::note(p, format!("+ {} more light(s), all kept in sync.", s.devices.len() - 1)));
            }
            w::card(p, "device").grow(1.0).min_w(0.0).child(w::card_header(p, "Device", "Busylight on USB", w::badge(p, Tone::Ok, "Connected"))).child(w::card_body(list))
        }
        None => w::card(p, "device")
            .grow(1.0)
            .min_w(0.0)
            .child(w::card_header(p, "Device", "Busylight on USB", w::badge(p, Tone::Warn, "Not connected")))
            .child(w::card_body(
                row()
                    .gap(12.0)
                    .items_start()
                    .child(icon(&icons::USB, 20.0).color(p.text2).a11y_hidden())
                    .child(col().grow(1.0).min_w(0.0).child(w::note(p, "Plug in your VT DND Omega or any Kuando Busylight. It's picked up automatically, no restart needed."))),
            )),
    };

    let modes = [(Mode::Teams, "Teams"), (Mode::Manual, "Manual"), (Mode::Off, "Off")];
    let mode = w::card(p, "mode").grow(1.0).min_w(0.0).child(w::card_header(p, "Mode", "What drives the light", el())).child(w::card_body(
        col()
            .gap(12.0)
            .child(w::segmented(p, "mode-seg", "Mode", &modes, s.cfg.mode, 80.0, |s: &mut Ui, m| s.set_mode(m)))
            .child(w::note(
                p,
                match s.cfg.mode {
                    Mode::Teams => "The light follows your Teams presence, using the colours on the Colors page.",
                    Mode::Manual => "The light holds the colour below until you switch back to Teams.",
                    Mode::Off => "The light stays dark. The app keeps running in the tray.",
                },
            )),
    ));

    let mut page = col()
        .key("light")
        .w_full()
        .gap(16.0)
        .child(w::page_header(p, "Light", "What your status light is showing, and what decides it.", el()))
        .child(showing)
        .child(row().w_full().gap(16.0).items_stretch().child(device).child(mode));
    if s.cfg.mode == Mode::Manual {
        page = page.child(
            w::card(p, "manual")
                .child(w::card_header(p, "Manual colour", "Pick a colour and effect. The light changes as you go.", el()))
                .child(w::card_body(color_editor(s, p, Target::Manual))),
        );
    }
    page
}

/// Presets, R/G/B sliders, a hex field and the effect, for one colour.
fn color_editor(s: &Ui, p: Pal, t: Target) -> El {
    let cur = s.color_of(t);
    let effect = match t {
        Target::Status(st) => s.cfg.style(st).effect,
        Target::Manual => s.cfg.manual.effect,
    };
    let mut chips = row().key("presets").role("radiogroup").label("Preset colours").gap(12.0).center_y().flex_wrap();
    if let Target::Status(st) = t {
        let teams = st.teams_color();
        chips = chips
            .child(
                row()
                    .gap(8.0)
                    .center_y()
                    .child(w::swatch(p, "teams-default".into(), teams, cur == teams, 24.0, move |s| s.set_color(t, teams)))
                    .child(text("Teams").font(w::SMALL_MED).color(p.text2)),
            )
            .child(el().w(1.0).h(20.0).fill(p.divider));
    }
    for (i, c) in PRESETS.iter().copied().enumerate() {
        chips = chips.child(w::swatch(p, format!("preset-{i}"), c, cur == c, 24.0, move |s| s.set_color(t, c)));
    }
    let names = ["Red", "Green", "Blue"];
    let inks = [w::c("#E5484D"), w::c("#30A46C"), w::c("#3E63DD")];
    let mut sliders = col().gap(8.0);
    for ch in 0..3 {
        let v = f32::from(cur.get(ch)) / 255.0;
        sliders = sliders.child(
            row().gap(12.0).center_y().child(text(names[ch]).font(w::SMALL_MED).color(p.text2).w(52.0)).child(w::slider(
                p,
                ["ch-r", "ch-g", "ch-b"][ch],
                names[ch],
                v,
                format!("{}", cur.get(ch)),
                inks[ch],
                move |s: &Ui| f32::from(s.color_of(t).get(ch)) / 255.0,
                move |s: &mut Ui, v: f32| {
                    let c = s.color_of(t).with(ch, (v * 255.0).round() as u8);
                    s.set_color(t, c);
                },
            )),
        );
    }
    let hex = row()
        .gap(12.0)
        .center_y()
        .child(text("Hex").font(w::SMALL_MED).color(p.text2).w(52.0))
        .child(w::field(p, "hex", "Hex colour", &s.hex_edit, "#RRGGBB", 128.0, |s: &mut Ui| &mut s.hex_edit))
        .child(el().size(34.0, 34.0).radius(8.0).fill(w::rgb(cur)).border(1.0, rune::Color::BLACK.alpha(0.12)).a11y_hidden());
    let effects: Vec<(Effect, &'static str)> = Effect::ALL.iter().map(|e| (*e, e.label())).collect();
    let fx = w::segmented(p, "effect", "Effect", &effects, effect, 70.0, move |s: &mut Ui, e| s.set_effect(t, e));
    col()
        .key("editor")
        .gap(18.0)
        .child(chips)
        .child(w::divider(p))
        .child(sliders)
        .child(hex)
        .child(row().gap(12.0).center_y().child(text("Effect").font(w::SMALL_MED).color(p.text2).w(52.0)).child(fx))
}

// --- Teams ---------------------------------------------------------------------------------

pub fn teams(s: &Ui, p: Pal) -> El {
    let following = s.cfg.mode == Mode::Teams;
    let action = if following {
        w::badge(p, Tone::Ok, "Following")
    } else {
        w::button(p, "follow", "Follow Teams", Variant::Primary, |s, _| s.set_mode(Mode::Teams))
    };
    let presence_body = match &s.presence {
        Some(pr) => {
            let st = s.cfg.style(pr.status);
            let source = match pr.source {
                Source::TeamsClient => "Teams app",
                Source::Graph => "Microsoft Graph",
            };
            row()
                .gap(16.0)
                .center_y()
                .child(el().size(44.0, 44.0).center().child(w::orb(p, "presence-orb", st.color, st.effect != Effect::Off || pr.status == Status::Offline, 32.0)))
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(3.0)
                        .child(text(pr.status.label()).key("presence").font(w::BIG).color(p.text))
                        .child(text(format!("Reported as \"{}\" by the {source} · {}", pr.raw, ago(pr.at))).font(w::SMALL).color(p.text2).wrap()),
                )
        }
        None => col().gap(4.0).child(text("No presence yet").key("presence").font(w::BIG).color(p.text)).child(w::note(
            p,
            match s.cfg.source {
                Source::TeamsClient => "Open Microsoft Teams and sign in. Your status appears here within a second or two of changing.",
                Source::Graph => "Sign in to Microsoft Graph below.",
            },
        )),
    };
    let presence = w::card(p, "presence").child(w::card_header(p, "Presence", "Your current Teams status", action)).child(w::card_body(presence_body));

    let sources = [(Source::TeamsClient, "Teams app"), (Source::Graph, "Microsoft Graph")];
    let source = w::card(p, "source").child(w::card_header(p, "Status source", "Where HudLight reads your status from", el())).child(w::card_body(
        col()
            .gap(12.0)
            .child(w::segmented(p, "source-seg", "Status source", &sources, s.cfg.source, 140.0, |s: &mut Ui, v| {
                s.cfg.source = v;
                s.commit();
            }))
            .child(w::note(
                p,
                match s.cfg.source {
                    Source::TeamsClient => "Reads the status the Teams desktop app writes to its own log on this PC. Works with new and classic Teams, no sign-in or admin consent.",
                    Source::Graph => "Asks Microsoft Graph for your presence. Works with Teams on any device, and needs an Entra ID app registration with the Presence.Read permission.",
                },
            )),
    ));

    let detail = match s.cfg.source {
        Source::TeamsClient => {
            let (running, running_tone) = match s.teams_running {
                Some(true) => ("Running", Tone::Ok),
                Some(false) => ("Not running", Tone::Warn),
                None => ("Checking…", Tone::Neutral),
            };
            let log = match &s.log_path {
                Some(path) => w::mono(p, path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()),
                None => text("Not found").font(w::BODY).color(p.text2),
            };
            let ok = s.log_path.is_some() && s.teams_running != Some(false);
            w::card(p, "client")
                .child(w::card_header(p, "Teams app", "The Teams client on this PC", w::badge(p, if ok { Tone::Ok } else { Tone::Warn }, if ok { "Connected" } else { "Not found" })))
                .child(w::card_body(col().w_full().child(w::kv(p, "Teams", row().child(w::badge(p, running_tone, running)))).child(w::kv(p, "Log file", log))))
                .child(w::card_footer(p, w::note(p, "When Teams is closed, the light shows the \"Offline / Teams closed\" style."), el()))
        }
        Source::Graph => graph_card(s, p),
    };

    col()
        .key("teams")
        .w_full()
        .gap(16.0)
        .child(w::page_header(p, "Microsoft Teams", "Your presence, and where it comes from.", el()))
        .child(presence)
        .child(source)
        .child(detail)
}

fn graph_card(s: &Ui, p: Pal) -> El {
    let fields = row()
        .w_full()
        .gap(14.0)
        .flex_wrap()
        .child(w::form_field(
            p,
            "Client ID",
            w::field(p, "client-id", "Client ID", &s.client_id, "00000000-0000-0000-0000-000000000000", 320.0, |s: &mut Ui| &mut s.client_id),
        ))
        .child(w::form_field(p, "Tenant", w::field(p, "tenant", "Tenant", &s.tenant, "organizations", 200.0, |s: &mut Ui| &mut s.tenant)));
    let (badge, status, action): (El, El, El) = match &s.graph {
        GraphState::SignedOut => (
            w::badge(p, Tone::Neutral, "Signed out"),
            el(),
            w::button(p, "sign-in", "Sign in", Variant::Primary, |s, _| s.graph_sign_in()),
        ),
        GraphState::Starting => (w::badge(p, Tone::Neutral, "Starting"), w::note(p, "Asking Microsoft for a sign-in code…"), el()),
        GraphState::Code { user_code, url } => {
            let (code, url2) = (user_code.clone(), url.clone());
            (
                w::badge(p, Tone::Warn, "Waiting for you"),
                col()
                    .w_full()
                    .gap(12.0)
                    .padding(16.0)
                    .radius(10.0)
                    .fill(if p.dark { p.muted } else { w::c("#FBF2F9") })
                    .border(1.0, p.divider)
                    .child(text(format!("Go to {url} and enter this code:")).font(w::BODY).color(p.text).wrap())
                    .child(text(user_code.clone()).key("user-code").font(w::BIG.mono().tracking(2.0)).color(p.text).selectable(true))
                    .child(
                        row()
                            .gap(8.0)
                            .child(w::button_icon(p, "open-login", "Open sign-in page", Some(&icons::EXTERNAL), Variant::Primary, move |_, tx| {
                                let _ = tx.open_url(&url2);
                            }))
                            .child(w::button_icon(p, "copy-code", "Copy code", Some(&icons::COPY), Variant::Outline, move |_, tx| tx.copy(code.clone()))),
                    ),
                w::button(p, "cancel-sign-in", "Cancel", Variant::Outline, |s, _| s.graph_sign_out()),
            )
        }
        GraphState::SignedIn { name } => (
            w::badge(p, Tone::Ok, "Signed in"),
            row().gap(8.0).center_y().child(icon(&icons::CHECK, 16.0).color(p.ok)).child(text(format!("Signed in as {name}")).font(w::BODY_MED).color(p.text)),
            w::button(p, "sign-out", "Sign out", Variant::Outline, |s, _| s.graph_sign_out()),
        ),
        GraphState::Error(e) => (
            w::badge(p, Tone::Warn, "Error"),
            row()
                .w_full()
                .gap(10.0)
                .items_start()
                .padding(12.0)
                .radius(8.0)
                .fill(p.danger.alpha(0.07))
                .border(1.0, p.danger.alpha(0.3))
                .child(icon(&icons::ALERT, 17.0).color(p.danger))
                .child(text(e.clone()).font(w::SMALL).color(p.text).wrap().grow(1.0).min_w(0.0)),
            w::button(p, "sign-in", "Sign in", Variant::Primary, |s, _| s.graph_sign_in()),
        ),
    };
    w::card(p, "graph")
        .child(w::card_header(p, "Microsoft Graph", "Sign in with a device code", badge))
        .child(w::card_body(col().w_full().gap(16.0).child(fields).child(status)))
        .child(w::card_footer(
            p,
            w::note(p, "Register an app in Microsoft Entra ID: Mobile and desktop platform, \"Allow public client flows\" on, delegated Presence.Read and User.Read."),
            action,
        ))
}

// --- Colors --------------------------------------------------------------------------------

pub fn colors(s: &Ui, p: Pal) -> El {
    let presets = row()
        .gap(8.0)
        .child(w::button(p, "preset-teams", "Teams colors", Variant::Outline, |s, _| {
            for st in Status::ALL {
                s.cfg.style_mut(st).color = st.teams_color();
            }
            s.commit();
        }))
        .child(w::button_icon(p, "preset-vivid", "Vivid LED", Some(&icons::SPARKLE), Variant::Outline, |s, _| {
            for st in Status::ALL {
                s.cfg.style_mut(st).color = st.vivid_color();
            }
            s.commit();
        }))
        .child(w::button_icon(p, "preset-reset", "Reset", Some(&icons::RESET), Variant::Ghost, |s, _| {
            for st in Status::ALL {
                *s.cfg.style_mut(st) = st.default_style();
            }
            s.commit();
        }));
    let mut list = col().key("statuses").w_full();
    for (i, st) in Status::ALL.into_iter().enumerate() {
        let style = s.cfg.style(st);
        let open = s.editing == Some(st);
        let summary = row()
            .key(format!("row-{}", st.key()))
            .role("button")
            .label(st.label())
            .expanded(open)
            .focusable(true)
            .focus_fill(p.hover)
            .w_full()
            .h(54.0)
            .px(20.0)
            .gap(14.0)
            .center_y()
            .hover(p.hover)
            .press(p.press)
            .cursor(rune::Cursor::Pointer)
            .on_press_tx(move |s: &mut Ui, tx| {
                s.editing = if s.editing == Some(st) { None } else { Some(st) };
                if s.editing.is_some() {
                    tx.scroll_into_view(format!("status-{}", st.key()));
                }
            })
            .child(el().size(24.0, 24.0).center().child(w::orb(p, "dot", style.color, style.effect != Effect::Off, 16.0)))
            .child(text(st.label()).font(w::BODY_MED).color(p.text).grow(1.0).min_w(0.0))
            .child(
                row()
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(if p.dark { p.muted } else { w::c("#F8ECF6") })
                    .child(text(style.color.hex()).font(w::MONO).color(p.text2)),
            )
            .child(row().w(64.0).child(w::badge(p, if style.effect == Effect::Off { Tone::Neutral } else { Tone::Brand }, style.effect.label())))
            .child(icon(if open { &icons::CHEVRON_UP } else { &icons::CHEVRON_DOWN }, 17.0).color(p.text2).a11y_hidden());
        let mut item = col().key(format!("status-{}", st.key())).w_full();
        if i > 0 {
            item = item.child(w::divider(p));
        }
        item = item.child(summary);
        if open {
            item = item.child(
                col()
                    .key("open")
                    .w_full()
                    .px(20.0)
                    .pl(58.0)
                    .pt(18.0)
                    .pb(22.0)
                    .fill(p.muted)
                    .border_sides([1.0, 0.0, 0.0, 0.0], p.divider)
                    .child(color_editor(s, p, Target::Status(st))),
            );
        }
        list = list.child(item);
    }
    col()
        .key("colors")
        .w_full()
        .gap(16.0)
        .child(w::page_header(p, "Status colors", "How the light shows each Teams status. An open status shows on the light.", presets))
        .child(w::card(p, "status-card").child(list))
}

// --- Settings ------------------------------------------------------------------------------

pub fn settings(s: &Ui, p: Pal) -> El {
    let light = w::card(p, "light-settings").child(w::card_header(p, "Light", "Brightness and effects", el())).child(w::divider(p)).child(w::rows(
        p,
        vec![
            w::setting(
                p,
                "Brightness",
                "How bright the LED shines.",
                w::slider(
                    p,
                    "brightness",
                    "Brightness",
                    (s.cfg.brightness - 0.05) / 0.95,
                    format!("{:.0}%", s.cfg.brightness * 100.0),
                    p.primary,
                    |s: &Ui| (s.cfg.brightness - 0.05) / 0.95,
                    |s: &mut Ui, v| {
                        s.cfg.brightness = 0.05 + v * 0.95;
                        s.commit();
                    },
                ),
            ),
            w::setting(
                p,
                "Blink speed",
                "One on-and-off cycle.",
                w::slider(
                    p,
                    "blink",
                    "Blink period",
                    (s.cfg.blink_period - 0.2) / 2.8,
                    format!("{:.1} s", s.cfg.blink_period),
                    p.primary,
                    |s: &Ui| (s.cfg.blink_period - 0.2) / 2.8,
                    |s: &mut Ui, v| {
                        s.cfg.blink_period = ((0.2 + v * 2.8) * 10.0).round() / 10.0;
                        s.commit();
                    },
                ),
            ),
            w::setting(
                p,
                "Pulse speed",
                "One full breath of the pulse effect.",
                w::slider(
                    p,
                    "pulse",
                    "Pulse period",
                    (s.cfg.pulse_period - 0.6) / 5.4,
                    format!("{:.1} s", s.cfg.pulse_period),
                    p.primary,
                    |s: &Ui| (s.cfg.pulse_period - 0.6) / 5.4,
                    |s: &mut Ui, v| {
                        s.cfg.pulse_period = ((0.6 + v * 5.4) * 10.0).round() / 10.0;
                        s.commit();
                    },
                ),
            ),
            w::setting(
                p,
                "Colour correction",
                "Match the LED to on-screen colours. Turn off for the brightest light.",
                w::toggle(p, "gamma", "Colour correction", s.cfg.gamma_correction, |s| {
                    s.cfg.gamma_correction = !s.cfg.gamma_correction;
                    s.commit();
                }),
            ),
        ],
    ));
    let mut app_rows = vec![
        w::setting(
            p,
            "Start with Windows",
            "Open HudLight in the tray when you sign in.",
            w::toggle(p, "autostart", "Start with Windows", s.cfg.start_with_windows, |s| {
                let on = !s.cfg.start_with_windows;
                s.set_autostart(on);
            }),
        ),
        w::setting(
            p,
            "Keep running in tray",
            "Closing the window keeps the light in sync.",
            w::toggle(p, "tray", "Keep running in the tray when the window closes", s.cfg.close_to_tray, |s| {
                s.cfg.close_to_tray = !s.cfg.close_to_tray;
                s.commit();
            }),
        ),
        w::setting(
            p,
            "Start minimized",
            "Start in the tray without opening this window.",
            w::toggle(p, "minimized", "Start minimized to the tray", s.cfg.start_minimized, |s| {
                s.cfg.start_minimized = !s.cfg.start_minimized;
                s.commit();
            }),
        ),
        w::setting(
            p,
            "Appearance",
            "",
            w::segmented(
                p,
                "appearance",
                "Appearance",
                &[(Appearance::System, "System"), (Appearance::Light, "Light"), (Appearance::Dark, "Dark")],
                s.cfg.appearance,
                72.0,
                |s: &mut Ui, a| {
                    s.cfg.appearance = a;
                    s.commit();
                },
            ),
        ),
    ];
    if let Some(m) = &s.message {
        app_rows.push(row().px(20.0).py(12.0).gap(8.0).center_y().child(icon(&icons::ALERT, 16.0).color(p.warn)).child(text(m.clone()).font(w::SMALL).color(p.warn).wrap()));
    }
    let app = w::card(p, "app-settings").child(w::card_header(p, "App", "Startup, tray and theme", el())).child(w::divider(p)).child(w::rows(p, app_rows));
    col()
        .key("settings")
        .w_full()
        .gap(16.0)
        .child(w::page_header(p, "Settings", "Tune the light and how HudLight runs.", el()))
        .child(light)
        .child(app)
        .child(updates(s, p))
}

fn updates(s: &Ui, p: Pal) -> El {
    let status = match (&s.update, &s.updater) {
        (Some(state), _) => state.label(),
        (None, None) if s.live() => "Updates are off in development builds.".to_owned(),
        (None, _) => "Checks GitHub for new versions.".to_owned(),
    };
    let action = if s.available_update().is_some() {
        w::button(p, "settings-update-now", "Update now", Variant::Primary, |s, _| s.install_update())
    } else if s.updating() {
        el()
    } else {
        w::button(p, "check-update", "Check now", Variant::Outline, |s, _| s.check_for_update())
    };
    let rows = vec![
        w::setting(
            p,
            "Install updates automatically",
            "While HudLight is in the tray, install new versions and restart by itself. With the window open, you're asked first.",
            w::toggle(p, "auto-update", "Install updates automatically", s.cfg.auto_update, |s| {
                let on = !s.cfg.auto_update;
                s.set_auto_update(on);
            }),
        ),
        w::setting(p, &format!("Version {}", crate::update::CURRENT), &status, action),
    ];
    w::card(p, "updates").child(w::card_header(p, "Updates", "Keep HudLight current", el())).child(w::divider(p)).child(w::rows(p, rows))
}

/// Release notes as plain lines: GitHub's generated Markdown without headings, bold markers,
/// author credits and the changelog link.
pub(super) fn plain_notes(notes: &str) -> String {
    notes
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("**Full Changelog**"))
        .map(|l| {
            let l = l.trim_start_matches('#').trim().replace("**", "");
            let l = match l.strip_prefix("* ").or_else(|| l.strip_prefix("- ")) {
                Some(item) => format!("•  {item}"),
                None => l,
            };
            // "Fix X by @someone in https://…/pull/3" → "Fix X".
            match l.find(" by @") {
                Some(i) if l[i..].contains(" in http") => l[..i].to_owned(),
                _ => l,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The update dialog, over the whole window.
pub fn update_dialog(s: &Ui, p: Pal) -> Option<El> {
    use crate::update::{CURRENT, UpdateState};
    let state = s.update.as_ref()?;
    let (title, lead) = match state {
        UpdateState::Available(a) | UpdateState::Failed { update: a, .. } => {
            ("Update available".to_owned(), format!("HudLight {} is ready to install. You have {CURRENT}.", a.version))
        }
        UpdateState::Downloading { version } => (format!("Updating to {version}"), "Downloading… HudLight restarts by itself when it's done.".to_owned()),
        UpdateState::Ready { version } => (format!("Updating to {version}"), "Installed. Restarting…".to_owned()),
        _ => return None,
    };
    let header = row()
        .w_full()
        .gap(14.0)
        .items_start()
        .child(el().size(40.0, 40.0).radius(20.0).center().fill(p.secondary).child(icon(&icons::DOWNLOAD, 20.0).color(p.secondary_text).a11y_hidden()))
        .child(col().grow(1.0).min_w(0.0).gap(4.0).child(text(title.clone()).font(w::CARD_TITLE).color(p.text)).child(text(lead).font(w::BODY).color(p.text2).wrap()));
    let mut sheet = col()
        .key("update-dialog")
        .role("dialog")
        .label(title)
        .w(460.0)
        .padding(24.0)
        .gap(18.0)
        .radius(14.0)
        .fill(p.card)
        .border(1.0, p.border)
        .shadow(0.0, 24.0, rune::Color::BLACK.alpha(0.25))
        .modal(true, |s: &mut Ui| s.dismiss_update())
        .child(header);

    if let Some(a) = state.available() {
        let notes = plain_notes(&a.notes);
        if !notes.is_empty() {
            sheet = sheet.child(
                col()
                    .key("update-notes")
                    .w_full()
                    .max_h(180.0)
                    .padding(14.0)
                    .radius(10.0)
                    .fill(p.muted)
                    .border(1.0, p.divider)
                    .scroll(rune::ScrollSpec { stick_bottom: false })
                    .child(text(notes).font(w::SMALL).color(p.text).wrap()),
            );
        }
        if let UpdateState::Failed { error, .. } = state {
            sheet = sheet.child(
                row()
                    .w_full()
                    .gap(8.0)
                    .items_start()
                    .child(icon(&icons::ALERT, 16.0).color(p.danger).a11y_hidden())
                    .child(text(format!("Update failed: {error}")).font(w::SMALL).color(p.danger).wrap().grow(1.0).min_w(0.0)),
            );
        }
        let url = a.url.clone();
        let link = (!url.is_empty()).then(|| {
            w::button_icon(p, "update-notes-link", "Release notes", Some(&icons::EXTERNAL), Variant::Ghost, move |_, tx| {
                let _ = tx.open_url(&url);
            })
        });
        let go = if matches!(state, UpdateState::Failed { .. }) { "Try again" } else { "Update now" };
        sheet = sheet.child(
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .children(link)
                .child(el().grow(1.0))
                .child(w::button(p, "update-later", "Later", Variant::Outline, |s, _| s.dismiss_update()))
                .child(w::button_icon(p, "update-now", go, Some(&icons::DOWNLOAD), Variant::Primary, |s, _| s.install_update())),
        );
    } else {
        // Downloading or restarting: nothing to choose, but the dialog can go.
        sheet = sheet.child(row().w_full().child(el().grow(1.0)).child(w::button(p, "update-hide", "Hide", Variant::Outline, |s, _| s.update_dialog = false)));
    }
    Some(el().key("update-scrim").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).fill(rune::Color::BLACK.alpha(0.4)).center().child(sheet))
}

// --- Help ----------------------------------------------------------------------------------

pub fn help(s: &Ui, p: Pal) -> El {
    let dir = crate::config::Config::dir();
    let about = w::card(p, "about").child(w::card_header(p, "Software", "", w::badge(p, Tone::Brand, &format!("v{}", env!("CARGO_PKG_VERSION"))))).child(w::card_body(
        col()
            .w_full()
            .child(w::kv(p, "Product", text("HudLight").font(w::BODY_MED).color(p.text)))
            .child(w::kv(p, "Version", w::mono(p, env!("CARGO_PKG_VERSION"))))
            .child(w::kv(p, "Built with", text("Rune (Rust, GPU)").font(w::BODY).color(p.text))),
    ));
    let steps = [
        "HudLight finds your VT DND Omega over USB (Kuando Busylight protocol) and keeps it awake.",
        "It watches your Microsoft Teams status, from the Teams app on this PC or from Microsoft Graph.",
        "Each status maps to a colour and effect you choose on the Colors page, matching Teams' own colours by default.",
    ];
    let mut how_list = col().w_full().gap(12.0);
    for (i, t) in steps.iter().enumerate() {
        how_list = how_list.child(row().gap(12.0).items_start().child(w::step(p, i + 1)).child(col().grow(1.0).min_w(0.0).pt(1.0).child(text(*t).font(w::BODY).color(p.text).wrap())));
    }
    let how = w::card(p, "how")
        .child(w::card_header(p, "How it works", "", el()))
        .child(w::card_body(how_list))
        .child(w::card_footer(p, w::note(p, "Closing the window keeps the light in sync from the tray. Right-click the tray icon to switch modes or quit."), el()));
    let devices = w::card(p, "supported").child(w::card_header(p, "Supported lights", "", el())).child(w::card_body(w::note(
        p,
        "VT DND Omega, Kuando Busylight Omega, Alpha, UC and Mini (USB vendor 27BB or 04D8:F848). Several lights at once all show the same colour.",
    )));
    let files = w::card(p, "files")
        .child(w::card_header(p, "Settings file", "Plain JSON; edit it while HudLight is closed.", el()))
        .child(w::card_body(col().w_full().child(w::code(p, dir.join("config.json").display().to_string()))))
        .child(w::card_footer(
            p,
            el(),
            w::button_icon(p, "open-folder", "Open folder", Some(&icons::FOLDER), Variant::Outline, move |s, _| {
                if !s.live() {
                    return;
                }
                let _ = std::fs::create_dir_all(crate::config::Config::dir());
                let _ = std::process::Command::new("explorer").arg(crate::config::Config::dir()).spawn();
            }),
        ));
    let _ = s;
    col()
        .key("help")
        .w_full()
        .gap(16.0)
        .child(w::page_header(p, "Help", "How HudLight works and where it keeps things.", el()))
        .child(about)
        .child(how)
        .child(devices)
        .child(files)
}
