//! What is open and where: the captures, their poses and sizes, and the
//! keyboard. Saved as the last session (`session.txt`, restored on start) and
//! as numbered presets (`preset1.txt` ...) in `%APPDATA%\Wander Overlay`.
//!
//! One record per line, fields separated by tabs:
//!
//! ```text
//! name     <preset name>                    (presets only)
//! keyboard <shown 0|1> <width m> <12 pose numbers>
//! overlay  <monitor|window> <scale> <width px> <height px> <opacity %> <style> <room|head|left|right> <12 pose numbers> <identity>
//! capture  <monitor|window> <scale> <width px> <height px> <12 pose numbers> <identity>
//! ```
//!
//! `style` adds 1 for curved and 2 for fading when turned away. `capture` is
//! the older form of `overlay`, still read: opaque, flat, in the room. The identity comes last because a window identity contains tabs
//! itself (see native/sources.h). The pixel size gives a placeholder the right
//! shape while its display or window is missing. A pose is relative to what
//! the overlay is attached to: the headset, a controller, or the room. Room
//! poses are in SteamVR's standing universe, so they only stay meaningful
//! while the play area is the same. Lines that do not parse are skipped.
use crate::{geometry::Matrix, settings};
use std::{fs, path::PathBuf};

pub const PRESETS: usize = 4;
/// Panel widths the scale gesture and saved layouts may produce, in metres.
pub const WIDTH_LIMITS: (f32, f32) = (0.15, 5.);
/// A capture's scale multiplies its natural width (source pixels / 960 m).
pub const SCALE_LIMITS: (f32, f32) = (0.1, 10.);

