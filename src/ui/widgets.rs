//! The window's look: shadcn/ui components (cards, switches, tabs, badges) in the T3 Chat
//! theme from tweakcn, and the controls the pages share.

use rune::tok::motion;
use rune::{Align, Color, Cursor, Drag, El, Family, Phase, TextEdit, TextStyle, col, el, icon, row, text, text_input};

use super::Ui;
use crate::config::Rgb;

pub const H1: TextStyle = TextStyle::new(22.0, 600.0).tracking(-0.45);
pub const LEAD: TextStyle = TextStyle::new(13.5, 400.0).lh(20.0);
pub const CARD_TITLE: TextStyle = TextStyle::new(15.0, 600.0).tracking(-0.2);
pub const BODY: TextStyle = TextStyle::new(13.5, 400.0).lh(20.0);
pub const BODY_MED: TextStyle = TextStyle::new(13.5, 500.0);
pub const SMALL: TextStyle = TextStyle::new(12.5, 400.0).lh(18.0);
pub const SMALL_MED: TextStyle = TextStyle::new(12.0, 500.0);
pub const LABEL: TextStyle = TextStyle::new(11.5, 500.0).tracking(0.2);
pub const MONO: TextStyle = TextStyle::new(12.5, 400.0).family(Family::Named("Consolas, Cascadia Mono, Courier New"));
pub const BIG: TextStyle = TextStyle::new(21.0, 600.0).tracking(-0.4);
pub const BRAND: TextStyle = TextStyle::new(15.0, 600.0).tracking(-0.25);

/// The theme's tokens, light or dark (tweakcn "T3 Chat", converted from OKLCH).
#[derive(Clone, Copy)]
pub struct Pal {
    pub dark: bool,
    /// The page behind the cards.
    pub bg: Color,
    pub sidebar: Color,
    pub card: Color,
    pub text: Color,
    /// shadcn's `muted-foreground`.
    pub text2: Color,
    pub border: Color,
    /// Lines inside a card, a step softer than its edge.
    pub divider: Color,
    pub input: Color,
    pub primary: Color,
    pub primary_text: Color,
    pub secondary: Color,
    pub secondary_text: Color,
    pub muted: Color,
    pub ring: Color,
    pub hover: Color,
    pub press: Color,
    pub nav_active: Color,
    pub nav_active_text: Color,
    pub tab_active: Color,
    pub track: Color,
    pub knob: Color,
    pub ok: Color,
    pub warn: Color,
    pub danger: Color,
    pub shadow: Color,
}

/// `#RRGGBB` (sRGB) to Rune's linear colour.
pub fn c(hex: &str) -> Color {
    Rgb::parse(hex).map_or(Color::BLACK, rgb)
}

pub fn rgb(v: Rgb) -> Color {
    let lin = |b: u8| {
        let x = f32::from(b) / 255.0;
        if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
    };
    Color { r: lin(v.0), g: lin(v.1), b: lin(v.2), a: 1.0 }
}

impl Pal {
    pub fn new(dark: bool) -> Pal {
        if dark {
            Pal {
                dark,
                bg: c("#221D27"),
                sidebar: c("#181117"),
                card: c("#2C2632"),
                text: c("#E4D9EE"),
                text2: c("#A797B5"),
                border: c("#3B3237"),
                divider: c("#352D38"),
                input: c("#463C47"),
                primary: c("#A3004C"),
                primary_text: c("#F6D7E7"),
                secondary: c("#362D3D"),
                secondary_text: c("#D8CDE4"),
                muted: c("#1F1A24"),
                ring: c("#DB2777"),
                hover: Color::WHITE.alpha(0.05),
                press: Color::WHITE.alpha(0.09),
                nav_active: c("#2E1D29"),
                nav_active_text: c("#F8F1F5"),
                tab_active: c("#463753"),
                track: c("#3E343C"),
                knob: c("#F8F1F5"),
                ok: c("#4ADE80"),
                warn: c("#FBBF24"),
                danger: c("#F87171"),
                shadow: Color::BLACK.alpha(0.35),
            }
        } else {
            Pal {
                dark,
                bg: c("#FAF5FA"),
                sidebar: c("#F3E4F6"),
                card: c("#FFFFFF"),
                text: c("#501854"),
                text2: c("#834588"),
                border: c("#EFBDEB"),
                divider: c("#F6DDF3"),
                input: c("#E7C1DC"),
                primary: c("#A84370"),
                primary_text: c("#FFFFFF"),
                secondary: c("#F1C4E6"),
                secondary_text: c("#77347C"),
                muted: c("#FBF2F9"),
                ring: c("#DB2777"),
                hover: c("#501854").alpha(0.05),
                press: c("#501854").alpha(0.09),
                nav_active: c("#EBCBEA"),
                nav_active_text: c("#AC1668"),
                tab_active: c("#FFFFFF"),
                track: c("#F3DCEF"),
                knob: c("#FFFFFF"),
                ok: c("#15803D"),
                warn: c("#B45309"),
                danger: c("#AB4347"),
                shadow: c("#501854").alpha(0.08),
            }
        }
    }

