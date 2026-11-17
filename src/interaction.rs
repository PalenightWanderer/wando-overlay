use crate::{
    geometry::{Hit, Matrix, Panel, Vec3, add, dot, scale, sub},
    layout::WIDTH_LIMITS,
    platform::{BAR, DASHBOARD, Hand, KEYBOARD, PANELS, VIEWS, WRIST},
};
// Initial tuning values; validate feel in-headset before changing them.
const DRAG_INTENT_METRES: f32 = 0.012;
const HOVER_TIME_CONSTANT: f32 = 0.012;
const GRAB_TIME_CONSTANT: f32 = 0.020;
/// Thumbstick at full deflection moves a grabbed panel this fast along the laser.
const GRAB_PUSH_METRES_PER_SECOND: f32 = 1.2;
/// A grabbed panel's centre stays between these distances in front of the hand.
const GRAB_DISTANCE: std::ops::RangeInclusive<f32> = 0.15..=6.;
/// Thumbstick at full deflection with the trigger held grows or shrinks a
/// grabbed panel by this factor per second (e to this power).
const GRAB_SCALE_RATE: f32 = 0.9;
/// Thumbstick at full deflection scrolls an HTML panel this many pixels per second.
const UI_SCROLL_PIXELS_PER_SECOND: f32 = 900.;

/// True while the hand's thumb rests on its right-click mode button: the
/// trigger then presses the right mouse button on captures.
pub fn right_mode(hand: &Hand) -> bool {
    hand.right != 0 && hand.actions_active & 4 != 0
}
/// Capture panels inject OS mouse input; the others are HTML views.
pub fn is_capture(panel: usize) -> bool {
    panel >= VIEWS
}

pub fn response(dt: f32, tau: f32) -> f32 {
    1. - (-dt.max(0.) / tau).exp()
}

/// Steadies one hand's pointer ray against shaky hands and the twitch of a
/// trigger pull. A One Euro filter: the slower the hand moves, the more its
/// pose is smoothed; a quick sweep passes through almost unchanged. Initial
/// tuning values; validate feel in-headset before changing them.
#[derive(Clone, Copy, Default)]
pub struct Steady {
    /// Last raw and last returned pose.
    last: Option<(Matrix, Matrix)>,
    /// Smoothed change per second of the ray direction and of the position.
    turn: Vec3,
    travel: Vec3,
}
/// Cutoff in Hz for a resting hand at the weakest and the strongest setting.
const STEADY_REST_HZ: (f32, f32) = (6., 0.6);
/// Cutoff gained per radian (or metre) per second of hand speed.
const STEADY_HZ_PER_SPEED: f32 = 20.;
/// Speed itself is smoothed at this cutoff, so a tremor does not read as motion.
const STEADY_SPEED_HZ: f32 = 2.;
impl Steady {
    /// `strength` runs from 0 (off) to 1. An invalid pose resets the filter.
    pub fn apply(&mut self, raw: Matrix, strength: f32, dt: f32) -> Matrix {
        let Some((before, shown)) = self
            .last
            .filter(|_| strength > 0. && dt > 0. && raw.rigid())
        else {
            *self = Self::default();
            if raw.rigid() {
                self.last = Some((raw, raw));
            }
            return raw;
        };
        let alpha = |hz: f32| response(dt, 1. / (std::f32::consts::TAU * hz));
        let blend = |old: Vec3, new: Vec3, a: f32| add(old, scale(sub(new, old), a));
        let a = alpha(STEADY_SPEED_HZ);
        self.turn = blend(self.turn, scale(sub(raw.col(2), before.col(2)), 1. / dt), a);
        self.travel = blend(
            self.travel,
            scale(sub(raw.col(3), before.col(3)), 1. / dt),
            a,
        );
        let speed = dot(self.turn, self.turn).sqrt() + dot(self.travel, self.travel).sqrt();
        let rest = STEADY_REST_HZ.0 * (STEADY_REST_HZ.1 / STEADY_REST_HZ.0).powf(strength.min(1.));
        let shown = shown.smooth(raw, alpha(rest + STEADY_HZ_PER_SPEED * speed));
        self.last = Some((raw, shown));
        shown
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// Bring a captured window to the foreground before clicking into it.
    Focus(usize),
    /// OS cursor to UV on a capture panel.
    Move(usize, f32, f32),
    Button(u32, bool),
    Scroll(f32),
    KeyDown(usize, u32),
    KeyUp(usize, u32),
    Cancel(usize),
    Windows,
    /// Generic overlay UI pointer (one press at a time across all HTML
    /// panels). Coordinates are panel UV; every event carries its panel and position.
    UiMove(usize, f32, f32),
    UiButton(usize, f32, f32, bool),
    UiCancel(usize),
    /// Scroll an HTML panel by pixels; positive scrolls up.
    UiScroll(usize, f32),
    /// The "show or hide keyboard" button was pressed.
    ToggleKeyboard,
    /// The "hide or show all overlays" button was pressed.
    ToggleOverlays,
}
#[derive(Clone, Copy, Default)]
struct Gate {
    ready: bool,
    previous: bool,
}
impl Gate {
    fn sample(&mut self, down: bool, available: bool) -> (bool, bool) {
        if !available {
            *self = Self::default();
            return (false, false);
        }
        if !down {
            self.ready = true;
        }
        let press = self.ready && down && !self.previous;
        let release = self.ready && !down && self.previous;
        self.previous = down;
        (press, release)
    }
}
#[derive(Clone, Copy)]
struct Press {
    panel: usize,
    key: Option<u32>,
    button: u32,
    generation: u64,
    anchor: (f32, f32),
    intent_origin: (f32, f32),
    dragging: bool,
}
#[derive(Clone, Copy, Default)]
pub struct Pointer {
    pub hover: Option<Hit>,
    press: Option<Press>,
    /// Trigger, grip, the keyboard button and the hide button.
    gates: [Gate; 4],
    device: Option<u32>,
    filtered: Option<Hit>,
}
#[derive(Clone, Copy)]
struct Grab {
    owner: usize,
    relative: Matrix,
}
pub struct Interaction {
    pub pointers: [Pointer; 2],
    pub panels: [Panel; PANELS],
    grabs: [Option<Grab>; PANELS],
    pub drag: Option<(usize, u32)>,
    cursor: usize,
    pub generation: u64,
    /// Hand and HTML panel of the single generic UI press.
    ui_owner: Option<(usize, usize)>,
}
impl Default for Interaction {
    fn default() -> Self {
        Self {
            pointers: [Pointer::default(); 2],
            panels: std::array::from_fn(|panel| {
                // Widths in metres. Heights follow each view's pixel aspect;
                // capture panels are resized when their first frame arrives.
                let (width, aspect) = match panel {
                    KEYBOARD => (2.05, 540. / 1760.),
                    DASHBOARD => (1.1, 760. / 1200.),
                    WRIST => (0.15, 170. / 440.),
                    BAR => (0.5, 190. / 700.),
                    _ => (2., 0.5),
                };
                Panel {
                    pose: Matrix::identity(),
                    width,
                    height: width * aspect,
                    enabled: false,
                    // The wrist clock is attached to the controller, the
                    // quick bar to its capture.
                    grabbable: panel != WRIST && panel != BAR,
                    curvature: 0.,
                    passive: false,
                }
            }),
            grabs: [None; PANELS],
            drag: None,
            cursor: 0,
            generation: 0,
            ui_owner: None,
        }
    }
}
impl Interaction {
    /// Latch drag intent using physical panel distance, independent of resolution.
    fn desktop_position(&mut self, h: usize) -> Action {
        let hit = self.pointers[h].filtered.unwrap();
        if let Some(mut press) = self.pointers[h].press.filter(|p| is_capture(p.panel)) {
            let raw = self.pointers[h].hover.unwrap();
            let dx = (raw.u - press.intent_origin.0) * self.panels[press.panel].width;
            let dy = (raw.v - press.intent_origin.1) * self.panels[press.panel].height;
            press.dragging |= dx.hypot(dy) > DRAG_INTENT_METRES;
            self.pointers[h].press = Some(press);
            if !press.dragging {
                return Action::Move(press.panel, press.anchor.0, press.anchor.1);
            }
        }
        Action::Move(hit.panel, hit.u, hit.v)
    }