/// What an overlay is attached to and moves with.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Anchor {
    #[default]
    Room,
    Head,
    /// Left and right controller.
    Left,
    Right,
}
impl Anchor {
    pub fn name(self) -> &'static str {
        match self {
            Self::Room => "room",
            Self::Head => "head",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        [Self::Room, Self::Head, Self::Left, Self::Right]
            .into_iter()
            .find(|anchor| anchor.name() == name)
    }
    /// The hand (0 left, 1 right) this anchor follows, if it follows one.
    pub fn hand(self) -> Option<usize> {
        match self {
            Self::Left => Some(0),
            Self::Right => Some(1),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct SavedCapture {
    pub window: bool,
    pub identity: String,
    /// Relative to `anchor`.
    pub pose: Matrix,
    pub scale: f32,
    /// Capture size in pixels when saved; (0, 0) if no frame had arrived.
    pub size: (u32, u32),
    /// Percent.
    pub opacity: u32,
    pub curved: bool,
    /// Fades out as its front turns away from the viewer.
    pub fade: bool,
    pub anchor: Anchor,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    /// Preset name; empty for the session file and unnamed presets.
    pub name: String,
    pub keyboard_shown: bool,
    /// Where the keyboard was last put and how wide, once it has been shown.
    pub keyboard: Option<(Matrix, f32)>,
    pub captures: Vec<SavedCapture>,
}
/// A preset name as stored and shown: one line, no tabs, at most 40 characters.
pub fn clean_name(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    name.trim()
        .chars()
        .take(40)
        .collect::<String>()
        .trim_end()
        .into()
}
fn pose_text(pose: Matrix) -> String {
    pose.m.map(|v| v.to_string()).join(" ")
}
fn parse_pose(text: &str) -> Option<Matrix> {
    let mut m = [0f32; 12];
    let mut values = text.split(' ');
    for slot in &mut m {
        *slot = values.next()?.parse().ok()?;
    }
    let pose = Matrix { m };
    (values.next().is_none() && pose.rigid()).then_some(pose)
}
fn parse_size(text: &str, limits: (f32, f32)) -> Option<f32> {
    let value: f32 = text.parse().ok()?;
    value.is_finite().then(|| value.clamp(limits.0, limits.1))
}
impl Layout {
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        if !self.name.is_empty() {
            out += &format!("name\t{}\n", clean_name(&self.name));
        }
        if let Some((pose, width)) = self.keyboard {
            out += &format!(
                "keyboard\t{}\t{width}\t{}\n",
                self.keyboard_shown as u32,
                pose_text(pose)
            );
        }
        for c in &self.captures {
            out += &format!(
                "overlay\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                if c.window { "window" } else { "monitor" },
                c.scale,
                c.size.0,
                c.size.1,
                c.opacity,
                c.curved as u32 + 2 * c.fade as u32,
                c.anchor.name(),
                pose_text(c.pose),
                c.identity
            );
        }
        out
    }
    pub fn parse(text: &str) -> Self {
        let mut layout = Self::default();
        for line in text.lines() {
            let mut fields = line.splitn(7, '\t');
            let fields = [(); 6].map(|_| fields.next()).map(|f| f.unwrap_or(""));
            match fields[0] {
                "name" => layout.name = clean_name(line.split_once('\t').map_or("", |(_, n)| n)),
                "keyboard" => {
                    // The pose is the fourth field; nothing follows it.
                    if let (Some(width), Some(pose)) =
                        (parse_size(fields[2], WIDTH_LIMITS), parse_pose(fields[3]))
                    {
                        layout.keyboard = Some((pose, width));
                        layout.keyboard_shown = fields[1] == "1";
                    }
                }
                kind @ ("capture" | "overlay") => {
                    // `overlay` has three more fields before the pose.
                    let extra = if kind == "overlay" { 3 } else { 0 };
                    let rest: Vec<&str> = line.splitn(7 + extra, '\t').skip(5).collect();
                    let style = if extra == 0 {
                        Some((100, 0, Anchor::Room))
                    } else {
                        (|| {
                            let opacity = rest.first()?.parse().ok()?;
                            let style = rest.get(1)?.parse::<u32>().ok().filter(|s| *s < 4)?;
                            settings::OPACITIES.contains(&opacity).then_some(())?;
                            Some((opacity, style, Anchor::parse(rest.get(2)?)?))
                        })()
                    };
                    let identity = rest.get(extra + 1).copied().unwrap_or("");
                    if let (
                        Some(scale),
                        Ok(width),
                        Ok(height),
                        Some((opacity, style, anchor)),
                        Some(pose),
                        false,
                    ) = (
                        parse_size(fields[2], SCALE_LIMITS),
                        fields[3].parse::<u32>(),
                        fields[4].parse::<u32>(),
                        style,
                        rest.get(extra).and_then(|pose| parse_pose(pose)),
                        identity.is_empty() || !matches!(fields[1], "monitor" | "window"),
                    ) {
                        layout.captures.push(SavedCapture {
                            window: fields[1] == "window",
                            identity: identity.into(),
                            pose,
                            scale,
                            size: (width, height),
                            opacity,
                            curved: style & 1 != 0,
                            fade: style & 2 != 0,
                            anchor,
                        });
                    }
                }
                _ => {}
            }
        }
        layout
    }
    /// One line for the Presets screen, such as `2 overlays, keyboard: Notepad, DISPLAY1`.
    pub fn summary(&self) -> String {
        let names: Vec<&str> = self
            .captures
            .iter()
            .map(|c| {
                // A window's title, or a display's Windows name without `\\.\`.
                let name = c.identity.rsplit('\t').next().unwrap_or("");
                name.trim_start_matches(r"\\.\")
            })
            .collect();
        format!(
            "{} overlay{}{}{}{}",
            names.len(),
            if names.len() == 1 { "" } else { "s" },
            if self.keyboard_shown {
                ", keyboard"
            } else {
                ""
            },
            if names.is_empty() { "" } else { ": " },
            names.join(", ")
        )
    }
    pub fn load(path: Option<PathBuf>) -> Option<Self> {
        fs::read_to_string(path?)
            .ok()
            .map(|text| Self::parse(&text))
    }
    pub fn save(&self, path: Option<PathBuf>) -> Result<(), String> {
        settings::write_atomic(path.ok_or("APPDATA is not set")?, &self.serialize())
    }
}
pub fn session_path() -> Option<PathBuf> {
    Some(settings::data_dir()?.join("session.txt"))
}
/// `number` is 1-based, as shown on the Presets screen.
pub fn preset_path(number: usize) -> Option<PathBuf> {
    Some(settings::data_dir()?.join(format!("preset{number}.txt")))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Layout {
        let mut pose = Matrix::identity();
        pose.m[3] = 0.25;
        pose.m[11] = -1.5;
        Layout {
            name: "Stream: chat + OBS".into(),
            keyboard_shown: true,
            keyboard: Some((pose, 1.65)),
            captures: vec![
                SavedCapture {
                    window: false,
                    identity: "\\\\?\\DISPLAY#GSM5B09#5&2f#{e6f0}\t\\\\.\\DISPLAY1".into(),
                    pose,
                    scale: 1.,
                    size: (1920, 1080),
                    opacity: 100,
                    curved: false,
                    fade: false,
                    anchor: Anchor::Room,
                },
                SavedCapture {
                    window: true,
                    identity: "notepad.exe\tNotepad\tnotes.txt - Notepad".into(),
                    pose,
                    scale: 0.75,
                    size: (0, 0),
                    opacity: 45,
                    curved: true,
                    fade: true,
                    anchor: Anchor::Left,
                },
            ],
        }
    }
    #[test]
    fn round_trip_keeps_identities_with_tabs() {
        let layout = sample();
        assert_eq!(Layout::parse(&layout.serialize()), layout);
        assert_eq!(
            layout.summary(),
            "2 overlays, keyboard: DISPLAY1, notes.txt - Notepad"
        );
        assert_eq!(Layout::default().summary(), "0 overlays");
    }
    #[test]
    fn older_capture_lines_load_opaque_flat_and_in_the_room() {
        let pose = pose_text(Matrix::identity());
        let text = format!(
            "capture\twindow\t0.5\t800\t600\t{pose}\tapp.exe\tClass\tTitle\n\
             overlay\tmonitor\t1\t8\t8\t60\t1\thead\t{pose}\tDISPLAY2\n\
             overlay\tmonitor\t1\t8\t8\t5\t1\thead\t{pose}\tfaint\n\
             overlay\tmonitor\t1\t8\t8\t60\t4\thead\t{pose}\tbent\n\
             overlay\tmonitor\t1\t8\t8\t60\t1\tfoot\t{pose}\tfoot\n"
        );
        let captures = Layout::parse(&text).captures;
        assert_eq!(
            captures.len(),
            2,
            "bad opacity, shape and anchor are skipped"
        );
        let old = &captures[0];
        assert_eq!(old.identity, "app.exe\tClass\tTitle");
        assert_eq!(
            (old.opacity, old.curved, old.anchor),
            (100, false, Anchor::Room)
        );
        let new = &captures[1];
        assert_eq!(
            (new.opacity, new.curved, new.anchor, new.identity.as_str()),
            (60, true, Anchor::Head, "DISPLAY2")
        );
    }
    #[test]
    fn names_are_one_trimmed_line_of_at_most_40_characters() {
        assert_eq!(clean_name("  a\tb\nc  "), "a b c");
        assert_eq!(clean_name(&"x".repeat(50)).len(), 40);
        assert_eq!(clean_name("\t\n"), "");
        let named = Layout {
            name: "Work\tdesk".into(),
            ..Layout::default()
        };
        assert_eq!(Layout::parse(&named.serialize()).name, "Work desk");
    }
    #[test]
    fn damaged_lines_are_skipped_and_sizes_clamped() {
        let pose = pose_text(Matrix::identity());
        let skew = "2 0 0 0 0 1 0 0 0 0 1 0";
        let text = format!(
            "capture\twindow\t1\t8\t8\t{pose}\t\ncapture\ttv\t1\t8\t8\t{pose}\tx\ncapture\tmonitor\tNaN\t8\t8\t{pose}\tx\n\
             capture\tmonitor\t1\t8\t8\t{skew}\tx\ncapture\tmonitor\t1\t8\t8\t1 2 3\tx\ngarbage\n\
             capture\tmonitor\t1\t-8\t8\t{pose}\tx\ncapture\tmonitor\t1\t{pose}\told format\n\
             keyboard\t1\t99\t{pose}\ncapture\tmonitor\t500\t8\t8\t{pose}\tDISPLAY2\n"
        );
        let layout = Layout::parse(&text);
        assert_eq!(layout.captures.len(), 1);
        assert_eq!(layout.captures[0].scale, SCALE_LIMITS.1);
        assert_eq!(layout.keyboard, Some((Matrix::identity(), WIDTH_LIMITS.1)));
    }
}