    fn focus(&self) -> Color {
        self.ring.alpha(0.45)
    }
}

// --- Frame ---------------------------------------------------------------------------------

/// A sidebar link: icon and label, tinted when it's the current page.
pub fn nav_item(p: Pal, key: &'static str, label: &'static str, glyph: &'static rune::Icon, active: bool, go: impl Fn(&mut Ui) + 'static) -> El {
    let ink = if active { p.nav_active_text } else { p.text };
    let mut item = row()
        .key(key)
        .role("tab")
        .label(label)
        .checked(active)
        .focusable(true)
        .focus_ring(2.0, 0.0, p.focus())
        .w_full()
        .h(34.0)
        .px(10.0)
        .gap(10.0)
        .center_y()
        .radius(8.0)
        .cursor(Cursor::Pointer)
        .on_press(go)
        .child(icon(glyph, 17.0).color(if active { p.nav_active_text } else { p.text2 }).a11y_hidden())
        .child(text(label).font(BODY_MED).color(ink).a11y_hidden());
    item = if active { item.fill(p.nav_active) } else { item.hover(p.hover).press(p.press) };
    item
}

/// The page title and its one-line description, with optional actions on the right.
pub fn page_header(p: Pal, title: &str, desc: &str, actions: El) -> El {
    row()
        .key("header")
        .w_full()
        .pb(8.0)
        .gap(16.0)
        .items_end()
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(4.0)
                .child(text(title.to_owned()).font(H1).color(p.text))
                .child(text(desc.to_owned()).font(LEAD).color(p.text2).wrap()),
        )
        .child(actions)
}

// --- Cards ---------------------------------------------------------------------------------

/// A shadcn card: rounded, a hairline edge and a soft shadow. Add a `card_header`, a
/// `card_body` and maybe a `card_footer`.
pub fn card(p: Pal, key: impl Into<String>) -> El {
    col()
        .key(key.into())
        .w_full()
        .fill(p.card)
        .radius(12.0)
        .border(1.0, p.border)
        .padding(1.0)
        .shadow(1.0, 3.0, p.shadow)
        .shadow(1.0, 2.0, p.shadow)
        .clip_rounded(12.0)
}

pub fn card_header(p: Pal, title: &str, desc: &str, trailing: El) -> El {
    let mut words = col().grow(1.0).min_w(0.0).gap(3.0).child(text(title.to_owned()).font(CARD_TITLE).color(p.text));
    if !desc.is_empty() {
        words = words.child(text(desc.to_owned()).font(SMALL).color(p.text2).wrap());
    }
    row().w_full().px(20.0).pt(18.0).pb(14.0).gap(12.0).items_start().child(words).child(trailing)
}

pub fn card_body(content: El) -> El {
    col().w_full().px(20.0).pb(20.0).gap(14.0).child(content)
}

/// Vercel's card footer: a tinted strip with a hint on the left and the action on the right.
pub fn card_footer(p: Pal, hint: El, action: El) -> El {
    row()
        .w_full()
        .px(20.0)
        .py(12.0)
        .gap(16.0)
        .center_y()
        .fill(p.muted)
        .border_sides([1.0, 0.0, 0.0, 0.0], p.divider)
        .child(col().grow(1.0).min_w(0.0).child(hint))
        .child(action)
}

pub fn divider(p: Pal) -> El {
    el().w_full().h(1.0).fill(p.divider).a11y_hidden()
}

/// Settings-form rows split by dividers.
pub fn rows(p: Pal, items: Vec<El>) -> El {
    let mut c = col().w_full();
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            c = c.child(divider(p));
        }
        c = c.child(item);
    }
    c
}

/// A labelled setting: name and help text on the left, its control on the right.
pub fn setting(p: Pal, name: &str, help: &str, control: El) -> El {
    let mut words = col().grow(1.0).min_w(0.0).gap(2.0).child(text(name.to_owned()).font(BODY_MED).color(p.text));
    if !help.is_empty() {
        words = words.child(text(help.to_owned()).font(SMALL).color(p.text2).wrap());
    }
    row().w_full().px(20.0).py(14.0).gap(20.0).center_y().child(words).child(control)
}

