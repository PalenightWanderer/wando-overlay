use crate::{
    keyboard::{ALT, CTRL, META, SHIFT},
    platform::{Input, Key, Platform},
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Target {
    #[default]
    Windows,
    Local,
}
pub trait Sink {
    fn inject(&mut self, event: Input) -> bool;
    fn local(&mut self, key: Key) -> bool;
}
impl Sink for Platform {
    fn inject(&mut self, event: Input) -> bool {
        Platform::inject(self, event)
    }
    fn local(&mut self, key: Key) -> bool {
        self.local_key(key)
    }
}
#[derive(Default)]
pub struct OwnedInput {
    pub target: Target,
    pub blocked: bool,
    /// Last injected cursor position (capture slot, x, y); unchanged positions are not resent.
    pub last_move: Option<(u32, i32, i32)>,
    owned: Vec<Input>,
    /// Modifier bits a hand holds down on the overlay keyboard right now.
    held: u32,
    /// Of those, the ones pressed in Windows and left down between
    /// keystrokes, so Alt codes and Win shortcuts see one continuous hold.
    sustained: u32,
}
const MODIFIER_KEYS: [(u32, u32); 4] = [(CTRL, 162), (ALT, 164), (META, 91), (SHIFT, 160)];
impl OwnedInput {
    pub fn send(&mut self, sink: &mut impl Sink, event: Input) -> bool {
        if self.blocked && event.down != 0 {
            return false;
        }
        if !sink.inject(event) {
            self.blocked = true;
            return false;
        }
        if event.kind == 0 || event.kind == 1 || event.kind == 4 {
            if event.down != 0 {
                self.owned.push(Input { down: 0, ..event });
            } else {
                self.owned
                    .retain(|e| !(e.kind == event.kind && e.code == event.code));
            }
        }
        true
    }
    /// Tells which modifiers are held on the overlay keyboard, and lets go in
    /// Windows of the ones no longer held.
    pub fn hold(&mut self, sink: &mut impl Sink, held: u32) {
        self.held = held;
        for (bit, vk) in MODIFIER_KEYS {
            if self.sustained & bit != 0 && held & bit == 0 {
                self.sustained &= !bit;
                self.send(
                    sink,
                    Input {
                        code: vk,
                        ..Input::default()
                    },
                );
            }
        }
    }
    pub fn release(&mut self, sink: &mut impl Sink) {
        self.last_move = None;
        self.sustained = 0;
        // Retry only known successful downs. Never release unrelated OS input.
        for _ in 0..3 {
            for event in self.owned.clone().into_iter().rev() {
                self.send(sink, event);
            }
            if self.owned.is_empty() {
                break;
            }
        }
    }
    pub fn pending(&self) -> bool {
        !self.owned.is_empty()
    }
    pub fn key(&mut self, sink: &mut impl Sink, k: Key) -> bool {
        if self.target == Target::Local {
            return sink.local(k);
        }
        if self.blocked {
            return false;
        }
        // Printable ANSI text uses Unicode injection so OS layout/Caps state
        // cannot turn a displayed US key into another glyph. Shortcuts, Space
        // (layout independent, often a hotkey) and editing keys use VKs.
        let unicode = k.character > 32 && k.modifiers & (ALT | CTRL | META) == 0;
        if !unicode {
            for (bit, vk) in MODIFIER_KEYS {
                if k.modifiers & bit != 0
                    && self.sustained & bit == 0
                    && !self.send(
                        sink,
                        Input {
                            code: vk,
                            down: 1,
                            ..Input::default()
                        },
                    )
                {
                    self.release(sink);
                    return false;
                }
                self.sustained |= k.modifiers & bit & self.held;
            }
        }
        let down = Input {
            kind: if unicode { 4 } else { 0 },
            code: if unicode { k.character } else { k.vk },
            down: 1,
            extended: (!unicode && matches!(k.vk, 33..=40 | 45 | 46 | 91..=93 | 163 | 165)) as u32,
            ..Input::default()
        };
        let success = self.send(sink, down);
        if success {
            self.send(sink, Input { down: 0, ..down });
        }
        // A successful key transaction cannot release another hand's mouse drag.
        // On rejection the caller cancels the session; full release is appropriate.
        if self.blocked {
            self.release(sink);
        } else {
            let sustained = self.sustained;
            let keys: Vec<_> = self
                .owned
                .iter()
                .copied()
                .filter(|e| {
                    e.kind != 1
                        && !(e.kind == 0
                            && MODIFIER_KEYS
                                .iter()
                                .any(|(bit, vk)| sustained & bit != 0 && *vk == e.code))
                })
                .collect();
            for release in keys.into_iter().rev() {
                self.send(sink, release);
            }
            if self.blocked {
                self.release(sink);
            }
        }
        success && !self.blocked
    }
}
#[cfg(test)]
pub mod fake {
    use super::*;
    #[derive(Default)]
    pub struct Fake {
        pub events: Vec<Input>,
        pub local: Vec<Key>,
        pub fail: Option<usize>,
    }
    impl Sink for Fake {
        fn inject(&mut self, e: Input) -> bool {
            self.events.push(e);
            self.fail != Some(self.events.len())
        }
        fn local(&mut self, k: Key) -> bool {
            self.local.push(k);
            true
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use fake::Fake;
    #[test]
    fn local_context_isolation() {
        let mut input = OwnedInput {
            target: Target::Local,
            ..Default::default()
        };
        let mut sink = Fake::default();
        input.key(
            &mut sink,
            Key {
                vk: 65,
                character: 97,
                modifiers: 0,
            },
        );
        assert!(sink.events.is_empty());
        assert_eq!(sink.local.len(), 1);
    }
    #[test]
    fn partial_injection_failure_releases_successful_downs() {
        let mut input = OwnedInput::default();
        let mut sink = Fake {
            fail: Some(3),
            ..Default::default()
        };
        assert!(!input.key(
            &mut sink,
            Key {
                vk: 65,
                modifiers: CTRL | META,
                character: 97
            }
        ));
        assert!(input.blocked);
        assert!(!input.pending());
        assert_eq!(
            sink.events
                .iter()
                .filter(|e| e.down == 0)
                .map(|e| e.code)
                .collect::<Vec<_>>(),
            vec![91, 162]
        );
    }
    #[test]
    fn failed_up_retains_ownership_for_bounded_retry() {
        let mut input = OwnedInput::default();
        let mut sink = Fake {
            fail: Some(2),
            ..Default::default()
        };
        input.key(
            &mut sink,
            Key {
                vk: 65,
                character: 97,
                modifiers: 0,
            },
        );
        assert!(input.blocked);
        assert!(!input.pending());
        assert_eq!(sink.events.len(), 3);
        assert_eq!(sink.events[2].down, 0);
    }
    #[test]
    fn shift_navigation_and_meta_tap_are_virtual() {
        let mut i = OwnedInput::default();
        let mut s = Fake::default();
        i.key(
            &mut s,
            Key {
                vk: 37,
                modifiers: SHIFT,
                character: 0,
            },
        );
        assert_eq!(s.events[0].code, 160);
        assert_eq!(s.events[1].code, 37);
        assert_eq!(s.events[1].extended, 1);
        s.events.clear();
        i.key(
            &mut s,
            Key {
                vk: 91,
                modifiers: 0,
                character: 0,
            },
        );
        assert_eq!(s.events.len(), 2);
    }
    #[test]
    fn space_and_menu_are_virtual_keys() {
        let mut i = OwnedInput::default();
        let mut s = Fake::default();
        i.key(
            &mut s,
            Key {
                vk: 32,
                modifiers: 0,
                character: 32,
            },
        );
        assert_eq!((s.events[0].kind, s.events[0].code), (0, 32));
        s.events.clear();
        i.key(
            &mut s,
            Key {
                vk: 93,
                modifiers: 0,
                character: 0,
            },
        );
        assert_eq!((s.events[0].code, s.events[0].extended), (93, 1));
        s.events.clear();
        i.key(
            &mut s,
            Key {
                vk: 186,
                modifiers: SHIFT,
                character: ':' as u32,
            },
        );
        assert_eq!(
            (s.events.len(), s.events[0].kind, s.events[0].code),
            (2, 4, ':' as u32)
        );
    }
    #[test]
    fn typing_preserves_other_hand_mouse_drag() {
        let mut i = OwnedInput::default();
        let mut s = Fake::default();
        let mouse = Input {
            kind: 1,
            code: 0,
            down: 1,
            ..Default::default()
        };
        assert!(i.send(&mut s, mouse));
        assert!(i.key(
            &mut s,
            Key {
                vk: 65,
                character: 97,
                modifiers: 0
            }
        ));
        assert!(i.pending());
        assert!(!s.events.iter().any(|e| e.kind == 1 && e.down == 0));
        i.send(&mut s, Input { down: 0, ..mouse });
        assert!(!i.pending());
    }
    #[test]
    fn held_modifier_stays_down_between_keystrokes_until_let_go() {
        let (mut input, mut sink) = (OwnedInput::default(), fake::Fake::default());
        let pad = |vk| Key {
            vk,
            modifiers: ALT,
            character: 0,
        };
        input.hold(&mut sink, ALT);
        assert!(input.key(&mut sink, pad(97)) && input.key(&mut sink, pad(98)));
        let sent: Vec<_> = sink.events.iter().map(|e| (e.code, e.down)).collect();
        assert_eq!(sent, [(164, 1), (97, 1), (97, 0), (98, 1), (98, 0)]);
        input.hold(&mut sink, 0);
        assert_eq!(sink.events.last().map(|e| (e.code, e.down)), Some((164, 0)));
        assert!(!input.pending());
        // Not held: the modifier wraps the single keystroke as before.
        input.key(&mut sink, pad(99));
        let sent: Vec<_> = sink.events[6..].iter().map(|e| (e.code, e.down)).collect();
        assert_eq!(sent, [(164, 1), (99, 1), (99, 0), (164, 0)]);
    }
}