    fn cancel_press(&mut self, h: usize, out: &mut Vec<Action>) {
        if let Some((_, panel)) = self.ui_owner.filter(|(owner, _)| *owner == h) {
            out.push(Action::UiCancel(panel));
            self.ui_owner = None;
        }
        self.pointers[h].press = None;
        out.push(Action::Cancel(h));
    }
    fn cancel_hand(&mut self, h: usize, out: &mut Vec<Action>) {
        if let Some((owner, b)) = self.drag {
            if owner == h {
                out.push(Action::Button(b, false));
                self.drag = None;
            }
        }
        self.cancel_press(h, out);
        self.pointers[h] = Pointer::default();
        for grab in &mut self.grabs {
            if grab.is_some_and(|g| g.owner == h) {
                *grab = None;
            }
        }
    }
    /// Hand that owns the generic UI press on `panel`, if any.
    pub fn ui_owner(&self, panel: usize) -> Option<usize> {
        self.ui_owner.filter(|(_, p)| *p == panel).map(|(h, _)| h)
    }
    pub fn grabbed(&self, panel: usize) -> bool {
        self.grabs[panel].is_some()
    }
    /// Stops any grab of `panel`, for example when it is hidden or re-placed.
    pub fn release_panel(&mut self, panel: usize) {
        self.grabs[panel] = None;
    }
    /// True while hand `h` holds a press, the desktop drag or a panel grab.
    pub fn engaged(&self, h: usize) -> bool {
        self.pointers[h].press.is_some()
            || self.drag.is_some_and(|(owner, _)| owner == h)
            || self.grabs.iter().flatten().any(|g| g.owner == h)
    }
    pub fn cancel(&mut self) -> Vec<Action> {
        let mut out = Vec::new();
        for h in 0..2 {
            self.cancel_hand(h, &mut out);
        }
        self.generation += 1;
        out
    }
    pub fn update(
        &mut self,
        hands: [Hand; 2],
        hits: [Option<Hit>; 2],
        keys: [Option<u32>; 2],
        active: bool,
        dt: f32,
    ) -> Vec<Action> {
        if !active {
            return self.cancel();
        }
        let mut out = Vec::new();
        // Detect identities and disconnects before processing either hand.
        for (h, hand) in hands.iter().enumerate() {
            let valid = hand.valid != 0 && hand.pose.rigid();
            if !valid || self.pointers[h].device.is_some_and(|d| d != hand.device) {
                self.cancel_hand(h, &mut out);
            }
            if valid {
                self.pointers[h].device = Some(hand.device);
            }
        }
        for (h, hand) in hands.iter().enumerate() {
            if self.pointers[h].device.is_none() {
                continue;
            }
            let hit = hits[h].filter(|hit| self.panels.get(hit.panel).is_some_and(|p| p.enabled));
            let old = self.pointers[h].hover;
            if old.map(|x| x.panel) != hit.map(|x| x.panel) {
                if let Some((owner, b)) = self.drag {
                    if owner == h {
                        out.push(Action::Button(b, false));
                        self.drag = None;
                    }
                }
                if self.pointers[h].press.is_some() {
                    self.cancel_press(h, &mut out);
                }
                // A target transition while held cannot synthesize a fresh down.
                // Retain an authoritative released sample across acquisition.
                // A held prior sample must still wait for release.
                let gate = &mut self.pointers[h].gates[0];
                if gate.previous {
                    *gate = Gate::default();
                }
                self.pointers[h].filtered = None;
            }
            self.pointers[h].hover = hit;
            self.pointers[h].filtered = hit.map(|mut next| {
                if is_capture(next.panel) {
                    if let Some(previous) = self.pointers[h].filtered {
                        let alpha = response(dt, HOVER_TIME_CONSTANT);
                        next.u = previous.u + alpha * (next.u - previous.u);
                        next.v = previous.v + alpha * (next.v - previous.v);
                    }
                }
                next
            });
            let (trigger, trigger_up) =
                self.pointers[h].gates[0].sample(hand.trigger != 0, hand.actions_active & 1 != 0);
            let (grip, grip_up) =
                self.pointers[h].gates[1].sample(hand.grip != 0, hand.actions_active & 2 != 0);
            // Works wherever the hand points, also at nothing.
            if self.pointers[h].gates[2]
                .sample(hand.keyboard != 0, hand.actions_active & 16 != 0)
                .0
            {
                out.push(Action::ToggleKeyboard);
            }
            if self.pointers[h].gates[3]
                .sample(hand.toggle != 0, hand.actions_active & 32 != 0)
                .0
            {
                out.push(Action::ToggleOverlays);
            }
            if hand.actions_active & 1 == 0 && self.pointers[h].press.is_some() {
                if let Some((owner, b)) = self.drag {
                    if owner == h {
                        out.push(Action::Button(b, false));
                        self.drag = None;
                    }
                }
                self.cancel_press(h, &mut out);
            }
            for panel in 0..PANELS {
                if let Some(g) = self.grabs[panel] {
                    if g.owner == h {
                        if grip_up || hand.actions_active & 2 == 0 {
                            self.grabs[panel] = None;
                        } else {
                            // Thumbstick up pushes the grabbed panel away along the
                            // laser (the hand's -Z), down pulls it closer. With the
                            // trigger also held it grows or shrinks the panel instead.
                            let mut g = g;
                            let stick = hand.actions_active & 8 != 0
                                && hand.scroll.is_finite()
                                && hand.scroll.abs() > 0.2;
                            if stick && hand.trigger != 0 && hand.actions_active & 1 != 0 {
                                let p = &mut self.panels[panel];
                                let width = (p.width * (hand.scroll * dt * GRAB_SCALE_RATE).exp())
                                    .clamp(WIDTH_LIMITS.0, WIDTH_LIMITS.1);
                                p.height *= width / p.width;
                                p.width = width;
                            } else if stick {
                                let distance = -g.relative.m[11]
                                    + hand.scroll * dt * GRAB_PUSH_METRES_PER_SECOND;
                                g.relative.m[11] =
                                    -distance.clamp(*GRAB_DISTANCE.start(), *GRAB_DISTANCE.end());
                                self.grabs[panel] = Some(g);
                            }
                            let pose = hand.pose.mul(g.relative);
                            if pose.rigid() {
                                self.panels[panel].pose = self.panels[panel]
                                    .pose
                                    .smooth(pose, response(dt, GRAB_TIME_CONSTANT));
                            }
                        }
                    }
                }
            }
            if grip && self.pointers[h].press.is_none() && self.drag.is_none() {
                if let Some(hit) = hit {
                    if self.grabs[hit.panel].is_none() && self.panels[hit.panel].grabbable {
                        self.grabs[hit.panel] = Some(Grab {
                            owner: h,
                            relative: hand.pose.inverse().mul(self.panels[hit.panel].pose),
                        });
                    }
                }
            }
            let grabbing = self.grabs.iter().flatten().any(|g| g.owner == h);
            if grabbing {
                continue;
            }
            if trigger {
                if let Some(hit) = hit {
                    self.cursor = h;
                    if is_capture(hit.panel) {
                        if self.drag.is_none() && !self.panels[hit.panel].passive {
                            // The mode is read once, at the press.
                            let b = right_mode(hand) as u32;
                            out.push(Action::Windows);
                            out.push(Action::Focus(hit.panel));
                            let cursor = self.pointers[h].filtered.unwrap_or(hit);
                            out.push(Action::Move(hit.panel, cursor.u, cursor.v));
                            out.push(Action::Button(b, true));
                            self.drag = Some((h, b));
                            self.pointers[h].press = Some(Press {
                                panel: hit.panel,
                                key: None,
                                button: b,
                                generation: self.generation,
                                anchor: self.pointers[h]
                                    .filtered
                                    .map_or((hit.u, hit.v), |p| (p.u, p.v)),
                                intent_origin: (hit.u, hit.v),
                                dragging: false,
                            });
                        }
                    } else if self.pointers[h].press.is_none() {
                        if let Some(key) = keys[h].filter(|_| hit.panel == KEYBOARD) {
                            out.push(Action::KeyDown(h, key));
                            self.pointers[h].press = Some(Press {
                                panel: hit.panel,
                                key: Some(key),
                                button: 0,
                                generation: self.generation,
                                anchor: self.pointers[h]
                                    .filtered
                                    .map_or((hit.u, hit.v), |p| (p.u, p.v)),
                                intent_origin: (hit.u, hit.v),
                                dragging: false,
                            });
                        } else if self.ui_owner.is_none() {
                            self.ui_owner = Some((h, hit.panel));
                            out.push(Action::UiMove(hit.panel, hit.u, hit.v));
                            out.push(Action::UiButton(hit.panel, hit.u, hit.v, true));
                            self.pointers[h].press = Some(Press {
                                panel: hit.panel,
                                key: None,
                                button: 0,
                                generation: self.generation,
                                anchor: self.pointers[h]
                                    .filtered
                                    .map_or((hit.u, hit.v), |p| (p.u, p.v)),
                                intent_origin: (hit.u, hit.v),
                                dragging: false,
                            });
                        }
                    }
                }
            }
            if let Some(p) = self.pointers[h].press {
                let still =
                    hit.is_some_and(|hit| hit.panel == p.panel) && p.generation == self.generation;
                if !is_capture(p.panel) && (!still || (p.key.is_none() && keys[h].is_some())) {
                    self.cancel_press(h, &mut out);
                } else if trigger_up {
                    if is_capture(p.panel) {
                        if self.drag == Some((h, p.button)) {
                            out.push(self.desktop_position(h));
                            out.push(Action::Button(p.button, false));
                            self.drag = None;
                        }
                    } else if let Some(key) = p.key {
                        out.push(Action::KeyUp(h, key));
                    } else if self.ui_owner == Some((h, p.panel)) {
                        // `still` above guarantees a hit on the pressed panel.
                        let (u, v) = hit.map_or((-1., -1.), |hit| (hit.u, hit.v));
                        out.push(Action::UiMove(p.panel, u, v));
                        out.push(Action::UiButton(p.panel, u, v, false));
                        self.ui_owner = None;
                    }
                    self.pointers[h].press = None;
                }
            }
            if let Some(hit) = hit.filter(|_| {
                hand.actions_active & 8 != 0 && hand.scroll.is_finite() && hand.scroll.abs() > 0.2
            }) {
                if !is_capture(hit.panel) {
                    out.push(Action::UiScroll(
                        hit.panel,
                        hand.scroll * dt * UI_SCROLL_PIXELS_PER_SECOND,
                    ));
                } else if self.drag.is_none() && !self.panels[hit.panel].passive {
                    self.cursor = h;
                    out.push(Action::Move(hit.panel, hit.u, hit.v));
                    out.push(Action::Scroll(hand.scroll * dt * 8.));
                }
            }
        }
        if let Some((owner, panel)) = self.ui_owner {
            if let Some(hit) = self.pointers[owner].hover.filter(|hit| hit.panel == panel) {
                out.push(Action::UiMove(panel, hit.u, hit.v));
            }
        }
        // One desktop cursor, stable hover ownership. Other hands cannot move
        // it or release its drag; both keyboard pointers remain independent.
        let owner = if let Some((owner, _)) = self.drag {
            Some(owner)
        } else {
            if !self.pointers[self.cursor]
                .hover
                .is_some_and(|h| is_capture(h.panel))
            {
                if let Some(h) =
                    (0..2).find(|h| self.pointers[*h].hover.is_some_and(|x| is_capture(x.panel)))
                {
                    self.cursor = h;
                }
            }
            Some(self.cursor)
        };
        if let Some(owner) = owner {
            if self.pointers[owner]
                .hover
                .is_some_and(|x| is_capture(x.panel))
            {
                out.push(self.desktop_position(owner));
            }
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::CAPTURES;
    const K: usize = KEYBOARD;
    const C: usize = VIEWS;
    fn hand() -> Hand {
        Hand {
            valid: 1,
            device: 1,
            actions_active: 15,
            ..Default::default()
        }
    }
    fn hit(panel: usize) -> Option<Hit> {
        Some(Hit {
            panel,
            distance: 1.,
            u: 0.5,
            v: 0.5,
        })
    }
    fn engine() -> Interaction {
        let mut e = Interaction::default();
        for p in &mut e.panels {
            p.enabled = true;
        }
        e
    }
    /// A hand at the origin aiming `angle` radians to the right of straight ahead.
    fn aimed(angle: f32) -> Matrix {
        Matrix::basis(
            [angle.cos(), 0., -angle.sin()],
            [0., 1., 0.],
            [angle.sin(), 0., angle.cos()],
            [0.; 3],
        )
    }
    #[test]
    fn steady_pointer_damps_a_tremor_but_follows_a_sweep() {
        // 10 Hz tremor of 0.3 degrees, sampled at 120 Hz.
        let tremor = |i: usize| 0.005 * (i as f32 * std::f32::consts::TAU * 10. / 120.).sin();
        let mut steady = Steady::default();
        let mut worst = 0f32;
        for i in 0..240 {
            let out = steady.apply(aimed(tremor(i)), 0.5, 1. / 120.);
            assert!(out.rigid());
            if i > 60 {
                worst = worst.max(out.m[2].abs());
            }
        }
        assert!(worst < 0.0015, "tremor left {worst} of 0.005");
        // Off passes poses through untouched.
        let mut off = Steady::default();
        off.apply(aimed(0.), 0., 1. / 120.);
        assert_eq!(off.apply(aimed(0.004), 0., 1. / 120.), aimed(0.004));
        // A sweep of 2 radians per second is followed within a degree.
        let mut steady = Steady::default();
        let mut out = aimed(0.);
        for i in 0..120 {
            out = steady.apply(aimed(i as f32 * 2. / 120.), 1., 1. / 120.);
        }
        let lag = (119. * 2. / 120f32) - out.m[2].atan2(out.m[10]);
        assert!((0. ..0.0175).contains(&lag), "lag {lag} rad");
        // Losing tracking forgets the old pose instead of gliding from it.
        let mut lost = aimed(0.);
        lost.m[0] = f32::NAN;
        steady.apply(lost, 1., 1. / 120.);
        assert_eq!(steady.apply(aimed(-1.), 1., 1. / 120.), aimed(-1.));
    }
    #[test]
    fn thumbstick_pushes_and_pulls_a_grabbed_panel_within_limits() {
        let mut e = engine();
        e.panels[C].pose.m[11] = -1.;
        let mut h = [hand(), Hand::default()];
        e.update(h, [hit(C), None], [None; 2], true, 0.01);
        h[0].grip = 1;
        e.update(h, [hit(C), None], [None; 2], true, 0.01);
        let settle = |e: &mut Interaction, h: [Hand; 2], frames: usize| {
            let mut actions = Vec::new();
            for _ in 0..frames {
                actions.extend(e.update(h, [hit(C), None], [None; 2], true, 0.01));
            }
            actions
        };
        h[0].scroll = 1.;
        let actions = settle(&mut e, h, 50);
        // Half a second at 1.2 m/s, and the stick does not scroll meanwhile.
        h[0].scroll = 0.;
        settle(&mut e, h, 50);
        assert!((e.panels[C].pose.m[11] + 1.6).abs() < 0.01);
        assert!(!actions.iter().any(|a| matches!(a, Action::Scroll(_))));
        h[0].scroll = -1.;
        settle(&mut e, h, 1000);
        assert!(
            (e.panels[C].pose.m[11] + 0.15).abs() < 0.01,
            "closest limit"
        );
        h[0].scroll = 1.;
        settle(&mut e, h, 1000);
        assert!((e.panels[C].pose.m[11] + 6.).abs() < 0.01, "farthest limit");
        // Released, the stick leaves the panel where it is.
        h[0].grip = 0;
        settle(&mut e, h, 20);
        assert!((e.panels[C].pose.m[11] + 6.).abs() < 0.01);
    }
    #[test]
    fn trigger_while_gripping_scales_instead_of_moving_and_never_clicks() {
        let mut e = engine();
        e.panels[C].pose.m[11] = -1.;
        let (width, aspect) = (e.panels[C].width, e.panels[C].height / e.panels[C].width);
        let mut h = [hand(), Hand::default()];
        e.update(h, [hit(C), None], [None; 2], true, 0.01);
        h[0].grip = 1;
        e.update(h, [hit(C), None], [None; 2], true, 0.01);
        h[0].trigger = 1;
        h[0].scroll = 1.;
        let mut actions = Vec::new();
        for _ in 0..100 {
            actions.extend(e.update(h, [hit(C), None], [None; 2], true, 0.01));
        }
        let p = e.panels[C];
        assert!((p.width / width - GRAB_SCALE_RATE.exp()).abs() < 0.01);
        assert!((p.height / p.width - aspect).abs() < 0.0001, "aspect kept");
        assert!((p.pose.m[11] + 1.).abs() < 0.001, "distance unchanged");
        h[0].scroll = -1.;
        for _ in 0..2000 {
            actions.extend(e.update(h, [hit(C), None], [None; 2], true, 0.01));
        }
        assert_eq!(e.panels[C].width, WIDTH_LIMITS.0);
        // Letting go of the grip with the trigger still held is not a click.
        h[0].grip = 0;
        h[0].scroll = 0.;
        actions.extend(e.update(h, [hit(C), None], [None; 2], true, 0.01));
        assert!(!actions.iter().any(|a| matches!(a, Action::Button(..))));
    }
    #[test]
    fn placeholder_panel_can_be_grabbed_but_takes_no_clicks_or_scrolling() {
        let mut e = engine();
        e.panels[C].passive = true;
        let mut h = [hand(), Hand::default()];
        e.update(h, [hit(C), None], [None; 2], true, 0.01);
        h[0].trigger = 1;
        h[0].scroll = 1.;
        let a = e.update(h, [hit(C), None], [None; 2], true, 0.01);
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Button(..) | Action::Scroll(_) | Action::Focus(_)))
        );
        h[0].trigger = 0;
        h[0].scroll = 0.;
        e.update(h, [hit(C), None], [None; 2], true, 0.01);
        h[0].grip = 1;
        e.update(h, [hit(C), None], [None; 2], true, 0.01);
        assert!(e.engaged(0));
    }
    #[test]
    fn keyboard_button_toggles_once_per_press_anywhere() {
        let mut e = engine();
        let mut h = [
            Hand {
                actions_active: 31,
                ..hand()
            },
            Hand::default(),
        ];
        e.update(h, [None; 2], [None; 2], true, 0.01);
        h[0].keyboard = 1;
        assert!(
            e.update(h, [None; 2], [None; 2], true, 0.01)
                .contains(&Action::ToggleKeyboard)
        );
        assert!(
            !e.update(h, [None; 2], [None; 2], true, 0.01)
                .contains(&Action::ToggleKeyboard)
        );
        // Unbound (readiness bit clear) never toggles.
        h[0].keyboard = 0;
        e.update(h, [None; 2], [None; 2], true, 0.01);
        h[0].actions_active = 15;
        h[0].keyboard = 1;
        assert!(
            !e.update(h, [None; 2], [None; 2], true, 0.01)
                .contains(&Action::ToggleKeyboard)
        );
    }
    #[test]
    fn html_panels_click_scroll_and_never_inject_os_input() {
        let mut e = engine();
        let mut h = [hand(), Hand::default()];
        e.update(h, [hit(WRIST), None], [None; 2], true, 0.016);
        h[0].trigger = 1;
        h[0].grip = 1;
        let a = e.update(h, [hit(WRIST), None], [None; 2], true, 0.016);
        assert!(a.contains(&Action::UiButton(WRIST, 0.5, 0.5, true)));
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Button(..) | Action::Move(..)))
        );
        // The wrist panel cannot be grabbed, so the press survives the grip.
        assert_eq!(e.ui_owner(WRIST), Some(0));
        h[0].trigger = 0;
        h[0].grip = 0;
        let a = e.update(h, [hit(WRIST), None], [None; 2], true, 0.016);
        assert!(a.contains(&Action::UiButton(WRIST, 0.5, 0.5, false)));
        h[0].scroll = 1.;
        let a = e.update(h, [hit(WRIST), None], [None; 2], true, 0.01);
        assert!(a.contains(&Action::UiScroll(WRIST, 9.)));
    }
    #[test]
    fn capture_press_focuses_its_window_and_reports_its_panel() {
        let mut e = engine();
        let mut h = [hand(), Hand::default()];
        let last = C + CAPTURES - 1;
        let second = Some(Hit {
            panel: last,
            distance: 1.,
            u: 0.25,
            v: 0.75,
        });
        e.update(h, [second, None], [None; 2], true, 0.016);
        h[0].trigger = 1;
        let a = e.update(h, [second, None], [None; 2], true, 0.016);
        let focus = a.iter().position(|a| *a == Action::Focus(last)).unwrap();
        assert_eq!(a[focus + 1], Action::Move(last, 0.25, 0.75));
        assert_eq!(a[focus + 2], Action::Button(0, true));
    }
    #[test]
    fn independent_keyboard_presses_and_disconnect() {
        let mut e = engine();
        let mut h = [
            hand(),
            Hand {
                device: 2,
                ..hand()
            },
        ];
        e.update(h, [hit(K); 2], [Some(65), Some(66)], true, 0.016);
        h[0].trigger = 1;
        h[1].trigger = 1;
        let a = e.update(h, [hit(K); 2], [Some(65), Some(66)], true, 0.016);
        assert!(a.contains(&Action::KeyDown(0, 65)) && a.contains(&Action::KeyDown(1, 66)));
        h[0].valid = 0;
        h[1].trigger = 0;
        let a = e.update(h, [hit(K); 2], [Some(65), Some(66)], true, 0.016);
        assert!(a.contains(&Action::Cancel(0)) && a.contains(&Action::KeyUp(1, 66)));
    }
    #[test]
    fn only_owner_moves_and_releases_drag() {
        let mut e = engine();
        let mut h = [hand(), hand()];
        e.update(h, [hit(C); 2], [None; 2], true, 0.016);
        h[0].trigger = 1;
        e.update(h, [hit(C); 2], [None; 2], true, 0.016);
        h[1].trigger = 1;
        let mut other = hit(C).unwrap();
        other.u = 0.9;
        let a = e.update(h, [hit(C), Some(other)], [None; 2], true, 0.016);
        assert_eq!(e.drag, Some((0, 0)));
        assert!(!a.contains(&Action::Move(C, 0.9, 0.5)));
        h[1].trigger = 0;
        assert!(
            !e.update(h, [hit(C); 2], [None; 2], true, 0.016)
                .contains(&Action::Button(0, false))
        );
        h[0].valid = 0;
        assert!(
            e.update(h, [hit(C); 2], [None; 2], true, 0.016)
                .contains(&Action::Button(0, false))
        );
    }
    #[test]
    fn startup_dashboard_recovery_and_target_change_require_release() {
        let mut e = engine();
        let mut h = [
            Hand {
                trigger: 1,
                ..hand()
            },
            Hand::default(),
        ];
        assert!(
            !e.update(h, [hit(C), None], [None; 2], true, 0.016)
                .contains(&Action::Button(0, true))
        );
        h[0].trigger = 0;
        e.update(h, [hit(C), None], [None; 2], true, 0.016);
        h[0].trigger = 1;
        assert!(
            e.update(h, [hit(C), None], [None; 2], true, 0.016)
                .contains(&Action::Button(0, true))
        );
        e.update(h, [None; 2], [None; 2], false, 0.016);
        assert!(
            !e.update(h, [hit(C), None], [None; 2], true, 0.016)
                .contains(&Action::Button(0, true))
        );
        assert!(
            !e.update(h, [hit(K), None], [Some(65), None], true, 0.016)
                .contains(&Action::KeyDown(0, 65))
        );
    }
    #[test]
    fn optional_capabilities_do_not_block_trigger() {
        let mut e = engine();
        let mut h = [
            Hand {
                actions_active: 1,
                ..hand()
            },
            Hand::default(),
        ];
        e.update(h, [hit(C), None], [None; 2], true, 0.016);
        h[0].trigger = 1;
        assert!(
            e.update(h, [hit(C), None], [None; 2], true, 0.016)
                .contains(&Action::Button(0, true))
        );
    }
    #[test]
    fn grip_release_and_disconnect_keep_last_transform() {
        let mut e = engine();
        let mut h = [hand(), Hand::default()];
        e.update(h, [hit(C), None], [None; 2], true, 0.016);
        h[0].grip = 1;
        e.update(h, [hit(C), None], [None; 2], true, 0.016);
        h[0].pose.m[3] = 0.7;
        e.update(h, [hit(C), None], [None; 2], true, 0.016);
        let last = e.panels[C].pose;
        assert!(last.m[3] > 0. && last.m[3] < 0.7);
        h[0].valid = 0;
        e.update(h, [None; 2], [None; 2], true, 0.016);
        assert_eq!(last, e.panels[C].pose);
    }
    #[test]
    fn generic_ui_serializes_gestures_while_keys_stay_independent() {
        let mut e = engine();
        let mut h = [hand(), hand()];
        e.update(h, [hit(K); 2], [None; 2], true, 0.016);
        h[0].trigger = 1;
        let a = e.update(h, [hit(K); 2], [None; 2], true, 0.016);
        assert!(a.contains(&Action::UiButton(K, 0.5, 0.5, true)));
        h[1].trigger = 1;
        assert!(
            !e.update(h, [hit(K); 2], [None; 2], true, 0.016)
                .contains(&Action::UiButton(K, 0.5, 0.5, true))
        );
        h[1].trigger = 0;
        e.update(h, [hit(K); 2], [None, Some(65)], true, 0.016);
        h[1].trigger = 1;
        assert!(
            e.update(h, [hit(K); 2], [None, Some(65)], true, 0.016)
                .contains(&Action::KeyDown(1, 65))
        );
        h[0].valid = 0;
        assert!(
            e.update(h, [hit(K); 2], [None, Some(65)], true, 0.016)
                .contains(&Action::UiCancel(K))
        );
    }
    #[test]
    fn keyboard_latches_original_key_across_neighbour_and_gap() {
        let mut e = engine();
        let mut h = [hand(), Hand::default()];
        e.update(h, [hit(K), None], [Some(65), None], true, 0.008);
        h[0].trigger = 1;
        assert!(
            e.update(h, [hit(K), None], [Some(65), None], true, 0.008)
                .contains(&Action::KeyDown(0, 65))
        );
        for key in [Some(66), None] {
            let a = e.update(h, [hit(K), None], [key, None], true, 0.008);
            assert!(!a.contains(&Action::Cancel(0)));
        }
        h[0].trigger = 0;
        let a = e.update(h, [hit(K), None], [None; 2], true, 0.008);
        assert!(a.contains(&Action::KeyUp(0, 65)));
        assert!(
            !e.update(h, [hit(K), None], [None; 2], true, 0.008)
                .contains(&Action::KeyUp(0, 65))
        );
    }
    #[test]
    fn released_sample_allows_press_on_acquisition_but_held_entry_does_not() {
        for held in [false, true] {
            let mut e = engine();
            let mut h = [
                Hand {
                    trigger: held as u32,
                    ..hand()
                },
                Hand::default(),
            ];
            e.update(h, [None; 2], [None; 2], true, 0.008);
            h[0].trigger = 1;
            let a = e.update(h, [hit(K), None], [Some(65), None], true, 0.008);
            assert_eq!(a.contains(&Action::KeyDown(0, 65)), !held);
        }
    }
    #[test]
    fn stationary_clicks_freeze_until_release_and_intent_latches_for_both_buttons() {
        for button in [0, 1] {
            let mut e = engine();
            let mut h = [hand(), Hand::default()];
            e.update(h, [hit(C), None], [None; 2], true, 0.008);
            h[0].trigger = 1;
            h[0].right = button;
            e.update(h, [hit(C), None], [None; 2], true, 0.008);
            for u in [0.501, 0.499, 0.502] {
                let mut noisy = hit(C).unwrap();
                noisy.u = u;
                let a = e.update(h, [Some(noisy), None], [None; 2], true, 0.008);
                assert!(
                    a.iter()
                        .filter_map(|a| if let Action::Move(_, u, v) = a {
                            Some((*u, *v))
                        } else {
                            None
                        })
                        .all(|p| p == (0.5, 0.5))
                );
            }
            h[0].trigger = 0;
            h[0].right = 0;
            let mut noisy = hit(C).unwrap();
            noisy.u = 0.502;
            let a = e.update(h, [Some(noisy), None], [None; 2], true, 0.008);
            let release = a
                .iter()
                .position(|a| *a == Action::Button(button, false))
                .unwrap();
            assert_eq!(a[release - 1], Action::Move(C, 0.5, 0.5));
            assert!(
                !a[..release]
                    .iter()
                    .any(|a| matches!(a,Action::Move(_,u,_) if *u != 0.5))
            );
            // Start a deliberate drag, then return to the original anchor.
            e.update(h, [hit(C), None], [None; 2], true, 0.008);
            h[0].trigger = 1;
            h[0].right = button;
            e.update(h, [hit(C), None], [None; 2], true, 0.008);
            noisy.u = 0.55;
            e.update(h, [Some(noisy), None], [None; 2], true, 0.008);
            assert!(e.pointers[0].press.unwrap().dragging);
            e.update(h, [hit(C), None], [None; 2], true, 0.008);
            assert!(e.pointers[0].press.unwrap().dragging);
        }
    }
    #[test]
    fn grab_release_and_reacquisition_reset_smoothing() {
        let mut e = engine();
        let mut h = [hand(), Hand::default()];
        e.update(h, [hit(C), None], [None; 2], true, 0.008);
        h[0].grip = 1;
        e.update(h, [hit(C), None], [None; 2], true, 0.008);
        h[0].pose.m[3] = 0.2;
        e.update(h, [hit(C), None], [None; 2], true, 0.008);
        let shown = e.panels[C].pose;
        h[0].grip = 0;
        h[0].pose.m[3] = 0.4;
        e.update(h, [hit(C), None], [None; 2], true, 0.008);
        assert_eq!(shown, e.panels[C].pose);
        h[0].grip = 1;
        e.update(h, [hit(C), None], [None; 2], true, 0.008);
        e.update(h, [hit(C), None], [None; 2], true, 0.008);
        assert!((shown.m[3] - e.panels[C].pose.m[3]).abs() < 0.00001);
    }
    #[test]
    fn grab_filter_reduces_noise_and_tracks_fast_motion_with_dt() {
        let origin = Matrix::identity();
        let mut noise = origin;
        noise.m[3] = 0.002;
        let filtered = origin.smooth(noise, response(0.008, GRAB_TIME_CONSTANT));
        assert!(filtered.m[3] > 0. && filtered.m[3] < noise.m[3]);
        let target = Matrix::basis([0., 0., -1.], [0., 1., 0.], [1., 0., 0.], [1., 0., 0.]);
        let short = origin.smooth(target, response(0.008, GRAB_TIME_CONSTANT));
        let long = origin.smooth(target, response(0.016, GRAB_TIME_CONSTANT));
        assert!(short.rigid() && long.rigid());
        assert!(long.m[3] > short.m[3]);
        let mut tracked = origin;
        for _ in 0..12 {
            tracked = tracked.smooth(target, response(0.008, GRAB_TIME_CONSTANT));
        }
        assert!(tracked.rigid() && tracked.m[3] > 0.99);
        let half_turn = Matrix::basis([-1., 0., 0.], [0., 1., 0.], [0., 0., -1.], [0.; 3]);
        assert!(origin.smooth(half_turn, 0.5).rigid());
    }
    #[test]
    fn latched_key_still_cancels_on_panel_exit_and_action_loss() {
        for lose_action in [false, true] {
            let mut e = engine();
            let mut h = [hand(), Hand::default()];
            e.update(h, [hit(K), None], [Some(65), None], true, 0.008);
            h[0].trigger = 1;
            e.update(h, [hit(K), None], [Some(65), None], true, 0.008);
            let hits = if lose_action {
                h[0].actions_active = 0;
                [hit(K), None]
            } else {
                [None; 2]
            };
            assert!(
                e.update(h, hits, [None; 2], true, 0.008)
                    .contains(&Action::Cancel(0))
            );
            h[0].trigger = 0;
            h[0].actions_active = 15;
            assert!(
                !e.update(h, [hit(K), None], [Some(65), None], true, 0.008)
                    .contains(&Action::KeyUp(0, 65))
            );
        }
    }
}