/// One `label  value` line of a description list.
pub fn kv(p: Pal, label: &str, value: El) -> El {
    row().w_full().min_h(30.0).gap(12.0).center_y().child(text(label.to_owned()).font(SMALL).color(p.text2).w(108.0)).child(col().grow(1.0).min_w(0.0).child(value))
}

pub fn mono(p: Pal, s: impl Into<String>) -> El {
    text(s.into()).font(MONO).color(p.text).truncate()
}

/// Code in a tinted, rounded box.
pub fn code(p: Pal, s: impl Into<String>) -> El {
    row()
        .px(10.0)
        .py(7.0)
        .radius(7.0)
        .fill(if p.dark { p.muted } else { c("#F8ECF6") })
        .border(1.0, p.divider)
        .child(text(s.into()).font(MONO).color(p.text).truncate().selectable(true))
}

pub fn note(p: Pal, s: impl Into<String>) -> El {
    text(s.into()).font(SMALL).color(p.text2).wrap()
}

// --- Controls ------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Primary,
    Outline,
    Ghost,
}

pub fn button(p: Pal, key: &'static str, label: &str, v: Variant, on: impl Fn(&mut Ui, &mut rune::Tx) + 'static) -> El {
    button_icon(p, key, label, None, v, on)
}

/// A shadcn button, optionally with a leading icon.
pub fn button_icon(p: Pal, key: &'static str, label: &str, glyph: Option<&'static rune::Icon>, v: Variant, on: impl Fn(&mut Ui, &mut rune::Tx) + 'static) -> El {
    let ink = match v {
        Variant::Primary => p.primary_text,
        _ => p.text,
    };
    let mut b = row()
        .key(key)
        .role("button")
        .label(label)
        .focusable(true)
        .focus_ring(3.0, 0.0, p.focus())
        .h(34.0)
        .px(if glyph.is_some() { 12.0 } else { 14.0 })
        .gap(7.0)
        .center()
        .radius(8.0)
        .cursor(Cursor::Pointer)
        .on_press_tx(on);
    b = match v {
        Variant::Primary => b.fill(p.primary).hover(Color::WHITE.alpha(0.08)).press(Color::BLACK.alpha(0.08)).shadow(1.0, 2.0, p.shadow),
        Variant::Outline => b.fill(p.card).border(1.0, p.input).hover(p.hover).press(p.press).shadow(1.0, 2.0, p.shadow),
        Variant::Ghost => b.hover(p.hover).press(p.press),
    };
    if let Some(g) = glyph {
        b = b.child(icon(g, 15.0).color(ink).a11y_hidden());
    }
    b.child(text(label.to_owned()).font(BODY_MED).color(ink).a11y_hidden())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Warn,
    Neutral,
    Brand,
}

/// A rounded status pill with a dot.
pub fn badge(p: Pal, tone: Tone, label: &str) -> El {
    let (ink, bg, edge) = match tone {
        Tone::Ok => (p.ok, p.ok.alpha(if p.dark { 0.12 } else { 0.09 }), p.ok.alpha(0.25)),
        Tone::Warn => (p.warn, p.warn.alpha(if p.dark { 0.12 } else { 0.09 }), p.warn.alpha(0.28)),
        Tone::Neutral => (p.text2, Color::TRANSPARENT, p.border),
        Tone::Brand => (p.secondary_text, p.secondary.alpha(if p.dark { 1.0 } else { 0.6 }), Color::TRANSPARENT),
    };
    row()
        .h(24.0)
        .px(10.0)
        .gap(6.0)
        .center_y()
        .radius(12.0)
        .fill(bg)
        .border(1.0, edge)
        .child(el().size(6.0, 6.0).radius(3.0).fill(ink).a11y_hidden())
        .child(text(label.to_owned()).font(SMALL_MED).color(ink))
}

/// A shadcn switch.
pub fn toggle(p: Pal, key: &'static str, label: &str, on: bool, flip: impl Fn(&mut Ui) + 'static) -> El {
    let (w, h, k) = (38.0, 22.0, 18.0);
    let inset = (h - k) / 2.0;
    el().key(key)
        .role("switch")
        .label(label)
        .checked(on)
        .focusable(true)
        .focus_ring(3.0, 0.0, p.focus())
        .size(w, h)
        .radius(h / 2.0)
        .fill(if on { p.primary } else { p.input })
        .cursor(Cursor::Pointer)
        .motion(motion::SNAPPY)
        .on_press(flip)
        .child(
            el().key("knob")
                .size(k, k)
                .absolute(if on { w - k - inset } else { inset }, inset)
                .radius(k / 2.0)
                .fill(p.knob)
                .shadow(1.0, 3.0, Color::BLACK.alpha(0.18))
                .a11y_hidden(),
        )
}

