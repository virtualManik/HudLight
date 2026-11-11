//! The USB HID protocols of the lights HudLight drives.
//!
//! Most lights speak Kuando's Busylight protocol (below). VT's own DND lights on vendor `340B`
//! (the VT DND Omega and Alpha) speak a much smaller one instead: see [`vt_report`].
//!
//! ## Kuando
//!
//! Every command is one 64-byte output report: seven 8-byte steps, then an 8-byte footer.
//!
//! ```text
//! step:   [opcode|operand, repeat, red, green, blue, on_time, off_time, audio]
//! footer: [sensitivity, timeout, trigger, 0xFF, 0xFF, 0xFF, checksum_hi, checksum_lo]
//! ```
//!
//! Opcode `0x1` jumps to step `operand` (a step that jumps to itself holds forever), `0x8` is a
//! keep-alive whose operand is the timeout in seconds. On and off times are tenths of a second;
//! both zero means a steady light. The checksum is the sum of bytes 0–61, big-endian. The light
//! turns itself off if it hears nothing for a while, so the host sends a keep-alive every 10 s.

use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};

use crate::config::Rgb;

/// Kuando/Plenom's vendor id, which VT's rebadged lights keep.
pub const VID_KUANDO: u16 = 0x27BB;
/// The original Busylight UC, on Microchip's vendor id.
pub const VID_MICROCHIP: u16 = 0x04D8;
/// VT (Xiamen VBeT), whose own DND lights use the VT protocol, not Kuando's.
pub const VID_VT: u16 = 0x340B;

pub const KNOWN: &[(u16, u16, &str)] = &[
    (VID_KUANDO, 0x3BC0, "Busylight Alpha"),
    (VID_KUANDO, 0x3BCA, "Busylight Alpha"),
    (VID_KUANDO, 0x3BCB, "Busylight UC Alpha"),
    (VID_KUANDO, 0x3BCC, "Busylight Mini"),
    (VID_KUANDO, 0x3BCD, "Busylight Omega"),
    (VID_KUANDO, 0x3BCE, "Busylight Omega"),
    (VID_KUANDO, 0x3BCF, "Busylight Omega"),
    (VID_MICROCHIP, 0xF848, "Busylight UC"),
    (VID_VT, 0xF001, "VT DND Omega"),
    (VID_VT, 0xF002, "VT DND Alpha"),
];

/// Which wire protocol a light speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    /// 64-byte reports with on-device blink and a keep-alive.
    Kuando,
    /// 5-byte reports; no blink timer, no keep-alive, and commands need a gap between them.
    Vt,
}

impl Protocol {
    fn of(vid: u16) -> Self {
        if vid == VID_VT { Protocol::Vt } else { Protocol::Kuando }
    }
}

const OP_JUMP: u8 = 0x10;
const OP_KEEP_ALIVE: u8 = 0x80;
/// Bit 7 of a step's audio byte: apply the tone bits (all zero, so the speaker stays quiet).
const AUDIO_UPDATE: u8 = 0x80;
pub const KEEP_ALIVE_SECONDS: u8 = 15;

/// A steady colour, or one that blinks on and off on the light's own clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub color: Rgb,
    /// Tenths of a second lit and dark; `(0, 0)` holds steady.
    pub on_time: u8,
    pub off_time: u8,
}

impl Command {
    pub fn solid(color: Rgb) -> Self {
        Command { color, on_time: 0, off_time: 0 }
    }

    pub fn off() -> Self {
        Self::solid(Rgb::OFF)
    }
}

fn finish(mut report: [u8; 64]) -> [u8; 64] {
    // Sensitivity, timeout and trigger as Kuando's own software sends them.
    report[56] = 0;
    report[57] = 0;
    report[58] = 0;
    report[59] = 0xFF;
    report[60] = 0xFF;
    report[61] = 0xFF;
    let sum: u16 = report[..62].iter().map(|&b| u16::from(b)).fold(0u16, u16::wrapping_add);
    report[62] = (sum >> 8) as u8;
    report[63] = (sum & 0xFF) as u8;
    report
}

/// `pwm_100` maps 0–255 onto 0–100, for firmware that reads channels as percent.
pub fn light_report_scaled(cmd: Command, pwm_100: bool) -> [u8; 64] {
    let ch = |v: u8| if pwm_100 { ((u16::from(v) * 100 + 127) / 255) as u8 } else { v };
    let mut r = [0u8; 64];
    // Step 0 jumps to itself: show this colour until told otherwise.
    r[0] = OP_JUMP;
    r[1] = 0;
    r[2] = ch(cmd.color.0);
    r[3] = ch(cmd.color.1);
    r[4] = ch(cmd.color.2);
    r[5] = cmd.on_time;
    r[6] = cmd.off_time;
    r[7] = AUDIO_UPDATE;
    finish(r)
}

pub fn keep_alive_report() -> [u8; 64] {
    let mut r = [0u8; 64];
    r[0] = OP_KEEP_ALIVE | (KEEP_ALIVE_SECONDS & 0x0F);
    finish(r)
}

