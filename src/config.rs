//! Settings the user customises, saved as JSON under `%APPDATA%\HudLight`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// An sRGB colour, 0–255 per channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const OFF: Rgb = Rgb(0, 0, 0);

    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }

    /// `#RRGGBB`, `RRGGBB` or `#RGB`.
    pub fn parse(s: &str) -> Option<Rgb> {
        let s = s.trim().trim_start_matches('#');
        let byte = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
        match s.len() {
            6 => Some(Rgb(byte(0)?, byte(2)?, byte(4)?)),
            3 => {
                let n = |i: usize| u8::from_str_radix(s.get(i..i + 1)?, 16).ok().map(|v| v * 17);
                Some(Rgb(n(0)?, n(1)?, n(2)?))
            }
            _ => None,
        }
    }

    pub fn get(self, channel: usize) -> u8 {
        [self.0, self.1, self.2][channel.min(2)]
    }

    pub fn with(self, channel: usize, v: u8) -> Rgb {
        let mut c = [self.0, self.1, self.2];
        c[channel.min(2)] = v;
        Rgb(c[0], c[1], c[2])
    }

    pub fn scale(self, f: f32) -> Rgb {
        let m = |v: u8| (f32::from(v) * f.clamp(0.0, 1.0)).round() as u8;
        Rgb(m(self.0), m(self.1), m(self.2))
    }

    pub fn is_off(self) -> bool {
        self == Rgb::OFF
    }
}

/// How the light shows a colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    Solid,
    Blink,
    Pulse,
    Off,
}

impl Effect {
    pub const ALL: [Effect; 4] = [Effect::Solid, Effect::Blink, Effect::Pulse, Effect::Off];

    pub fn label(self) -> &'static str {
        match self {
            Effect::Solid => "Solid",
            Effect::Blink => "Blink",
            Effect::Pulse => "Pulse",
            Effect::Off => "Off",
        }
    }
}

/// A Microsoft Teams presence, grouped the way Teams colours them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    Available,
    Busy,
    InCall,
    DoNotDisturb,
    Presenting,
    Away,
    BeRightBack,
    OutOfOffice,
    Offline,
    Unknown,
}

impl Status {
    pub const ALL: [Status; 10] = [
        Status::Available,
        Status::Busy,
        Status::InCall,
        Status::DoNotDisturb,
        Status::Presenting,
        Status::Away,
        Status::BeRightBack,
        Status::OutOfOffice,
        Status::Offline,
        Status::Unknown,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Status::Available => "Available",
            Status::Busy => "Busy",
            Status::InCall => "In a call or meeting",
            Status::DoNotDisturb => "Do not disturb",
            Status::Presenting => "Presenting",
            Status::Away => "Away",
            Status::BeRightBack => "Be right back",
            Status::OutOfOffice => "Out of office",
            Status::Offline => "Offline / Teams closed",
            Status::Unknown => "Unknown",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Status::Available => "available",
            Status::Busy => "busy",
            Status::InCall => "in-call",
            Status::DoNotDisturb => "dnd",
            Status::Presenting => "presenting",
            Status::Away => "away",
            Status::BeRightBack => "brb",
            Status::OutOfOffice => "ooo",
            Status::Offline => "offline",
            Status::Unknown => "unknown",
        }
    }

    /// Maps a Teams availability or activity string (from the Teams log or Microsoft Graph).
    pub fn from_teams(s: &str) -> Option<Status> {
        let k: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        Some(match k.as_str() {
            "available" | "availableidle" | "free" => Status::Available,
            "busy" | "busyidle" => Status::Busy,
            "inacall" | "inaconferencecall" | "onthephone" | "inameeting" | "incall" | "inmeeting" => Status::InCall,
            "donotdisturb" | "dnd" | "focusing" | "urgentinterruptionsonly" => Status::DoNotDisturb,
            "presenting" => Status::Presenting,
            "away" | "awayidle" | "inactive" => Status::Away,
            "berightback" | "brb" => Status::BeRightBack,
            "outofoffice" | "offwork" | "ooo" => Status::OutOfOffice,
            "offline" | "appearoffline" => Status::Offline,
            "presenceunknown" | "unknown" => Status::Unknown,
            _ => return None,
        })
    }

    /// The colour Microsoft Teams paints this presence with (Fluent presence palette).
    pub fn teams_color(self) -> Rgb {
        match self {
            Status::Available => Rgb(0x6B, 0xB7, 0x00),
            Status::Busy | Status::InCall | Status::DoNotDisturb | Status::Presenting => Rgb(0xC4, 0x31, 0x4B),
            Status::Away | Status::BeRightBack => Rgb(0xFF, 0xAA, 0x44),
            Status::OutOfOffice => Rgb(0xB4, 0x00, 0x9E),
            Status::Offline | Status::Unknown => Rgb(0x8A, 0x88, 0x86),
        }
    }

    /// Saturated colours that read clearly on an LED.
    pub fn vivid_color(self) -> Rgb {
        match self {
            Status::Available => Rgb(0x00, 0xFF, 0x00),
            Status::Busy | Status::InCall | Status::DoNotDisturb | Status::Presenting => Rgb(0xFF, 0x00, 0x00),
            Status::Away | Status::BeRightBack => Rgb(0xFF, 0xB0, 0x00),
            Status::OutOfOffice => Rgb(0xC0, 0x00, 0xFF),
            Status::Offline | Status::Unknown => Rgb(0x40, 0x40, 0x40),
        }
    }

    pub fn default_style(self) -> StatusStyle {
        let effect = match self {
            Status::Offline | Status::Unknown => Effect::Off,
            Status::DoNotDisturb | Status::Presenting => Effect::Solid,
            _ => Effect::Solid,
        };
        StatusStyle { color: self.teams_color(), effect }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusStyle {
    pub color: Rgb,
    pub effect: Effect,
}

/// What drives the light.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// Follow Microsoft Teams.
    Teams,
    /// A colour the user picked.
    Manual,
    Off,
}