/// shadcn Tabs: a tinted track with a raised, sliding active tab.
pub fn segmented<T: Copy + PartialEq + 'static>(
    p: Pal,
    key: &'static str,
    label: &str,
    items: &[(T, &'static str)],
    current: T,
    seg_w: f32,
    pick: impl Fn(&mut Ui, T) + Clone + 'static,
) -> El {
    let (h, pad) = (34.0, 3.0);
    let idx = items.iter().position(|(v, _)| *v == current).unwrap_or(0);
    let mut segs = row();
    for (i, (v, name)) in items.iter().copied().enumerate() {
        let selected = i == idx;
        let pick = pick.clone();
        segs = segs.child(
            el().key(name)
                .role("radio")
                .label(name)
                .checked(selected)
                .focusable(true)
                .focus_ring(2.0, 0.0, p.focus())
                .size(seg_w, h - 2.0 * pad)
                .center()
                .radius(6.0)
                .cursor(Cursor::Pointer)
                .on_press(move |s: &mut Ui| pick(s, v))
                .child(text(name).font(BODY_MED).color(if selected { p.text } else { p.text2 }).a11y_hidden()),
        );
    }
    el().key(key)
        .role("radiogroup")
        .label(label)
        .size(seg_w * items.len() as f32 + 2.0 * pad, h)
        .padding(pad)
        .radius(9.0)
        .fill(if p.dark { p.muted } else { c("#F6E5F3") })
        .motion(motion::SNAPPY)
        .child(
            el().key("indicator")
                .size(seg_w, h - 2.0 * pad)
                .absolute(pad + idx as f32 * seg_w, pad)
                .radius(6.0)
                .fill(p.tab_active)
                .shadow(1.0, 3.0, p.shadow)
                .shadow(1.0, 1.0, p.shadow)
                .lead(motion::LEAD, motion::DEFAULT)
                .a11y_hidden(),
        )
        .child(segs)
}

/// A horizontal slider over `0..=1`. `get` reads the value a drag starts from.
pub fn slider(
    p: Pal,
    key: &'static str,
    label: &str,
    v: f32,
    value_text: String,
    fill: Color,
    get: impl Fn(&Ui) -> f32 + 'static,
    set: impl Fn(&mut Ui, f32) + Clone + 'static,
) -> El {
    let (w, h, k, bar) = (220.0, 22.0, 18.0, 6.0);
    let travel = w - k;
    let to_value = move |x: f32| ((x - k / 2.0) / travel).clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let step_set = set.clone();
    row()
        .key(key)
        .gap(12.0)
        .center_y()
        .child(
            el().key("slider")
                .role("slider")
                .label(label)
                .range_value(f64::from(v), 0.0, 1.0)
                .range_step(0.05)
                .a11y_value(value_text.clone())
                .focusable(true)
                .focus_ring(3.0, 0.0, p.focus())
                .radius(h / 2.0)
                .on_step(move |s: &mut Ui, steps, _| {
                    let now = get(s);
                    step_set(s, (now + steps as f32 * 0.05).clamp(0.0, 1.0));
                })
                .size(w, h)
                .cursor(Cursor::Pointer)
                .motion(motion::SNAPPY)
                .on_drag(move |s: &mut Ui, d: &mut Drag, tx| match d.phase {
                    Phase::Down => {
                        d.data = v;
                        let knob = k / 2.0 + v * travel;
                        if (d.x - knob).abs() > k / 2.0 + 2.0 {
                            set(s, to_value(d.x));
                        }
                    }
                    Phase::Move => {
                        let knob = k / 2.0 + d.data * travel;
                        let grab = if (d.start_x - knob).abs() <= k / 2.0 + 2.0 { d.start_x - knob } else { 0.0 };
                        set(s, to_value(d.x - grab));
                        tx.instant();
                    }
                    Phase::Up => {
                        if d.cancelled() {
                            set(s, d.data);
                        }
                    }
                })
                .child(el().key("track").size(w, bar).absolute(0.0, (h - bar) / 2.0).radius(bar / 2.0).fill(p.track).a11y_hidden())
                .child(
                    el().key("fill")
                        .size(k / 2.0 + v * travel, bar)
                        .absolute(0.0, (h - bar) / 2.0)
                        .radius(bar / 2.0)
                        .fill(fill)
                        .a11y_hidden(),
                )
                .child(
                    el().key("knob")
                        .size(k, k)
                        .absolute(v * travel, (h - k) / 2.0)
                        .radius(k / 2.0)
                        .fill(p.knob)
                        .border(2.0, fill)
                        .shadow(1.0, 3.0, Color::BLACK.alpha(0.15))
                        .a11y_hidden(),
                ),
        )
        .child(text(value_text).key("value").w(48.0).font(MONO).color(p.text2).align(Align::End).a11y_hidden())
}

/// A shadcn input bound to a `TextEdit` in `Ui`.
pub fn field(
    p: Pal,
    key: &'static str,
    label: &str,
    edit: &TextEdit,
    placeholder: &str,
    w: f32,
    access: impl for<'a> Fn(&'a mut Ui) -> &'a mut TextEdit + 'static,
) -> El {
    el().key(key)
        .w(w)
        .h(34.0)
        .px(11.0)
        .radius(8.0)
        .fill(if p.dark { p.muted } else { p.card })
        .border(1.0, p.input)
        .shadow(1.0, 2.0, p.shadow)
        .focus_within_ring(3.0, 0.0, p.focus())
        .center_y()
        .cursor(Cursor::Text)
        .child(
            text_input(edit, BODY)
                .key("input")
                .label(label)
                .single_line()
                .w_full()
                .color(p.text)
                .placeholder(placeholder.to_owned(), p.text2.alpha(0.7))
                .on_edit(access),
        )
}

