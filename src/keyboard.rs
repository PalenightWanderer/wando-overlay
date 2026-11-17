use crate::platform::Key;
pub const ALT: u32 = 1;
pub const CTRL: u32 = 2;
pub const META: u32 = 4;
pub const SHIFT: u32 = 8;
/// Second Shift release within this window (no other key between) locks Shift.
/// Matches the Windows default double-click time.
pub const DOUBLE_PRESS_MS: u64 = 500;
/// A held key starts repeating after this delay. Matches the Windows default.
pub const REPEAT_DELAY_MS: u64 = 500;
/// Time between repeats, about 30 per second like the Windows default rate.
pub const REPEAT_INTERVAL_MS: u64 = 33;
/// Blocks of the keyboard, left to right, and how many grid columns each has.
/// The main block counts quarter key widths, the others whole keys.
pub const MAIN: u32 = 0;
pub const NAV: u32 = 1;
pub const PAD: u32 = 2;
#[derive(Clone, Debug)]
pub struct Definition {
    pub vk: u32,
    pub block: u32,
    /// Grid cell, from 0, and how many columns and rows the key covers.
    pub row: u32,
    pub column: u32,
    pub columns: u32,
    pub rows: u32,
    /// Empty for a key that shows the character it types.
    pub label: String,
    pub base: char,
    pub shifted: char,
}
pub fn layout() -> Vec<Definition> {
    let mut keys = Vec::new();
    // Next free column per block and row.
    let mut next = [[0u32; 6]; 3];
    let mut place = |vk, block: u32, row: u32, columns, rows, label: &str, base, shifted| {
        let column = &mut next[block as usize][row as usize];
        // `vk` 0 only leaves a gap.
        if vk != 0 {
            keys.push(Definition {
                vk,
                block,
                row,
                column: *column,
                columns,
                rows,
                label: label.into(),
                base,
                shifted,
            });
        }
        *column += columns;
    };
    // Main block: widths in key units, placed left to right.
    let mut add = |vk, row, units: f32, label: &str, base, shifted| {
        place(vk, MAIN, row, (units * 4.) as u32, 1, label, base, shifted)
    };
    add(27, 0, 1., "esc", '\0', '\0');
    for i in 0..12 {
        // The function keys stand in three groups of four.
        add(
            0,
            0,
            if i == 0 {
                1.
            } else if i % 4 == 0 {
                0.5
            } else {
                0.
            },
            "",
            '\0',
            '\0',
        );
        add(112 + i, 0, 1., &format!("F{}", i + 1), '\0', '\0');
    }
    for (vk, b, s) in [
        (192, '`', '~'),
        (49, '1', '!'),
        (50, '2', '@'),
        (51, '3', '#'),
        (52, '4', '$'),
        (53, '5', '%'),
        (54, '6', '^'),
        (55, '7', '&'),
        (56, '8', '*'),
        (57, '9', '('),
        (48, '0', ')'),
        (189, '-', '_'),
        (187, '=', '+'),
    ] {
        add(vk, 1, 1., "", b, s);
    }
    add(8, 1, 2., "\u{232b}", '\0', '\0');
    add(9, 2, 1.5, "\u{21e5}", '\t', '\t');
    for c in "qwertyuiop".chars() {
        add(
            c.to_ascii_uppercase() as u32,
            2,
            1.,
            "",
            c,
            c.to_ascii_uppercase(),
        );
    }
    add(219, 2, 1., "", '[', '{');
    add(221, 2, 1., "", ']', '}');
    add(220, 2, 1.5, "", '\\', '|');
    add(20, 3, 1.75, "caps", '\0', '\0');
    for c in "asdfghjkl".chars() {
        add(
            c.to_ascii_uppercase() as u32,
            3,
            1.,
            "",
            c,
            c.to_ascii_uppercase(),
        );
    }
    add(186, 3, 1., "", ';', ':');
    add(222, 3, 1., "", '\'', '"');
    add(13, 3, 2.25, "\u{21b5}", '\r', '\r');
    add(160, 4, 2.25, "\u{21e7}", '\0', '\0');
    for c in "zxcvbnm".chars() {
        add(
            c.to_ascii_uppercase() as u32,
            4,
            1.,
            "",
            c,
            c.to_ascii_uppercase(),
        );
    }
    add(188, 4, 1., "", ',', '<');
    add(190, 4, 1., "", '.', '>');
    add(191, 4, 1., "", '/', '?');
    add(161, 4, 2.75, "\u{21e7}", '\0', '\0');
    for (vk, u, name) in [
        (162, 1.25, "ctrl"),
        (91, 1.25, "win"),
        (164, 1.25, "alt"),
        (32, 6.25, ""),
        (165, 1.25, "alt"),
        (92, 1.25, "win"),
        (93, 1.25, "menu"),
        (163, 1.25, "ctrl"),
    ] {
        let c = if vk == 32 { ' ' } else { '\0' };
        add(vk, 5, u, name, c, c);
    }
    // Navigation block, three keys wide. Zero leaves a gap.
    for (row, vks) in [
        (0, [(44, "prt"), (145, "scr"), (19, "pause")]),
        (1, [(45, "ins"), (36, "home"), (33, "pgup")]),
        (2, [(46, "del"), (35, "end"), (34, "pgdn")]),
        (4, [(0, ""), (38, "\u{2191}"), (0, "")]),
        (5, [(37, "\u{2190}"), (40, "\u{2193}"), (39, "\u{2192}")]),
    ] {
        for (vk, name) in vks {
            place(vk, NAV, row, 1, 1, name, '\0', '\0');
        }
    }
    // Number pad, four keys wide. Plus and Enter are two rows tall; Enter is
    // the same key as in the main block.
    for (row, pad) in [
        (
            1,
            vec![
                (144, "num", '\0', 1, 1),
                (111, "", '/', 1, 1),
                (106, "", '*', 1, 1),
                (109, "", '-', 1, 1),
            ],
        ),
        (
            2,
            vec![
                (103, "", '7', 1, 1),
                (104, "", '8', 1, 1),
                (105, "", '9', 1, 1),
                (107, "", '+', 1, 2),
            ],
        ),
        (
            3,
            vec![
                (100, "", '4', 1, 1),
                (101, "", '5', 1, 1),
                (102, "", '6', 1, 1),
            ],
        ),
        (
            4,
            vec![
                (97, "", '1', 1, 1),
                (98, "", '2', 1, 1),
                (99, "", '3', 1, 1),
                (13, "\u{21b5}", '\r', 1, 2),
            ],
        ),
        (5, vec![(96, "", '0', 2, 1), (110, "", '.', 1, 1)]),
    ] {
        for (vk, name, c, columns, rows) in pad {
            place(vk, PAD, row, columns, rows, name, c, c);
        }
    }
    keys
}
pub fn definition(vk: u32) -> Option<Definition> {
    layout().into_iter().find(|k| k.vk == vk)
}
pub fn modifier(vk: u32) -> u32 {
    match vk {
        160 | 161 => SHIFT,
        162 | 163 => CTRL,
        164 | 165 => ALT,
        91 | 92 => META,
        _ => 0,
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ShiftState {
    #[default]
    Off,
    Once,
    Locked,
}
#[derive(Clone, Copy, Default)]
struct Held {
    vk: u32,
    used: bool,
    /// For a key typed on press: that keystroke and when it is typed again.
    repeat: Option<(Key, u64)>,
}
#[derive(Default)]
pub struct Keyboard {
    pub shift: ShiftState,
    pub caps: bool,
    pub once: u32,
    last_shift: Option<u64>,
    held: [Option<Held>; 2],
}
impl Keyboard {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn cancel(&mut self, hand: usize) {
        self.held[hand] = None;
    }
    /// Presses `vk` with `hand` and returns the keystroke it types right away.
    /// Every key except the modifiers acts on press, like a hardware keyboard,
    /// and repeats while held. A modifier acts on release, because only then
    /// is a tap told apart from a hold.
    pub fn down(&mut self, hand: usize, vk: u32, now: u64) -> Option<Key> {
        if definition(vk).is_none() || self.held[hand].is_some() {
            return None;
        }
        let mut held = Held {
            vk,
            ..Held::default()
        };
        let mut key = None;
        if vk == 20 {
            self.caps = !self.caps;
        } else if modifier(vk) == 0 {
            key = self.typed(vk);
            held.repeat = key.map(|key| (key, now + REPEAT_DELAY_MS));
        }
        self.held[hand] = Some(held);
        key
    }
    pub fn held_modifiers(&self) -> u32 {
        self.held
            .iter()
            .flatten()
            .fold(0, |a, h| a | modifier(h.vk))
    }
    pub fn modifiers(&self) -> u32 {
        self.once
            | self.held_modifiers()
            | if self.shift != ShiftState::Off {
                SHIFT
            } else {
                0
            }
    }
    pub fn up(&mut self, hand: usize, vk: u32, now: u64) -> Option<Key> {
        let held = self.held[hand]?;
        if held.vk != vk {
            return None;
        }
        self.held[hand] = None;
        let m = modifier(vk);
        if m != 0 {
            if held.used {
                return None;
            }
            match m {
                SHIFT => {
                    self.shift = match self.shift {
                        ShiftState::Locked => ShiftState::Off,
                        ShiftState::Once
                            if self
                                .last_shift
                                .is_some_and(|t| now.saturating_sub(t) <= DOUBLE_PRESS_MS) =>
                        {
                            ShiftState::Locked
                        }
                        ShiftState::Once => ShiftState::Off,
                        ShiftState::Off => ShiftState::Once,
                    };
                    self.last_shift = Some(now);
                }
                META => {
                    return Some(Key {
                        vk,
                        modifiers: 0,
                        character: 0,
                    });
                }
                _ => self.once ^= m,
            }
            return None;
        }
        // Every other key was typed when it was pressed.
        None
    }
    /// The keystroke for a plain key: marks held modifiers as used and consumes
    /// the one-shot modifiers.
    fn typed(&mut self, vk: u32) -> Option<Key> {
        let mods = self.modifiers();
        for held in self.held.iter_mut().flatten() {
            if modifier(held.vk) != 0 {
                held.used = true;
            }
        }
        self.last_shift = None;
        let k = definition(vk)?;
        let shift = mods & SHIFT != 0;
        let shifted = if k.base.is_ascii_alphabetic() {
            shift ^ self.caps
        } else {
            shift
        };
        let character = if shifted { k.shifted } else { k.base } as u32;
        self.once = 0;
        if self.shift == ShiftState::Once {
            self.shift = ShiftState::Off;
        }
        Some(Key {
            vk,
            modifiers: mods,
            character,
        })
    }
    fn repeat(&mut self, hand: usize, now: u64) -> Option<Key> {
        let held = self.held[hand]?;
        let (key, _) = held.repeat.filter(|(_, due)| now >= *due)?;
        // Every repeat is the keystroke typed on press again, so one-shot
        // modifiers it consumed still apply. Missed repeats are not made up.
        self.held[hand] = Some(Held {
            repeat: Some((key, now + REPEAT_INTERVAL_MS)),
            ..held
        });
        Some(key)
    }
    /// Keys to type again at `now` for the keys held down: the first once the
    /// repeat delay has passed since the press, then one per interval.
    pub fn repeats(&mut self, now: u64) -> Vec<Key> {
        (0..2).filter_map(|hand| self.repeat(hand, now)).collect()
    }
    pub fn state_script(&self) -> String {
        let labels = layout()
            .iter()
            .map(|k| {
                let shifted = if k.base.is_ascii_alphabetic() {
                    (self.modifiers() & SHIFT != 0) ^ self.caps
                } else {
                    self.modifiers() & SHIFT != 0
                };
                let label = if k.label.is_empty() {
                    if shifted { k.shifted } else { k.base }.to_string()
                } else {
                    k.label.clone()
                };
                format!("[{},{}]", k.vk, quote(&label))
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "setKeyboardState({{{}shift:{},caps:{},mods:{},labels:[{}]}})",
            "",
            quote(match self.shift {
                ShiftState::Off => "Off",
                ShiftState::Once => "Once",
                ShiftState::Locked => "Locked",
            }),
            self.caps,
            self.modifiers(),
            labels
        )
    }
}
pub fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
pub fn layout_script() -> String {
    format!(
        "installLayout([{}])",
        layout()
            .iter()
            .map(|k| format!(
                "[{},{},{},{},{},{},{}]",
                k.vk,
                k.block,
                k.row,
                k.column,
                k.columns,
                k.rows,
                quote(&if k.label.is_empty() {
                    k.base.to_string()
                } else {
                    k.label.clone()
                })
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tap(k: &mut Keyboard, vk: u32, t: u64) -> Option<Key> {
        let pressed = k.down(0, vk, t);
        assert!(k.held[0].is_some());
        pressed.or(k.up(0, vk, t))
    }
    #[test]
    fn shift_once_double_lock_timeout_and_selection() {
        let mut k = Keyboard::default();
        tap(&mut k, 160, 0);
        assert_eq!(k.shift, ShiftState::Once);
        assert_eq!(tap(&mut k, 49, 20).unwrap().character, '!' as u32);
        assert_eq!(k.shift, ShiftState::Off);
        tap(&mut k, 160, 100);
        tap(&mut k, 161, 300);
        assert_eq!(k.shift, ShiftState::Locked);
        assert_eq!(tap(&mut k, 37, 400).unwrap().modifiers, SHIFT);
        assert_eq!(tap(&mut k, 191, 500).unwrap().character, '?' as u32);
        tap(&mut k, 160, 600);
        assert_eq!(k.shift, ShiftState::Off);
        tap(&mut k, 160, 1000);
        tap(&mut k, 160, 1000 + DOUBLE_PRESS_MS + 1);
        assert_eq!(k.shift, ShiftState::Off);
    }
    #[test]
    fn intervening_key_prevents_lock_and_caps_is_letter_only() {
        let mut k = Keyboard::default();
        tap(&mut k, 160, 0);
        tap(&mut k, 65, 20);
        tap(&mut k, 161, 40);
        assert_eq!(k.shift, ShiftState::Once);
        tap(&mut k, 160, 40 + DOUBLE_PRESS_MS + 1);
        assert_eq!(k.shift, ShiftState::Off);
        // Caps Lock toggles on press, and only letters follow it.
        assert!(k.down(0, 20, 1000).is_none());
        assert!(k.caps);
        assert!(k.up(0, 20, 1000).is_none());
        assert_eq!(tap(&mut k, 65, 1100).unwrap().character, 'A' as u32);
        assert_eq!(tap(&mut k, 49, 1110).unwrap().character, '1' as u32);
    }
    #[test]
    fn two_hands_chord_meta_and_cancel() {
        let mut k = Keyboard::default();
        // Win held by one hand, A pressed by the other: Win+A types on press.
        assert!(k.down(0, 91, 0).is_none());
        assert_eq!(k.down(1, 65, 0).unwrap().modifiers, META);
        assert!(k.up(1, 65, 0).is_none());
        assert!(k.up(0, 91, 10).is_none());
        assert_eq!(tap(&mut k, 91, 20).unwrap().vk, 91);
        tap(&mut k, 162, 30);
        assert_eq!(tap(&mut k, 67, 40).unwrap().modifiers, CTRL);
        assert_eq!(k.modifiers(), 0);
        // Cancelling a hold cannot take back the key typed on press.
        assert_eq!(k.down(0, 65, 50).unwrap().vk, 65);
        assert_eq!(k.down(1, 66, 50).unwrap().vk, 66);
        assert!(k.up(1, 66, 50).is_none());
        k.cancel(0);
        assert!(k.up(0, 65, 60).is_none());
    }
    #[test]
    fn held_shift_is_consumed_by_other_hand() {
        let mut k = Keyboard::default();
        k.down(0, 160, 0);
        assert_eq!(k.down(1, 65, 0).unwrap().character, 'A' as u32);
        assert!(k.up(1, 65, 10).is_none());
        k.up(0, 160, 20);
        assert_eq!(k.shift, ShiftState::Off);
    }
    #[test]
    fn held_key_types_on_press_then_repeats_after_the_delay() {
        let mut k = Keyboard::default();
        let pressed = k.down(0, 65, 1000).unwrap();
        assert_eq!(pressed.character, 'a' as u32);
        assert!(k.repeats(1000).is_empty());
        let first_due = 1000 + REPEAT_DELAY_MS;
        assert!(k.repeats(first_due - 1).is_empty());
        assert_eq!(k.repeats(first_due), vec![pressed]);
        assert!(k.repeats(first_due + REPEAT_INTERVAL_MS - 1).is_empty());
        assert_eq!(k.repeats(first_due + REPEAT_INTERVAL_MS), vec![pressed]);
        // A late call types one repeat, not every missed one.
        assert_eq!(k.repeats(first_due + 10 * REPEAT_INTERVAL_MS).len(), 1);
        assert!(k.up(0, 65, first_due + 11 * REPEAT_INTERVAL_MS).is_none());
        // A hand that left the keyboard stops repeating.
        assert!(k.down(1, 8, 5000).is_some());
        k.cancel(1);
        assert!(k.repeats(5000 + 2 * REPEAT_DELAY_MS).is_empty());
    }
    #[test]
    fn quick_tap_types_once_and_modifiers_never_type_or_repeat_while_held() {
        let mut k = Keyboard::default();
        assert_eq!(k.down(0, 65, 0).unwrap().character, 'a' as u32);
        assert!(k.repeats(REPEAT_DELAY_MS - 1).is_empty());
        assert!(k.up(0, 65, REPEAT_DELAY_MS - 1).is_none());
        assert!(k.repeats(10 * REPEAT_DELAY_MS).is_empty());
        for vk in [160, 162, 164, 91, 20] {
            assert!(k.down(0, vk, 0).is_none(), "vk {vk}");
            assert!(k.repeats(10 * REPEAT_DELAY_MS).is_empty(), "vk {vk}");
            k.cancel(0);
        }
    }
    #[test]
    fn repeats_keep_the_first_keystrokes_modifiers() {
        let mut k = Keyboard::default();
        tap(&mut k, 160, 0);
        assert_eq!(k.shift, ShiftState::Once);
        let first = k.down(0, 65, 100).unwrap();
        assert_eq!((first.character, first.modifiers), ('A' as u32, SHIFT));
        assert_eq!(k.shift, ShiftState::Off);
        assert_eq!(k.repeats(100 + REPEAT_DELAY_MS), vec![first]);
        assert_eq!(
            k.repeats(100 + REPEAT_DELAY_MS + REPEAT_INTERVAL_MS),
            vec![first]
        );
        k.up(0, 65, 2000);
        // Ctrl held by the other hand applies to every repeat and is used up.
        assert!(k.down(0, 162, 3000).is_none());
        let arrow = k.down(1, 37, 3000).unwrap();
        assert_eq!((arrow.vk, arrow.modifiers), (37, CTRL));
        assert_eq!(k.repeats(3000 + REPEAT_DELAY_MS), vec![arrow]);
        assert!(k.up(1, 37, 4000).is_none());
        assert!(k.up(0, 162, 4010).is_none());
        assert_eq!(k.modifiers(), 0);
    }
}