// VT DND protocol: one 5-byte output report per command, `[0x01, action, d0, d1, d2]`, where
// 0x01 is the HID report id. Set colour takes R, G, B (0–255); brightness takes a level 1–3
// that the light keeps across reconnects. A command written too soon after another may be
// dropped, so commands are spaced by `VT_GAP`.
const VT_REPORT_ID: u8 = 0x01;
const VT_OFF: u8 = 0x01;
const VT_SET_COLOR: u8 = 0x04;
const VT_SET_BRIGHTNESS: u8 = 0x09;
const VT_BRIGHTNESS_HIGH: u8 = 3;
pub const VT_GAP: Duration = Duration::from_millis(100);

/// The VT report for a command. VT lights can't blink on their own, so `on_time`/`off_time`
/// are ignored here; the engine blinks them by sending on and off.
pub fn vt_report(cmd: Command) -> [u8; 5] {
    let Rgb(r, g, b) = cmd.color;
    if cmd.color.is_off() { [VT_REPORT_ID, VT_OFF, 0, 0, 0] } else { [VT_REPORT_ID, VT_SET_COLOR, r, g, b] }
}

/// Full hardware brightness, so the app's own brightness setting covers the whole range.
pub fn vt_brightness_report() -> [u8; 5] {
    [VT_REPORT_ID, VT_SET_BRIGHTNESS, VT_BRIGHTNESS_HIGH, 0, 0]
}

/// Linear-light correction: LEDs respond linearly, screens don't, so a mid value would look
/// far too bright on the light without it.
pub fn degamma(c: Rgb) -> Rgb {
    let f = |v: u8| {
        if v == 0 {
            return 0;
        }
        let x = f32::from(v) / 255.0;
        let lin = if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) };
        // Never round a lit channel down to dark.
        ((lin * 255.0).round() as u8).max(1)
    };
    Rgb(f(c.0), f(c.1), f(c.2))
}

/// What the UI shows about a connected light.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub product: String,
    pub manufacturer: String,
    pub model: &'static str,
    pub vid: u16,
    pub pid: u16,
    pub release: u16,
    pub serial: String,
}

struct Light {
    info: DeviceInfo,
    path: std::ffi::CString,
    dev: HidDevice,
    protocol: Protocol,
    /// VT only: when the last report went out, and a command waiting for the gap to pass.
    last_write: Instant,
    pending: Option<Command>,
}

impl Light {
    fn write_kuando(&self, report: &[u8; 64]) -> bool {
        // hidapi wants the report id first; these lights use none, so it's 0.
        let mut buf = [0u8; 65];
        buf[1..].copy_from_slice(report);
        self.dev.write(&buf).is_ok()
    }

    fn write_vt(&mut self, report: &[u8; 5]) -> bool {
        self.last_write = Instant::now();
        // The first byte is already the report id, as hidapi expects.
        self.dev.write(report).is_ok()
    }

    /// Shows `cmd`, or queues it if a VT light heard from us too recently. False when the
    /// light has gone away.
    fn show(&mut self, cmd: Command, pwm_100: bool) -> bool {
        match self.protocol {
            Protocol::Kuando => self.write_kuando(&light_report_scaled(cmd, pwm_100)),
            Protocol::Vt => {
                if self.last_write.elapsed() < VT_GAP {
                    self.pending = Some(cmd);
                    return true;
                }
                self.pending = None;
                self.write_vt(&vt_report(cmd))
            }
        }
    }

    /// Sends a queued VT command once its gap has passed.
    fn flush(&mut self) -> bool {
        match self.pending {
            Some(cmd) if self.last_write.elapsed() >= VT_GAP => {
                self.pending = None;
                self.write_vt(&vt_report(cmd))
            }
            _ => true,
        }
    }
}

/// Every connected Busylight, opened.
pub struct Lights {
    api: Option<HidApi>,
    lights: Vec<Light>,
}

fn model_of(vid: u16, pid: u16, product: &str) -> Option<&'static str> {
    if let Some((_, _, name)) = KNOWN.iter().find(|(v, p, _)| *v == vid && *p == pid) {
        return Some(name);
    }
    let p = product.to_ascii_lowercase();
    (vid == VID_KUANDO || p.contains("busylight") || p.contains("dnd omega")).then_some("Busylight")
}

impl Lights {
    pub fn new() -> Self {
        Lights { api: HidApi::new().ok(), lights: Vec::new() }
    }