/// A labelled form field: the label above, the input below.
pub fn form_field(p: Pal, label: &str, input: El) -> El {
    col().gap(7.0).child(text(label.to_owned()).font(BODY_MED).color(p.text)).child(input)
}

/// A round colour chip; pressing it picks the colour. The picked one gets a ring.
pub fn swatch(p: Pal, key: String, color: Rgb, selected: bool, d: f32, pick: impl Fn(&mut Ui) + 'static) -> El {
    let mut s = el()
        .key(key)
        .role("radio")
        .label(color.hex())
        .checked(selected)
        .focusable(true)
        .focus_ring(2.0, 2.0, p.focus())
        .size(d, d)
        .radius(d / 2.0)
        .fill(rgb(color))
        .border(1.0, Color::BLACK.alpha(0.12))
        .cursor(Cursor::Pointer)
        .on_press(pick);
    if selected {
        s = s.outline(2.0, 2.0, p.text);
    }
    s
}

/// The light, drawn: a glowing disc in the colour it shows, or a dim one when off.
pub fn orb(p: Pal, key: &'static str, color: Rgb, lit: bool, d: f32) -> El {
    let base = el().key(key).size(d, d).radius(d / 2.0).a11y_hidden().motion(motion::CALM);
    if lit && !color.is_off() {
        let c = rgb(color);
        base.fill(c).shadow(0.0, d * 0.5, c.alpha(0.45)).shadow(0.0, d * 0.14, c.alpha(0.7)).border(1.0, Color::WHITE.alpha(0.3))
    } else {
        base.fill(if p.dark { c("#3A3140") } else { c("#EADAEA") }).border(1.0, p.border)
    }
}

/// A numbered step marker.
pub fn step(p: Pal, n: usize) -> El {
    el().size(22.0, 22.0).radius(11.0).center().fill(p.secondary).child(text(n.to_string()).font(SMALL_MED).color(p.secondary_text))
}

/// The app icon at `pt` points, decoded once per device-pixel size so it's drawn sharp.
pub fn logo(pt: f32, scale: f32) -> El {
    use std::cell::RefCell;
    use std::collections::HashMap;
    thread_local! {
        static CACHE: RefCell<HashMap<u32, rune::Pixels>> = RefCell::new(HashMap::new());
    }
    let px = ((pt * scale).round() as u32).clamp(16, 256);
    let pixels = CACHE.with_borrow_mut(|cache| {
        cache
            .entry(px)
            .or_insert_with(|| {
                let p = rune::Pixels::new();
                if let Ok(img) = image::load_from_memory(include_bytes!("../../assets/icon-256.png")) {
                    let img = image::imageops::resize(&img.into_rgba8(), px, px, image::imageops::FilterType::Lanczos3);
                    p.set(px, px, img.into_raw());
                }
                p
            })
            .clone()
    });
    el().key("logo").size(pt, pt).pixels(&pixels).a11y_hidden()
}
