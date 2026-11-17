//! User settings, stored as `key=value` lines in
//! `%APPDATA%\Wander Overlay\settings.txt`. Unknown keys and invalid values are
//! ignored, so an old or damaged file falls back to defaults per setting.
use std::{fs, path::PathBuf};

pub const THEMES: [&str; 2] = ["dark", "light"];
/// Opacity of a capture panel in percent.
pub const OPACITIES: std::ops::RangeInclusive<u32> = 20..=100;
/// Size of a newly added capture, in percent of its natural size.
pub const SCALES: std::ops::RangeInclusive<u32> = 25..=300;
/// Distance of a newly added capture from the headset, in centimetres.
pub const DISTANCES: std::ops::RangeInclusive<u32> = 50..=400;
/// Pointer steadying strength in percent; 0 turns it off.
pub const SMOOTHINGS: std::ops::RangeInclusive<u32> = 0..=100;
/// Only the full US ANSI layout exists so far.
pub const KEYBOARD_LAYOUTS: [&str; 1] = ["full"];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub theme: String,
    /// New captures bend around the viewer instead of being flat.
    pub curved: bool,
    /// Opacity of new captures in percent.
    pub opacity: u32,
    pub clock_24h: bool,
    pub keyboard_layout: String,
    /// Default size of new captures in percent.
    pub scale: u32,
    /// Default distance of new captures in centimetres.
    pub distance: u32,
    /// Pointer steadying strength in percent.
    pub smoothing: u32,
    /// The keyboard, dashboard and wrist clock fade out as they are turned
    /// away from the viewer. Captures have this per overlay.
    pub fade_keyboard: bool,
    pub fade_dashboard: bool,
    pub fade_clock: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: THEMES[0].into(),
            curved: false,
            opacity: 100,
            clock_24h: true,
            keyboard_layout: KEYBOARD_LAYOUTS[0].into(),
            scale: 100,
            distance: 160,
            smoothing: 50,
            fade_keyboard: false,
            fade_dashboard: false,
            fade_clock: true,
        }
    }
}
impl Settings {
    /// Applies one setting. Returns false, changing nothing, when the key or
    /// value is not one this version offers.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        match key {
            "theme" if THEMES.contains(&value) => self.theme = value.into(),
            "curved" if value == "0" || value == "1" => self.curved = value == "1",
            "opacity" => match value.parse() {
                Ok(v) if OPACITIES.contains(&v) => self.opacity = v,
                _ => return false,
            },
            "scale" => match value.parse() {
                Ok(v) if SCALES.contains(&v) => self.scale = v,
                _ => return false,
            },
            "distance" => match value.parse() {
                Ok(v) if DISTANCES.contains(&v) => self.distance = v,
                _ => return false,
            },
            "smoothing" => match value.parse() {
                Ok(v) if SMOOTHINGS.contains(&v) => self.smoothing = v,
                _ => return false,
            },
            "fade_keyboard" | "fade_dashboard" | "fade_clock" if value == "0" || value == "1" => {
                *match key {
                    "fade_keyboard" => &mut self.fade_keyboard,
                    "fade_dashboard" => &mut self.fade_dashboard,
                    _ => &mut self.fade_clock,
                } = value == "1"
            }
            "clock" if value == "12" || value == "24" => self.clock_24h = value == "24",
            "keyboard" if KEYBOARD_LAYOUTS.contains(&value) => self.keyboard_layout = value.into(),
            _ => return false,
        }
        true
    }
    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=') {
                settings.set(key.trim(), value.trim());
            }
        }
        settings
    }
    pub fn serialize(&self) -> String {
        format!(
            "theme={}\ncurved={}\nopacity={}\nclock={}\nkeyboard={}\nscale={}\ndistance={}\nsmoothing={}\nfade_keyboard={}\nfade_dashboard={}\nfade_clock={}\n",
            self.theme,
            self.curved as u32,
            self.opacity,
            if self.clock_24h { 24 } else { 12 },
            self.keyboard_layout,
            self.scale,
            self.distance,
            self.smoothing,
            self.fade_keyboard as u32,
            self.fade_dashboard as u32,
            self.fade_clock as u32
        )
    }
    /// The same values as a JavaScript object literal for the HTML views.
    pub fn script(&self) -> String {
        format!(
            "{{theme:\"{}\",curved:{},opacity:{},clock:{},keyboard:\"{}\",scale:{},distance:{},smoothing:{},fade_keyboard:{},fade_dashboard:{},fade_clock:{}}}",
            self.theme,
            self.curved,
            self.opacity,
            if self.clock_24h { 24 } else { 12 },
            self.keyboard_layout,
            self.scale,
            self.distance,
            self.smoothing,
            self.fade_keyboard as u32,
            self.fade_dashboard as u32,
            self.fade_clock as u32
        )
    }
    fn path() -> Option<PathBuf> {
        Some(data_dir()?.join("settings.txt"))
    }
    pub fn load() -> Self {
        Self::path()
            .and_then(|path| fs::read_to_string(path).ok())
            .map_or_else(Self::default, |text| Self::parse(&text))
    }
    pub fn save(&self) -> Result<(), String> {
        write_atomic(Self::path().ok_or("APPDATA is not set")?, &self.serialize())
    }
    /// Clock text such as `14:05` or `2:05 PM`.
    pub fn time(&self, hour: u32, minute: u32) -> String {
        if self.clock_24h {
            format!("{hour:02}:{minute:02}")
        } else {
            let suffix = if hour < 12 { "AM" } else { "PM" };
            format!("{}:{minute:02} {suffix}", (hour + 11) % 12 + 1)
        }
    }
}
/// `%APPDATA%\Wander Overlay`, where settings, the last session and presets live.
pub fn data_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("Wander Overlay"))
}
/// Writes a temporary file and renames it over the old one, so a crash
/// mid-write cannot leave a truncated file.
pub fn write_atomic(path: PathBuf, text: &str) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    fs::create_dir_all(path.parent().unwrap())
        .and_then(|_| fs::write(&temporary, text))
        .and_then(|_| fs::rename(&temporary, &path))
        .map_err(|e| format!("Cannot save {}: {e}", path.display()))
}
/// Session length such as `42m` or `1h 05m`.
pub fn duration(seconds: u32) -> String {
    let minutes = seconds / 60;
    if minutes < 60 {
        format!("{minutes}m")
    } else {
        format!("{}h {:02}m", minutes / 60, minutes % 60)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_and_invalid_values_keep_defaults() {
        let mut s = Settings::default();
        assert!(s.set("theme", "light") && s.set("curved", "1") && s.set("opacity", "85"));
        assert!(s.set("clock", "12") && s.set("scale", "125") && s.set("distance", "200"));
        assert!(s.set("scale", "135") && s.set("distance", "275"));
        assert!(s.set("smoothing", "0") && s.set("fade_keyboard", "1"));
        assert!(s.set("fade_clock", "0") && s.fade_keyboard && !s.fade_clock);
        assert!(!s.set("smoothing", "101") && !s.set("fade_clock", "2"));
        assert!(!s.set("scale", "20") && !s.set("scale", "301") && !s.set("distance", "5"));
        assert_eq!(Settings::parse(&s.serialize()), s);
        assert!(!s.set("theme", "neon") && !s.set("opacity", "5") && !s.set("unknown", "1"));
        assert!(!s.set("keyboard", "compact"));
        assert_eq!(
            Settings::parse("theme=neon\nopacity=abc\ngarbage\nclock = 12\n"),
            Settings {
                clock_24h: false,
                ..Settings::default()
            }
        );
    }
    #[test]
    fn clock_and_session_text() {
        let mut s = Settings::default();
        assert_eq!(s.time(0, 5), "00:05");
        s.clock_24h = false;
        assert_eq!(
            [s.time(0, 5), s.time(12, 0), s.time(23, 59)],
            ["12:05 AM", "12:00 PM", "11:59 PM"]
        );
        assert_eq!(
            [duration(59), duration(3540), duration(3900)],
            ["0m", "59m", "1h 05m"]
        );
    }
}