/// Where Teams presence comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    /// Read the Teams desktop client's own log. No sign-in needed.
    TeamsClient,
    /// Ask Microsoft Graph (`/me/presence`) after a device-code sign-in.
    Graph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Appearance {
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub mode: Mode,
    pub source: Source,
    pub styles: Vec<(Status, StatusStyle)>,
    pub manual: StatusStyle,
    /// 0.05–1.0.
    pub brightness: f32,
    /// Blink and pulse period in seconds.
    pub blink_period: f32,
    pub pulse_period: f32,
    /// Correct for the LED's linear response so colours look like they do on screen.
    pub gamma_correction: bool,
    pub start_with_windows: bool,
    pub close_to_tray: bool,
    pub start_minimized: bool,
    pub appearance: Appearance,
    /// Azure app registration (public client) used for the Graph source.
    pub graph_client_id: String,
    pub graph_tenant: String,
    pub graph_poll_seconds: u32,
    /// Send colour channels as 0–100 instead of 0–255. Only for lights whose firmware reads
    /// percent; set by hand in config.json.
    pub pwm_100: bool,
    /// Download and install new releases from GitHub in the background.
    pub auto_update: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            mode: Mode::Teams,
            source: Source::TeamsClient,
            styles: Status::ALL.iter().map(|s| (*s, s.default_style())).collect(),
            manual: StatusStyle { color: Rgb(0x00, 0x90, 0xD9), effect: Effect::Solid },
            brightness: 1.0,
            blink_period: 1.0,
            pulse_period: 2.0,
            gamma_correction: true,
            start_with_windows: false,
            close_to_tray: true,
            start_minimized: false,
            appearance: Appearance::System,
            graph_client_id: String::new(),
            graph_tenant: "organizations".into(),
            graph_poll_seconds: 10,
            pwm_100: false,
            auto_update: true,
        }
    }
}

impl Config {
    pub fn style(&self, status: Status) -> StatusStyle {
        self.styles.iter().find(|(s, _)| *s == status).map(|(_, st)| *st).unwrap_or_else(|| status.default_style())
    }

    pub fn style_mut(&mut self, status: Status) -> &mut StatusStyle {
        if let Some(i) = self.styles.iter().position(|(s, _)| *s == status) {
            return &mut self.styles[i].1;
        }
        self.styles.push((status, status.default_style()));
        let last = self.styles.len() - 1;
        &mut self.styles[last].1
    }

    /// Fill in statuses a config from an older version didn't have.
    fn normalize(mut self) -> Self {
        for s in Status::ALL {
            let _ = self.style_mut(s);
        }
        self.brightness = self.brightness.clamp(0.05, 1.0);
        self.blink_period = self.blink_period.clamp(0.2, 3.0);
        self.pulse_period = self.pulse_period.clamp(0.6, 6.0);
        self.graph_poll_seconds = self.graph_poll_seconds.clamp(5, 300);
        self
    }

    pub fn dir() -> PathBuf {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        base.join("HudLight")
    }

    fn path() -> PathBuf {
        Self::dir().join("config.json")
    }

    pub fn load() -> Config {
        let Ok(text) = std::fs::read_to_string(Self::path()) else { return Config::default().normalize() };
        match serde_json::from_str::<Config>(&text) {
            Ok(c) => c.normalize(),
            Err(e) => {
                // Keep the user's customisation for them to fix, rather than saving over it.
                eprintln!("hudlight: config.json didn't parse ({e}); saved it as config.bad.json");
                let _ = std::fs::rename(Self::path(), Self::dir().join("config.bad.json"));
                Config::default().normalize()
            }
        }
    }

    pub fn save(&self) {
        let dir = Self::dir();
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            // Write then rename, so a crash mid-write never leaves a half file.
            let tmp = dir.join("config.json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, Self::path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        assert_eq!(Rgb::parse("#6BB700"), Some(Rgb(0x6B, 0xB7, 0x00)));
        assert_eq!(Rgb::parse("fa0"), Some(Rgb(0xFF, 0xAA, 0x00)));
        assert_eq!(Rgb::parse("#12345"), None);
        assert_eq!(Rgb(1, 2, 255).hex(), "#0102FF");
    }

    #[test]
    fn teams_strings_map_to_statuses() {
        assert_eq!(Status::from_teams("DoNotDisturb"), Some(Status::DoNotDisturb));
        assert_eq!(Status::from_teams("Be right back"), Some(Status::BeRightBack));
        assert_eq!(Status::from_teams("InAConferenceCall"), Some(Status::InCall));
        assert_eq!(Status::from_teams("NewActivity"), None);
    }

    #[test]
    fn old_configs_gain_new_statuses() {
        let c = Config { styles: vec![], ..Config::default() }.normalize();
        assert_eq!(c.styles.len(), Status::ALL.len());
    }
}