    pub fn infos(&self) -> Vec<DeviceInfo> {
        self.lights.iter().map(|l| l.info.clone()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.lights.is_empty()
    }

    /// Opens lights plugged in since the last scan. True when the set changed.
    pub fn scan(&mut self) -> bool {
        let Some(api) = self.api.as_mut() else {
            self.api = HidApi::new().ok();
            return false;
        };
        if api.refresh_devices().is_err() {
            return false;
        }
        let present: Vec<std::ffi::CString> = api.device_list().map(|d| d.path().to_owned()).collect();
        let before = self.lights.len();
        self.lights.retain(|l| present.contains(&l.path));
        let mut changed = self.lights.len() != before;
        for d in api.device_list() {
            let product = d.product_string().unwrap_or_default().to_owned();
            let Some(model) = model_of(d.vendor_id(), d.product_id(), &product) else { continue };
            let serial = d.serial_number().unwrap_or_default().to_owned();
            let same_light = |l: &Light| {
                l.path.as_c_str() == d.path()
                    || (l.info.vid == d.vendor_id() && l.info.pid == d.product_id() && !serial.is_empty() && l.info.serial == serial)
            };
            if self.lights.iter().any(same_light) {
                continue;
            }
            let Ok(dev) = d.open_device(api) else { continue };
            // Windows lists one entry per HID collection; keep the one that takes reports.
            let protocol = Protocol::of(d.vendor_id());
            let probed = match protocol {
                Protocol::Kuando => {
                    let mut buf = [0u8; 65];
                    buf[1..].copy_from_slice(&keep_alive_report());
                    dev.write(&buf).is_ok()
                }
                Protocol::Vt => dev.write(&vt_brightness_report()).is_ok(),
            };
            if !probed {
                if std::env::var_os("HUDLIGHT_DEBUG").is_some() {
                    eprintln!("[busylight] {model} {:04X}:{:04X} rejected a {protocol:?} report", d.vendor_id(), d.product_id());
                }
                continue;
            }
            let info = DeviceInfo {
                product: if product.is_empty() { model.to_owned() } else { product },
                manufacturer: d.manufacturer_string().unwrap_or_default().to_owned(),
                model,
                vid: d.vendor_id(),
                pid: d.product_id(),
                release: d.release_number(),
                serial,
            };
            // The VT probe set the brightness, so the first colour waits out the gap.
            let light = Light { info, path: d.path().to_owned(), dev, protocol, last_write: Instant::now(), pending: None };
            self.lights.push(light);
            changed = true;
        }
        changed
    }

    /// True when every light can blink on its own clock; otherwise the engine blinks them.
    pub fn hardware_blink(&self) -> bool {
        self.lights.iter().all(|l| l.protocol == Protocol::Kuando)
    }

    /// Shows a command on every light, dropping the ones that went away. True when one did.
    pub fn send(&mut self, cmd: Command, pwm_100: bool) -> bool {
        let before = self.lights.len();
        self.lights.retain_mut(|l| l.show(cmd, pwm_100));
        self.lights.len() != before
    }

    /// Sends Kuando keep-alives (when `keep_alive`) and VT commands that were waiting out
    /// their gap. Call every tick. True when a light went away.
    pub fn service(&mut self, keep_alive: bool) -> bool {
        let before = self.lights.len();
        self.lights.retain_mut(|l| match l.protocol {
            Protocol::Kuando => !keep_alive || l.write_kuando(&keep_alive_report()),
            Protocol::Vt => l.flush(),
        });
        self.lights.len() != before
    }

    /// Turns every light off, waiting out VT command gaps so the last word is "off".
    pub fn turn_off(&mut self) {
        self.send(Command::off(), false);
        if self.lights.iter().any(|l| l.pending.is_some()) {
            std::thread::sleep(VT_GAP);
            self.service(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_solid_report_has_a_self_jump_and_a_checksum() {
        let r = light_report_scaled(Command::solid(Rgb(255, 0, 10)), false);
        assert_eq!(&r[..8], &[0x10, 0, 255, 0, 10, 0, 0, 0x80]);
        assert_eq!(&r[59..62], &[0xFF; 3]);
        let sum: u32 = r[..62].iter().map(|&b| u32::from(b)).sum();
        assert_eq!(u32::from(r[62]) << 8 | u32::from(r[63]), sum);
    }

    #[test]
    fn percent_scaling_maps_full_to_100() {
        let r = light_report_scaled(Command::solid(Rgb(255, 128, 0)), true);
        assert_eq!(&r[2..5], &[100, 50, 0]);
    }

    #[test]
    fn vt_reports_match_the_dnd_protocol() {
        assert_eq!(vt_report(Command::solid(Rgb(10, 20, 30))), [0x01, 0x04, 10, 20, 30]);
        assert_eq!(vt_report(Command::off()), [0x01, 0x01, 0, 0, 0]);
        assert_eq!(vt_brightness_report(), [0x01, 0x09, 0x03, 0, 0]);
        assert_eq!(Protocol::of(0x340B), Protocol::Vt);
        assert_eq!(model_of(0x340B, 0xF001, ""), Some("VT DND Omega"));
    }

    #[test]
    fn keep_alive_carries_its_timeout() {
        assert_eq!(keep_alive_report()[0], 0x8F);
    }

    #[test]
    fn degamma_keeps_ends_and_darkens_middles() {
        assert_eq!(degamma(Rgb(0, 255, 128)), Rgb(0, 255, 55));
        assert_eq!(degamma(Rgb(1, 0, 0)).0, 1);
    }
}
