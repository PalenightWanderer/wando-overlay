//! Dashboard and wrist-clock policy: which panels are shown, which capture
//! slots are open, and the validated commands the HTML views may send.
use crate::{
    geometry::Matrix,
    layout::{Anchor, PRESETS, clean_name},
    platform::{CAPTURES, Capture},
    settings::{OPACITIES, Settings},
};

/// One request from an HTML view. Views queue plain strings; anything that
/// does not parse to one of these is dropped.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Quit,
    /// Show or hide the dashboard panel (wrist button, dashboard close button).
    Dashboard,
    /// Show or hide the keyboard panel.
    Keyboard,
    /// List displays and windows again.
    Refresh,
    /// Open SteamVR's controller binding editor.
    Bindings,
    /// Capture a display (`window` false) or a window by OS handle.
    Open {
        window: bool,
        id: u64,
    },
    Close(usize),
    Set(String, String),
    /// Save, rename, load or delete preset 1..=PRESETS.
    Preset(PresetAction, usize),
    /// Change one open overlay, by capture slot.
    Overlay(usize, OverlaySet),
}
#[derive(Clone, Debug, PartialEq)]
pub enum OverlaySet {
    /// Percent.
    Opacity(u32),
    Curved(bool),
    /// Fade out as its front turns away from the viewer.
    Fade(bool),
    Anchor(Anchor),
    /// Show another display or window in the same place.
    Source {
        window: bool,
        id: u64,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum PresetAction {
    /// With the name typed in the row; empty keeps the preset's current name.
    Save(String),
    Rename(String),
    Load,
    Delete,
}
impl Command {
    pub fn parse(text: &str) -> Option<Self> {
        // `preset:<action>:<number>[:<name>]`. A name may contain colons.
        if let Some(rest) = text.strip_prefix("preset:") {
            let mut parts = rest.splitn(3, ':');
            let (action, number, name) = (parts.next()?, parts.next()?, parts.next());
            let number = number.parse().ok().filter(|n| (1..=PRESETS).contains(n))?;
            let action = match (action, name) {
                ("save", name) => PresetAction::Save(clean_name(name.unwrap_or(""))),
                ("rename", Some(name)) => PresetAction::Rename(clean_name(name)),
                ("load", None) => PresetAction::Load,
                ("delete", None) => PresetAction::Delete,
                _ => return None,
            };
            return Some(Self::Preset(action, number));
        }
        // `overlay:<slot>:<property>:<value>`.
        if let Some(rest) = text.strip_prefix("overlay:") {
            let parts: Vec<&str> = rest.split(':').collect();
            let [slot, property, value] = parts[..] else {
                return None;
            };
            let slot = slot.parse().ok().filter(|slot| *slot < CAPTURES)?;
            let set = match property {
                "opacity" => {
                    OverlaySet::Opacity(value.parse().ok().filter(|v| OPACITIES.contains(v))?)
                }
                "curved" if value == "0" || value == "1" => OverlaySet::Curved(value == "1"),
                "fade" if value == "0" || value == "1" => OverlaySet::Fade(value == "1"),
                "anchor" => OverlaySet::Anchor(Anchor::parse(value)?),
                "monitor" | "window" => OverlaySet::Source {
                    window: property == "window",
                    id: value.parse().ok().filter(|id| *id != 0)?,
                },
                _ => return None,
            };
            return Some(Self::Overlay(slot, set));
        }
        let mut parts = text.split(':');
        let command = match (parts.next()?, parts.next(), parts.next()) {
            ("quit", None, None) => Self::Quit,
            ("dashboard", None, None) => Self::Dashboard,
            ("keyboard", None, None) => Self::Keyboard,
            ("refresh", None, None) => Self::Refresh,
            ("bindings", None, None) => Self::Bindings,
            (kind @ ("monitor" | "window"), Some(id), None) => Self::Open {
                window: kind == "window",
                id: id.parse().ok().filter(|id| *id != 0)?,
            },
            ("close", Some(slot), None) => {
                Self::Close(slot.parse().ok().filter(|slot| *slot < CAPTURES)?)
            }
            ("set", Some(key), Some(value)) => Self::Set(key.into(), value.into()),
            _ => return None,
        };
        parts.next().is_none().then_some(command)
    }
    /// Commands queued by a view's `drainActions()`, one per line.
    pub fn parse_all(text: &str) -> Vec<Self> {
        text.lines().filter_map(Self::parse).collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Slot {
    pub window: bool,
    pub id: u64,
    /// The panel has a position: from the first captured frame, or restored.
    pub placed: bool,
    /// Panel width relative to the capture's natural width.
    pub scale: f32,
    /// Name that finds this source again after a restart (native/sources.h).
    pub identity: String,
    /// Capture size the panel was last sized for; (0, 0) before the first frame.
    pub size: (u32, u32),
    /// The source is missing: the panel shows a grey placeholder and the
    /// source is looked for again every few seconds.
    pub lost: bool,
    /// Percent.
    pub opacity: u32,
    pub curved: bool,
    pub fade: bool,
    pub anchor: Anchor,
    /// Width in metres before it was attached to a controller, which shrinks
    /// it; given back when it leaves the controller.
    pub free_width: Option<f32>,
    /// Pose relative to the headset or controller of `anchor`; unused in the room.
    pub relative: Matrix,
    /// Panel width in metres to keep when the first frame of a newly chosen
    /// source arrives, instead of sizing the panel from `scale`.
    pub keep_width: Option<f32>,
}
pub struct Shell {
    pub settings: Settings,
    pub slots: [Option<Slot>; CAPTURES],
    pub dashboard: bool,
    pub keyboard: bool,
    /// The keyboard has been shown at least once, so its pose is worth saving.
    pub keyboard_placed: bool,
    /// Name (possibly empty) and one-line summary per preset for the Presets
    /// screen; None where nothing is saved.
    pub presets: Vec<Option<(String, String)>>,
    /// Last failure worth showing on the dashboard, such as a rejected capture.
    pub notice: String,
    /// False in the diagnostic, whose scripted setting changes must not
    /// replace the user's saved settings.
    pub persist: bool,
}
impl Shell {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            slots: std::array::from_fn(|_| None),
            dashboard: false,
            keyboard: false,
            keyboard_placed: false,
            presets: vec![None; PRESETS],
            notice: String::new(),
            persist: true,
        }
    }
    pub fn free_slot(&self) -> Option<usize> {
        self.slots.iter().position(Option::is_none)
    }
    /// Slot already capturing this source, so a second click does not duplicate it.
    pub fn find(&self, window: bool, id: u64) -> Option<usize> {
        self.slots
            .iter()
            .position(|s| s.as_ref().is_some_and(|s| s.window == window && s.id == id))
    }
    /// Everything the dashboard shows besides the source list, as a
    /// `setState(...)` call. Source names are resolved by the page from ids.
    pub fn state_script(&self, captures: &[Capture; CAPTURES]) -> String {
        let list = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot, s)| {
                let s = s.as_ref()?;
                let c = captures[slot];
                Some(format!(
                    "{{slot:{slot},window:{},id:\"{}\",lost:{},ready:{},width:{},height:{},opacity:{},curved:{},fade:{},anchor:\"{}\"}}",
                    s.window,
                    s.id,
                    s.lost,
                    c.ready != 0,
                    c.width,
                    c.height,
                    s.opacity,
                    s.curved,
                    s.fade,
                    s.anchor.name()
                ))
            })
            .collect::<Vec<_>>()
            .join(",");
        let presets = self
            .presets
            .iter()
            .map(|p| {
                p.as_ref().map_or("null".into(), |(name, summary)| {
                    format!(
                        "{{name:{},summary:{}}}",
                        crate::keyboard::quote(name),
                        crate::keyboard::quote(summary)
                    )
                })
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "setState({{captures:[{list}],capacity:{CAPTURES},keyboard:{},settings:{},presets:[{presets}],notice:{}}})",
            self.keyboard,
            self.settings.script(),
            crate::keyboard::quote(&self.notice)
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_well_formed_commands_parse() {
        assert_eq!(
            Command::parse_all(
                "quit\nmonitor:65537\nwindow:42\nclose:7\nset:theme:light\nbindings\nrefresh\nkeyboard\ndashboard"
            ),
            vec![
                Command::Quit,
                Command::Open {
                    window: false,
                    id: 65537
                },
                Command::Open {
                    window: true,
                    id: 42
                },
                Command::Close(7),
                Command::Set("theme".into(), "light".into()),
                Command::Bindings,
                Command::Refresh,
                Command::Keyboard,
                Command::Dashboard,
            ]
        );
        for bad in [
            "",
            "quit:now",
            "monitor",
            "monitor:0",
            "monitor:-1",
            "window:abc",
            "close:8",
            "set:theme",
            "preset:save:0",
            "preset:save:5",
            "preset:rename:1",
            "preset:load",
            "preset:load:1:x",
            "preset:delete:2:x",
            "overlay:8:opacity:50",
            "overlay:0:opacity:5",
            "overlay:0:curved:2",
            "overlay:0:fade:yes",
            "overlay:0:anchor:foot",
            "overlay:0:monitor:0",
            "overlay:0:opacity",
            "overlay:0:opacity:50:1",
            "set:a:b:c",
            "launch:calc",
        ] {
            assert_eq!(Command::parse(bad), None, "{bad}");
        }
    }
    #[test]
    fn slots_and_state_script() {
        let mut shell = Shell::new(Settings::default());
        assert_eq!(shell.free_slot(), Some(0));
        shell.slots[0] = Some(Slot {
            window: true,
            id: 9,
            opacity: 70,
            anchor: Anchor::Head,
            ..Slot::default()
        });
        assert_eq!((shell.free_slot(), shell.find(true, 9)), (Some(1), Some(0)));
        assert_eq!(shell.find(false, 9), None);
        let mut captures = [Capture::default(); CAPTURES];
        captures[0] = Capture {
            open: 1,
            ready: 1,
            width: 800,
            height: 600,
        };
        let script = shell.state_script(&captures);
        assert!(
            script.contains(
                "{slot:0,window:true,id:\"9\",lost:false,ready:true,width:800,height:600,opacity:70,curved:false,fade:false,anchor:\"head\"}"
            )
        );
        assert!(script.contains("capacity:8,keyboard:false"));
        assert!(script.contains("presets:[null,null,null,null]"));
        shell.presets[1] = Some(("Work".into(), "1 overlay: \"Notes\"".into()));
        assert!(shell.state_script(&captures).contains(
            "presets:[null,{name:\"Work\",summary:\"1 overlay: \\\"Notes\\\"\"},null,null]"
        ));
        assert_eq!(
            Command::parse_all(
                "preset:save:1\npreset:save:3: Chat: 12:00 \npreset:rename:2:x\npreset:load:4\npreset:delete:2"
            ),
            vec![
                Command::Preset(PresetAction::Save(String::new()), 1),
                Command::Preset(PresetAction::Save("Chat: 12:00".into()), 3),
                Command::Preset(PresetAction::Rename("x".into()), 2),
                Command::Preset(PresetAction::Load, 4),
                Command::Preset(PresetAction::Delete, 2),
            ]
        );
        assert_eq!(
            Command::parse_all(
                "overlay:0:opacity:45\noverlay:7:curved:1\noverlay:2:anchor:left\noverlay:1:window:42"
            ),
            vec![
                Command::Overlay(0, OverlaySet::Opacity(45)),
                Command::Overlay(7, OverlaySet::Curved(true)),
                Command::Overlay(2, OverlaySet::Anchor(Anchor::Left)),
                Command::Overlay(
                    1,
                    OverlaySet::Source {
                        window: true,
                        id: 42
                    }
                ),
            ]
        );
    }
}
