use crate::{
    geometry::{self, Hit, Matrix},
    input::{OwnedInput, Target},
    interaction::{Action, Interaction, Steady, response},
    keyboard::{self, Keyboard},
    layout::{self, Anchor, Layout, SavedCapture, WIDTH_LIMITS},
    platform::{
        BAR, CAPTURES, Capture, CreateError, DASHBOARD, Frame, Hand, Input, KEYBOARD, Key, PANELS,
        Platform, VIEW_SIZE, VIEWS, WRIST,
    },
    settings::{self, Settings},
    shell::{Command, OverlaySet, PresetAction, Shell, Slot},
};
use std::time::{Duration, Instant};

/// Worker exit code: SteamVR is absent or quit. The supervisor waits and retries.
pub const EXIT_WAIT_FOR_RUNTIME: i32 = 10;
/// Beam length while a hand holds a press or grab but the ray hits nothing.
const ENGAGED_BEAM_LENGTH: f32 = 1.;
/// Capture panels show this many source pixels per metre of panel width.
const CAPTURE_PIXELS_PER_METRE: f32 = 960.;
/// SteamVR overlay curvature used by the "curved" display style.
const CURVED: f32 = 0.12;
/// A panel that fades when turned away is fully shown while its front faces
/// the viewer within about 63 degrees and gone beyond about 81 (cosines of the
/// angle between its normal and the line to the head). Tune in the headset.
const TURN_FULL: f32 = 0.45;
const TURN_GONE: f32 = 0.15;
/// How long a hidden menu stays drawn while its page eases out (ui/theme.css).
const VANISH: Duration = Duration::from_millis(150);
/// Time constant of a panel fading out and back in.
const FADE_TIME_CONSTANT: f32 = 0.06;
/// A panel fainter than this takes no laser hits.
const POINTABLE_ALPHA: f32 = 0.05;
/// A capture newly attached to a controller is at most this wide, in metres,
/// and its lower edge sits this far above the controller.
const HAND_WIDTH: f32 = 0.35;
const HAND_GAP: f32 = 0.1;
/// The quick bar hangs this far under the capture a laser last pointed at and
/// stays this long after no laser points at either of them.
const BAR_GAP: f32 = 0.02;
const BAR_LINGER: Duration = Duration::from_millis(1500);
/// Wrist clock pose relative to the left controller's tracking pose: behind
/// the grip toward the elbow, facing out from the back of the hand, text
/// running toward the fingers. Initial values; tune in the headset.
const WRIST_POSE: Matrix = Matrix {
    m: [0., 0., -1., -0.045, 0., -1., 0., -0.01, -1., 0., 0., 0.12],
};

fn dispatch(
    actions: Vec<Action>,
    platform: &mut Platform,
    input: &mut OwnedInput,
    keyboard: &mut Keyboard,
    now: u64,
    captures: &[Capture; CAPTURES],
) -> Result<(), String> {
    let pixel = |panel: usize, u: f32, v: f32| {
        let (w, h) = VIEW_SIZE[panel];
        ((u * w).round() as i32, (v * h).round() as i32)
    };
    let mut actions = actions.into_iter().peekable();
    while let Some(action) = actions.next() {
        match action {
            Action::Focus(panel) => {
                if !input.blocked {
                    platform.inject(Input {
                        kind: 5,
                        code: (panel - VIEWS) as u32,
                        ..Default::default()
                    });
                }
            }
            Action::Move(panel, u, v) => {
                let slot = panel - VIEWS;
                let size = captures[slot];
                if size.ready == 0 {
                    continue;
                }
                let position = (
                    slot as u32,
                    (u * size.width.saturating_sub(1) as f32).round() as i32,
                    (v * size.height.saturating_sub(1) as f32).round() as i32,
                );
                // Hover moves are deduplicated; a move preceding a click or
                // scroll is always sent in case the physical mouse moved.
                let force = matches!(actions.peek(), Some(Action::Button(..) | Action::Scroll(_)));
                if !input.blocked && (force || input.last_move != Some(position)) {
                    let event = Input {
                        kind: 2,
                        code: position.0,
                        x: position.1,
                        y: position.2,
                        ..Default::default()
                    };
                    if input.send(platform, event) {
                        input.last_move = Some(position);
                    }
                }
            }
            Action::Button(code, down) => {
                input.send(
                    platform,
                    Input {
                        kind: 1,
                        code,
                        down: down as u32,
                        ..Default::default()
                    },
                );
            }
            Action::Scroll(delta) => {
                if !input.blocked {
                    input.send(
                        platform,
                        Input {
                            kind: 3,
                            y: (delta * 120.).round() as i32,
                            ..Default::default()
                        },
                    );
                }
            }
            Action::KeyDown(hand, vk) => {
                platform.script(KEYBOARD, &format!("pointerPress({hand},{vk})"))?;
                if let Some(key) = keyboard.down(hand, vk, now) {
                    type_key(platform, input, keyboard, key);
                }
            }
            Action::KeyUp(hand, vk) => {
                platform.script(KEYBOARD, &format!("pointerCancel({hand})"))?;
                if let Some(key) = keyboard.up(hand, vk, now) {
                    type_key(platform, input, keyboard, key);
                }
            }
            Action::Cancel(hand) => {
                keyboard.cancel(hand);
                platform.script(KEYBOARD, &format!("pointerCancel({hand})"))?;
            }
            Action::Windows => {
                if input.target != Target::Windows {
                    keyboard.reset();
                    for view in 0..VIEWS {
                        platform.script(
                            view,
                            "if(document.activeElement)document.activeElement.blur()",
                        )?;
                    }
                    platform.script(KEYBOARD, "setContext(false)")?;
                    input.target = Target::Windows;
                }
            }
            Action::UiMove(panel, u, v) => {
                let (x, y) = pixel(panel, u, v);
                platform.mouse(panel, x, y, 0)?
            }
            Action::UiButton(panel, u, v, down) => {
                let (x, y) = pixel(panel, u, v);
                platform.mouse(panel, x, y, if down { 1 } else { 2 })?
            }
            Action::UiCancel(panel) => {
                platform.mouse(panel, -100, -100, 0)?;
                platform.mouse(panel, -100, -100, 2)?;
            }
            Action::UiScroll(panel, pixels) => {
                platform.mouse(panel, 0, pixels.round() as i32, 3)?
            }
            // Handled by the session loop, which owns panel visibility.
            Action::ToggleKeyboard | Action::ToggleOverlays => {}
        }
    }
    Ok(())
}

/// Types a key into the current target. An overlay text field that refuses it
/// has lost focus, so typing returns to Windows.
fn type_key(platform: &mut Platform, input: &mut OwnedInput, keyboard: &mut Keyboard, key: Key) {
    if !input.key(platform, key) && input.target == Target::Local {
        keyboard.reset();
        input.target = Target::Windows;
    }
}

/// Types again the held keys whose repeat is due at `now`.
fn repeat_keys(platform: &mut Platform, input: &mut OwnedInput, keyboard: &mut Keyboard, now: u64) {
    for key in keyboard.repeats(now) {
        type_key(platform, input, keyboard, key);
    }
}

/// Moves one hand's presentation pointer on the keyboard and returns the
/// validated key under it. Hits on other panels hide that hand's pointer.
fn keyboard_pointer(
    platform: &mut Platform,
    hand: usize,
    hit: Option<Hit>,
) -> Result<Option<u32>, String> {
    let ui = hit.filter(|hit| hit.panel == KEYBOARD);
    let (w, h) = VIEW_SIZE[KEYBOARD];
    let (x, y) = ui.map_or((-100., -100.), |hit| (hit.u * w, hit.v * h));
    let id = platform.script(
        KEYBOARD,
        &format!("pointerAt({hand},{x},{y},{})", ui.is_some()),
    )?;
    Ok(id
        .parse::<u32>()
        .ok()
        .filter(|vk| keyboard::definition(*vk).is_some()))
}

/// Keyboard output goes to a focused editable overlay element when one exists,
/// otherwise to the Windows foreground. A change cancels held input first.
fn route(
    platform: &mut Platform,
    input: &mut OwnedInput,
    keyboard: &mut Keyboard,
    state: &mut Interaction,
    local: bool,
    captures: &[Capture; CAPTURES],
) -> Result<(), String> {
    let target = if local {
        Target::Local
    } else {
        Target::Windows
    };
    if input.target == target {
        return Ok(());
    }
    dispatch(state.cancel(), platform, input, keyboard, 0, captures)?;
    input.release(platform);
    keyboard.reset();
    input.target = target;
    platform.script(
        KEYBOARD,
        &format!("setContext({})", target == Target::Local),
    )?;
    Ok(())
}

/// Releases everything held: presses, drags, grabs, injected input, modifiers.
fn cancel_all(
    platform: &mut Platform,
    input: &mut OwnedInput,
    keyboard: &mut Keyboard,
    state: &mut Interaction,
    captures: &[Capture; CAPTURES],
) -> Result<(), String> {
    dispatch(state.cancel(), platform, input, keyboard, 0, captures)?;
    input.release(platform);
    keyboard.reset();
    platform.script(KEYBOARD, "resetPointers()")?;
    Ok(())
}

/// `input_test` selects the diagnostic script that injects real OS input.
pub fn run(smoke: bool, diagnostic: bool, input_test: bool) -> Result<i32, String> {
    let mut platform = match Platform::new(!smoke) {
        Ok(platform) => platform,
        Err(CreateError::RuntimeUnavailable) => {
            if diagnostic {
                println!("SteamVR is not running; nothing was launched.");
            }
            return Ok(EXIT_WAIT_FOR_RUNTIME);
        }
        Err(CreateError::Failed(e)) => return Err(e),
    };
    if smoke {
        return smoke_run(&mut platform);
    }
    let mut input = OwnedInput::default();
    let mut keyboard = Keyboard::default();
    let mut state = Interaction::default();
    let result = session(
        &mut platform,
        &mut input,
        &mut keyboard,
        &mut state,
        diagnostic,
        input_test,
    );
    // Cleanup runs for orderly quit, runtime loss and errors, before resources die.
    let none = [Capture::default(); CAPTURES];
    let _ = dispatch(
        state.cancel(),
        &mut platform,
        &mut input,
        &mut keyboard,
        0,
        &none,
    );
    input.release(&mut platform);
    if input.pending() {
        eprintln!("Owned input remains after bounded release attempts");
    }
    keyboard.reset();
    let _ = platform.script(KEYBOARD, "resetPointers()");
    result
}

/// Tracked device index and pose of what an anchor follows, while it is tracked.
fn anchor_pose(anchor: Anchor, f: &Frame) -> Option<(u32, Matrix)> {
    match anchor {
        Anchor::Room => None,
        Anchor::Head => (f.hmd_valid != 0 && f.hmd.rigid()).then_some((0, f.hmd)),
        Anchor::Left | Anchor::Right => {
            let hand = f.hands[anchor.hand()?];
            (hand.valid != 0 && hand.raw.rigid()).then_some((hand.device, hand.raw))
        }
    }
}

/// Width in metres of a capture that is `pixels` wide at scale 1.
fn natural_width(pixels: u32) -> f32 {
    (pixels as f32 / CAPTURE_PIXELS_PER_METRE).clamp(0.3, 2.4)
}

/// What is open now, for the session file or a preset. Window titles are read
/// again so the saved identity matches what the window shows today.
fn current_layout(shell: &Shell, state: &Interaction, platform: &mut Platform) -> Layout {
    Layout {
        name: String::new(),
        keyboard_shown: shell.keyboard,
        keyboard: shell
            .keyboard_placed
            .then(|| (state.panels[KEYBOARD].pose, state.panels[KEYBOARD].width)),
        captures: shell
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| {
                let entry = entry.as_ref().filter(|entry| entry.placed)?;
                Some(SavedCapture {
                    window: entry.window,
                    identity: platform
                        .capture_identity(slot)
                        .unwrap_or_else(|_| entry.identity.clone()),
                    pose: if entry.anchor == Anchor::Room {
                        state.panels[VIEWS + slot].pose
                    } else {
                        entry.relative
                    },
                    scale: entry.scale,
                    size: entry.size,
                    opacity: entry.opacity,
                    curved: entry.curved,
                    fade: entry.fade,
                    anchor: entry.anchor,
                })
            })
            .collect(),
    }
}

/// Turns a slot into a grey placeholder for a source that is not there. The
/// native capture of that slot must already be closed.
fn show_lost(
    slot: usize,
    entry: &mut Slot,
    platform: &mut Platform,
    state: &mut Interaction,
) -> Result<(), String> {
    // Without a known size (never captured) assume a 16:9 display.
    if entry.size.0 == 0 || entry.size.1 == 0 {
        entry.size = (1920, 1080);
    }
    entry.lost = true;
    entry.id = 0;
    let panel = &mut state.panels[VIEWS + slot];
    panel.width = (natural_width(entry.size.0) * entry.scale).clamp(WIDTH_LIMITS.0, WIDTH_LIMITS.1);
    panel.height = panel.width * entry.size.1 as f32 / entry.size.0 as f32;
    let text = if entry.window {
        "Window not open"
    } else {
        "Display lost"
    };
    platform.capture_placeholder(slot, text, entry.size)
}

/// Replaces everything open with a saved layout. A source that is not present
/// now (a closed window, an unplugged display) gets a placeholder panel in its
/// place and is picked up when it appears.
fn apply_layout(
    saved: &Layout,
    shell: &mut Shell,
    platform: &mut Platform,
    input: &mut OwnedInput,
    keyboard: &mut Keyboard,
    state: &mut Interaction,
    captures: &[Capture; CAPTURES],
) -> Result<(), String> {
    cancel_all(platform, input, keyboard, state, captures)?;
    for slot in 0..CAPTURES {
        if shell.slots[slot].take().is_some() {
            platform.capture_close(slot)?;
        }
        state.panels[VIEWS + slot].enabled = false;
    }
    let mut missing = 0;
    for capture in &saved.captures {
        let Some(slot) = shell.free_slot() else {
            break;
        };
        let id = platform
            .source_find(capture.window, &capture.identity)?
            .filter(|id| shell.find(capture.window, *id).is_none());
        let opened = id.filter(
            |id| match platform.capture_open(slot, capture.window, *id) {
                Ok(()) => true,
                Err(e) => {
                    eprintln!("Cannot reopen {}: {e}", capture.identity);
                    false
                }
            },
        );
        state.release_panel(VIEWS + slot);
        state.panels[VIEWS + slot].pose = capture.pose;
        let mut entry = Slot {
            window: capture.window,
            id: opened.unwrap_or(0),
            placed: true,
            scale: capture.scale,
            identity: capture.identity.clone(),
            size: (0, 0),
            lost: false,
            opacity: capture.opacity,
            curved: capture.curved,
            fade: capture.fade,
            free_width: None,
            anchor: capture.anchor,
            relative: capture.pose,
            keep_width: None,
        };
        if opened.is_none() {
            entry.size = capture.size;
            show_lost(slot, &mut entry, platform, state)?;
            missing += 1;
        }
        shell.slots[slot] = Some(entry);
    }
    shell.keyboard = saved.keyboard_shown;
    if let Some((pose, width)) = saved.keyboard {
        let panel = &mut state.panels[KEYBOARD];
        panel.height *= width / panel.width;
        panel.width = width;
        panel.pose = pose;
        shell.keyboard_placed = true;
    }
    if missing > 0 {
        shell.notice = format!(
            "{missing} saved overlay{} waiting for a window or display that is not there.",
            if missing == 1 { " is" } else { "s are" }
        );
    }
    Ok(())
}

/// Reads the preset files again for the Presets screen.
fn refresh_presets(shell: &mut Shell) {
    for (index, entry) in shell.presets.iter_mut().enumerate() {
        *entry = Layout::load(layout::preset_path(index + 1))
            .map(|preset| (preset.name.clone(), preset.summary()));
    }
}

/// Pushes the theme to the views. The other settings are read where they apply.
fn apply_settings(platform: &mut Platform, settings: &Settings) -> Result<(), String> {
    for view in 0..VIEWS {
        platform.script(view, &format!("setTheme(\"{}\")", settings.theme))?;
    }
    Ok(())
}

/// Carries out one validated view command. Returns true to quit.
fn apply(
    command: Command,
    shell: &mut Shell,
    platform: &mut Platform,
    input: &mut OwnedInput,
    keyboard: &mut Keyboard,
    state: &mut Interaction,
    f: &Frame,
) -> Result<bool, String> {
    shell.notice.clear();
    match command {
        Command::Quit => return Ok(true),
        Command::Dashboard => {
            shell.dashboard = !shell.dashboard;
            state.release_panel(DASHBOARD);
            if shell.dashboard {
                if let Some(pose) = geometry::placement(f.hmd, 0.9, 0., -0.12) {
                    state.panels[DASHBOARD].pose = pose;
                }
                let sources = platform.sources()?;
                platform.script(DASHBOARD, &format!("setSources({sources})"))?;
            }
        }
        Command::Keyboard => {
            shell.keyboard = !shell.keyboard;
            cancel_all(platform, input, keyboard, state, &f.captures)?;
            if shell.keyboard {
                if let Some(pose) = geometry::facing(f.hmd, 1.3, 0., -0.6) {
                    state.panels[KEYBOARD].pose = pose;
                    shell.keyboard_placed = true;
                }
            }
        }
        Command::Preset(action, number) => {
            let path = layout::preset_path(number);
            match action {
                PresetAction::Save(name) => {
                    let mut preset = current_layout(shell, state, platform);
                    // Overwriting without a new name keeps the old one.
                    preset.name = if name.is_empty() {
                        Layout::load(path.clone()).map_or(String::new(), |old| old.name)
                    } else {
                        name
                    };
                    if let Err(e) = preset.save(path) {
                        shell.notice = e;
                    }
                }
                PresetAction::Rename(name) => match Layout::load(path.clone()) {
                    Some(mut preset) => {
                        preset.name = name;
                        if let Err(e) = preset.save(path) {
                            shell.notice = e;
                        }
                    }
                    None => shell.notice = format!("Preset {number} is empty."),
                },
                PresetAction::Load => match Layout::load(path) {
                    Some(preset) => apply_layout(
                        &preset,
                        shell,
                        platform,
                        input,
                        keyboard,
                        state,
                        &f.captures,
                    )?,
                    None => shell.notice = format!("Preset {number} is empty."),
                },
                PresetAction::Delete => {
                    if let Some(path) = path {
                        let _ = std::fs::remove_file(path);
                    }
                }
            }
            refresh_presets(shell);
        }
        Command::Refresh => {
            let sources = platform.sources()?;
            platform.script(DASHBOARD, &format!("setSources({sources})"))?;
        }
        Command::Bindings => {
            if let Err(e) = platform.open_bindings() {
                shell.notice = e;
            }
        }
        Command::Open { window, id } => {
            if shell.find(window, id).is_some() {
                shell.notice = "That source is already open.".into();
            } else if let Some(slot) = shell.free_slot() {
                match platform.capture_open(slot, window, id) {
                    Ok(()) => {
                        shell.slots[slot] = Some(Slot {
                            window,
                            id,
                            placed: false,
                            scale: shell.settings.scale as f32 / 100.,
                            identity: platform.capture_identity(slot).unwrap_or_default(),
                            size: (0, 0),
                            lost: false,
                            opacity: shell.settings.opacity,
                            curved: shell.settings.curved,
                            fade: false,
                            free_width: None,
                            anchor: Anchor::Room,
                            relative: Matrix::identity(),
                            keep_width: None,
                        })
                    }
                    Err(e) => shell.notice = e,
                }
            } else {
                shell.notice = format!("All {CAPTURES} capture slots are in use.");
            }
        }
        Command::Close(slot) => {
            if shell.slots[slot].take().is_some() {
                cancel_all(platform, input, keyboard, state, &f.captures)?;
                platform.capture_close(slot)?;
            }
        }
        Command::Set(key, value) => {
            if shell.settings.set(&key, &value) {
                if shell.persist {
                    if let Err(e) = shell.settings.save() {
                        shell.notice = e;
                    }
                }
                apply_settings(platform, &shell.settings)?;
            }
        }
        Command::Overlay(slot, set) => {
            let panel = VIEWS + slot;
            let taken = matches!(set, OverlaySet::Source { window, id } if shell.find(window, id).is_some());
            let Some(entry) = shell.slots[slot].as_mut() else {
                return Ok(false);
            };
            match set {
                OverlaySet::Opacity(opacity) => entry.opacity = opacity,
                OverlaySet::Curved(curved) => entry.curved = curved,
                OverlaySet::Fade(fade) => entry.fade = fade,
                OverlaySet::Anchor(anchor) => {
                    let p = &mut state.panels[panel];
                    let shrunk = p.width;
                    let relative = match anchor {
                        // Stays where it is now.
                        Anchor::Room => Some(Matrix::identity()),
                        _ if !entry.placed => None,
                        Anchor::Head => {
                            anchor_pose(anchor, f).map(|(_, head)| head.inverse().mul(p.pose))
                        }
                        Anchor::Left | Anchor::Right => anchor_pose(anchor, f).map(|(_, hand)| {
                            // Small, just above the controller as it is held
                            // now, facing the user. Grab it to put it elsewhere.
                            (p.width, p.height) = (
                                HAND_WIDTH.min(p.width),
                                p.height * HAND_WIDTH.min(p.width) / p.width,
                            );
                            let centre =
                                geometry::add(hand.col(3), [0., HAND_GAP + p.height / 2., 0.]);
                            let away = geometry::sub(geometry::scale(centre, 2.), f.hmd.col(3));
                            p.pose = geometry::aim(centre, away);
                            hand.inverse().mul(p.pose)
                        }),
                    };
                    match relative {
                        Some(relative) => {
                            // A controller shrinks it; leaving the controller
                            // gives back the size it had before.
                            let p = &mut state.panels[panel];
                            if anchor.hand().is_some() {
                                entry.free_width.get_or_insert(shrunk);
                            } else if let Some(width) = entry.free_width.take() {
                                (p.width, p.height) = (width, p.height * width / p.width);
                            }
                            entry.anchor = anchor;
                            entry.relative = relative;
                            state.release_panel(panel);
                        }
                        None => {
                            shell.notice =
                                "Cannot attach it: the headset or that controller is not tracked right now."
                                    .into()
                        }
                    }
                }
                OverlaySet::Source { .. } if taken => {
                    shell.notice = "That source is already open.".into()
                }
                OverlaySet::Source { window, id } => {
                    cancel_all(platform, input, keyboard, state, &f.captures)?;
                    platform.capture_close(slot)?;
                    match platform.capture_open(slot, window, id) {
                        Ok(()) => {
                            // The panel keeps its width; its height follows the new source.
                            entry.keep_width =
                                (entry.size.0 > 0).then_some(state.panels[panel].width);
                            entry.window = window;
                            entry.id = id;
                            entry.lost = false;
                            entry.size = (0, 0);
                            entry.identity = platform.capture_identity(slot).unwrap_or_default();
                        }
                        // The placeholder search brings the previous source back.
                        Err(e) => {
                            shell.notice = e;
                            show_lost(slot, entry, platform, state)?;
                        }
                    }
                }
            }
        }
    }
    Ok(false)
}

fn session(
    platform: &mut Platform,
    input: &mut OwnedInput,
    keyboard: &mut Keyboard,
    state: &mut Interaction,
    diagnostic: bool,
    input_test: bool,
) -> Result<i32, String> {
    let start = Instant::now();
    let demo = if input_test { DEMO_INPUT } else { DEMO };
    let mut shell = Shell::new(Settings::load());
    let mut loaded = false;
    let mut active_before = false;
    let mut last_keyboard = String::new();
    let mut last_shell = String::new();
    let mut last_clock = String::new();
    let mut readiness = None;
    let mut last_tick = Instant::now();
    let mut next_refresh = Instant::now();
    let mut refreshes = 0u64;
    let mut samples = 0u64;
    // Time spent in capture copy, UI render and texture submission.
    let mut frame_time = Duration::ZERO;
    let mut frame_worst = Duration::ZERO;
    // Last session file contents written, and when to compare again.
    let mut saved_session = String::new();
    let mut next_save = Instant::now();
    // When to look again for sources whose panels show a placeholder.
    let mut next_search = Instant::now();
    // Last placement sent per panel; unchanged placements are not resent.
    let mut sent: [Option<(Option<u32>, Matrix, f32)>; PANELS] = [None; PANELS];
    // Last hover position sent to the dashboard and wrist views.
    let mut hovers = [None; VIEWS];
    // Diagnostic script state: start time, where the clock is shown, the
    // current step and the panel UV its virtual laser aims at.
    // Script time advances only while the headset is tracked.
    let mut demo_elapsed = 0f32;
    let mut demo_wrist: Option<Matrix> = None;
    let mut demo_step = usize::MAX;
    let mut demo_target: Option<(usize, f32, f32)> = None;
    let mut demo_done = false;
    // Commands from controller buttons, applied with the views' commands.
    let mut button_commands: Vec<Command> = Vec::new();
    // An overlay text field had focus in the previous iteration.
    let mut field_focused = false;
    // Rendered frames during Sweep steps: (keyboard resubmitted, total).
    let mut sweep_frames = (0u32, 0u32);
    // During a Grab step: the virtual hand's pose and the panel's position at its start.
    let mut demo_grab: Option<(Matrix, geometry::Vec3)> = None;
    let mut frame_pending = false;
    let mut steady = [Steady::default(); 2];
    // Per panel: how much of its opacity the fade leaves (0..1), and the
    // curvature and opacity last sent to SteamVR.
    let mut fades = [1f32; PANELS];
    let mut styled: [Option<(f32, f32)>; PANELS] = [None; PANELS];
    // The hide button was pressed: everything but the wrist clock is hidden.
    let mut hidden = false;
    // Until when each hidden menu is still drawn, easing out.
    let mut vanish_until = [Instant::now(); VIEWS];
    // Capture slot the quick bar hangs under and when a laser last pointed
    // at that capture or the bar; the last state sent to the bar's page.
    let mut bar: Option<(usize, Instant)> = None;
    let mut last_bar = String::new();
    // Views whose panel was shown in the previous iteration.
    let mut shown = [false; VIEWS];
    shell.persist = !diagnostic;
    platform.dry_run = diagnostic && !input_test;
    println!(
        "Wrist clock on the left controller: point the other controller at its button and pull the trigger to open the dashboard. Trigger clicks/types (right-clicks while the thumb rests on B/Y), thumbstick scrolls, grip moves a panel (thumbstick while gripping: farther/closer), holding B/Y shows or hides the keyboard."
    );
    if diagnostic {
        println!(
            "Scripted diagnostic: a virtual right-hand laser works the wrist button and dashboard in the live SteamVR session. Real controllers are ignored, OS input is not injected and settings are not saved."
        );
    }
    loop {
        let tick = Instant::now();
        let dt = tick.duration_since(last_tick).as_secs_f32().min(0.1);
        last_tick = tick;
        let mut f = platform.sample()?;
        for (h, hand) in f.hands.iter_mut().enumerate() {
            if hand.valid != 0 {
                let strength = shell.settings.smoothing as f32 / 100.;
                hand.pose = steady[h].apply(hand.pose, strength, dt);
            } else {
                steady[h] = Steady::default();
            }
        }
        // `painted` describes the frame rendered in the previous iteration.
        if std::mem::take(&mut frame_pending)
            && demo
                .get(demo_step)
                .is_some_and(|d| matches!(d, Demo::Sweep(..)))
        {
            sweep_frames.1 += 1;
            sweep_frames.0 += (f.painted & 1 << KEYBOARD != 0) as u32;
        }
        samples += 1;
        let current_readiness = (
            f.dashboard,
            f.hmd_valid,
            f.ui_ready,
            f.hands.map(|h| (h.valid, h.device, h.actions_active)),
            f.action_error,
            f.captures.map(|c| (c.open, c.ready)),
        );
        if readiness != Some(current_readiness) {
            println!(
                "Readiness: dashboard={} HMD={} views={:#05b} hands(valid,device,actions)={:?} action_error={} captures(open,ready)={:?} (action bits: trigger=1 grip=2 right=4 scroll=8 keyboard=16)",
                f.dashboard,
                f.hmd_valid,
                f.ui_ready,
                current_readiness.3,
                f.action_error,
                current_readiness
                    .5
                    .iter()
                    .filter(|c| c.0 != 0)
                    .collect::<Vec<_>>()
            );
            readiness = Some(current_readiness);
        }
        if f.quit != 0 {
            return Ok(EXIT_WAIT_FOR_RUNTIME);
        }
        if f.ui_ready == (1 << VIEWS) - 1 && !loaded {
            platform.script(KEYBOARD, &keyboard::layout_script())?;
            apply_settings(platform, &shell.settings)?;
            // Menus start out eased away, so the first showing eases in too.
            for view in [KEYBOARD, DASHBOARD, BAR] {
                platform.script(view, "document.body.className='vanish'")?;
            }
            loaded = true;
            // Bring back what was open when the overlay last ran.
            if shell.persist {
                refresh_presets(&mut shell);
                if let Some(session) = Layout::load(layout::session_path()) {
                    apply_layout(
                        &session,
                        &mut shell,
                        platform,
                        input,
                        keyboard,
                        state,
                        &f.captures,
                    )?;
                }
            }
        }
        // Panels hide while the SteamVR dashboard is open. The diagnostic takes
        // no input, so it shows them regardless and can run unattended.
        let head = (diagnostic || f.dashboard == 0) && f.hmd_valid != 0 && f.hmd.rigid();
        let active = head && loaded;
        if active != active_before {
            cancel_all(platform, input, keyboard, state, &f.captures)?;
            active_before = active;
        }
        // A changed play area invalidates room positions: bring open panels
        // back in front of the user.
        if f.recenter != 0 && active {
            cancel_all(platform, input, keyboard, state, &f.captures)?;
            // Overlays attached to the headset or a controller are unaffected.
            for slot in shell.slots.iter_mut().flatten() {
                slot.placed &= slot.anchor != Anchor::Room;
            }
            if let Some(pose) = geometry::placement(f.hmd, 0.9, 0., -0.12) {
                state.panels[DASHBOARD].pose = pose;
            }
            if let Some(pose) = geometry::facing(f.hmd, 1.3, 0., -0.6) {
                state.panels[KEYBOARD].pose = pose;
            }
        }
        // Capture slots: drop closed sources, size and place new or resized ones.
        let search = loaded && tick >= next_search;
        if search {
            next_search = tick + Duration::from_secs(2);
        }
        for slot in 0..CAPTURES {
            let panel = VIEWS + slot;
            let c = f.captures[slot];
            if shell.slots[slot].is_some() && c.open == 2 {
                cancel_all(platform, input, keyboard, state, &f.captures)?;
                platform.capture_close(slot)?;
                // A closed window is gone for good. A display may come back
                // (powered off, virtual display recreated): keep its panel.
                match shell.slots[slot].as_mut().filter(|entry| !entry.window) {
                    Some(entry) => show_lost(slot, entry, platform, state)?,
                    None => {
                        shell.slots[slot] = None;
                        shell.notice = "A captured window was closed.".into();
                    }
                }
            }
            if search && shell.slots[slot].as_ref().is_some_and(|entry| entry.lost) {
                let (window, identity) = shell.slots[slot]
                    .as_ref()
                    .map(|entry| (entry.window, entry.identity.clone()))
                    .unwrap();
                let id = platform
                    .source_find(window, &identity)?
                    .filter(|id| shell.find(window, *id).is_none());
                if let Some(id) = id {
                    if platform.capture_open(slot, window, id).is_ok() {
                        let entry = shell.slots[slot].as_mut().unwrap();
                        entry.id = id;
                        entry.lost = false;
                        // Take the display's current name, which may have changed.
                        entry.identity = platform.capture_identity(slot).unwrap_or(identity);
                    }
                }
            }
            if shell.slots[slot]
                .as_ref()
                .is_some_and(|entry| c.ready != 0 && entry.size != (c.width, c.height))
            {
                // Held input was aimed at the old size.
                cancel_all(platform, input, keyboard, state, &f.captures)?;
            }
            let Some(entry) = &mut shell.slots[slot] else {
                state.panels[panel].enabled = false;
                state.panels[panel].passive = false;
                continue;
            };
            state.panels[panel].passive = entry.lost;
            if c.ready != 0 && entry.size != (c.width, c.height) {
                entry.size = (c.width, c.height);
                let width = entry
                    .keep_width
                    .take()
                    .unwrap_or(natural_width(c.width) * entry.scale)
                    .clamp(WIDTH_LIMITS.0, WIDTH_LIMITS.1);
                state.panels[panel].width = width;
                state.panels[panel].height = width * c.height as f32 / c.width as f32;
            }
            if (c.ready != 0 || entry.lost) && !entry.placed && active {
                // Later slots cascade right and down so panels do not coincide.
                let offset = slot as f32;
                let distance = shell.settings.distance as f32 / 100.;
                if let Some(pose) =
                    geometry::placement(f.hmd, distance, 0.15 * offset, 0.15 - 0.08 * offset)
                {
                    state.release_panel(panel);
                    state.panels[panel].pose = pose;
                    entry.placed = true;
                }
            }
            state.panels[panel].enabled =
                active && !hidden && entry.placed && (c.ready != 0 || entry.lost);
        }
        state.panels[KEYBOARD].enabled = active && !hidden && shell.keyboard;
        state.panels[DASHBOARD].enabled = active && !hidden && shell.dashboard;
        // The wrist panel is drawn attached to the controller by the
        // compositor; this world pose is only for hit testing.
        if diagnostic {
            f.hands = [Hand::default(); 2];
            if active {
                if demo_wrist.is_none() {
                    demo_wrist = geometry::placement(f.hmd, 0.7, 0.45, 0.1);
                }
                demo_elapsed += dt;
                let elapsed = demo_elapsed;
                let step = (elapsed / DEMO_STEP_SECONDS) as usize;
                let phase = elapsed - step as f32 * DEMO_STEP_SECONDS;
                demo_done = step >= demo.len();
                if !demo_done {
                    if step != demo_step {
                        demo_step = step;
                        // Only a Grab step sets `demo_grab`, so a previous step exists.
                        if let Some((_, start)) = demo_grab.take() {
                            if let Demo::Grab(panel) = demo[step - 1] {
                                let moved = geometry::sub(state.panels[panel].pose.col(3), start);
                                println!(
                                    "Diagnostic: grab moved the panel {:.2} m",
                                    geometry::dot(moved, moved).sqrt()
                                );
                            }
                        }
                        if step > 0 && matches!(demo[step - 1], Demo::Scroll(..)) {
                            println!(
                                "Diagnostic: Add list scrolled to {} px",
                                platform.script(
                                    DASHBOARD,
                                    "String(document.querySelector('#tab-add .scroll').scrollTop)"
                                )?
                            );
                        }
                        demo_target = match demo[step] {
                            Demo::Click(panel, selector) | Demo::Scroll(panel, selector) => {
                                locate(platform, panel, selector).ok().map(|(x, y)| {
                                    (panel, x / VIEW_SIZE[panel].0, y / VIEW_SIZE[panel].1)
                                })
                            }
                            Demo::Hover(panel) => Some((panel, 0.9, 0.04)),
                            Demo::ClickAt(panel, u, v) => Some((panel, u, v)),
                            Demo::Sweep(panel, v) => Some((panel, 0.05, v)),
                            Demo::Grab(panel) => Some((panel, 0.5, 0.5)),
                        };
                        // An unattended run keeps the SteamVR dashboard open, and it
                        // covers the keyboard's usual place below eye level. Sweep at
                        // eye level so headset screenshots show the keys.
                        if let Demo::Sweep(panel, _) = demo[step] {
                            if step == 0 || !matches!(demo[step - 1], Demo::Sweep(..)) {
                                if let Some(pose) = geometry::placement(f.hmd, 1.3, 0., 0.05) {
                                    state.release_panel(panel);
                                    state.panels[panel].pose = pose;
                                }
                            }
                        }
                        println!(
                            "Diagnostic step {step}: {:?} -> target {demo_target:?}",
                            demo[step]
                        );
                    }
                    if let Some((panel, mut u, v)) = demo_target {
                        if let Demo::Sweep(..) = demo[step] {
                            u += 0.9 * phase / DEMO_STEP_SECONDS;
                        }
                        let p = state.panels[panel];
                        let target = p.point(u, v);
                        let origin = f.hmd.point([0.15, -0.2, -0.05]);
                        let pressing = (0.5..0.8).contains(&phase);
                        let mut pose = geometry::aim(origin, target);
                        let grabbing = matches!(demo[step], Demo::Grab(..));
                        if grabbing {
                            // Keep the hand's starting orientation and carry it right.
                            let (start, _) = *demo_grab.get_or_insert((pose, p.pose.col(3)));
                            let shift = ((phase - 0.3) / 0.6).clamp(0., 1.) * 0.3;
                            pose = start;
                            for (i, axis) in f.hmd.col(0).into_iter().enumerate() {
                                pose.m[4 * i + 3] += axis * shift;
                            }
                        }
                        f.hands[1] = Hand {
                            pose,
                            // The laser overlays attach to a tracked device; the
                            // headset stands in for the absent controller.
                            raw: f.hmd,
                            valid: 1,
                            device: 0,
                            actions_active: 15,
                            trigger: (matches!(demo[step], Demo::Click(..) | Demo::ClickAt(..))
                                && pressing) as u32,
                            grip: (grabbing && phase > 0.15) as u32,
                            scroll: if matches!(demo[step], Demo::Scroll(..)) && phase > 0.4 {
                                -1.
                            } else {
                                0.
                            },
                            ..Default::default()
                        };
                    }
                }
            }
        }
        let left = f.hands[0];
        // Where the clock is in the room, for hit testing. The diagnostic has
        // no left controller and stands the clock beside the dashboard.
        let wrist_world = if diagnostic {
            demo_wrist.filter(|_| active)
        } else {
            (active && left.valid != 0 && left.raw.rigid()).then(|| left.raw.mul(WRIST_POSE))
        };
        let wrist = wrist_world.is_some();
        state.panels[WRIST].enabled = wrist;
        if let Some(pose) = wrist_world {
            state.panels[WRIST].pose = pose;
        }
        // Editable focus is queried from the views. Keyboard keys are hit-tested
        // per hand without mouse events, so they never replace that target.
        route(
            platform,
            input,
            keyboard,
            state,
            active && f.local_focus != 0,
            &f.captures,
        )?;
        // Clicking into a text field on a panel brings up the keyboard.
        let focused = active && f.local_focus != 0;
        if focused && !field_focused && !shell.keyboard {
            button_commands.push(Command::Keyboard);
        }
        field_focused = focused;
        // Overlays attached to the headset or a controller follow it, drawn by
        // the compositor relative to that device. While a grab moves one, the
        // grab decides where it is and its attachment is updated to match.
        let mut attached: [Option<(u32, Matrix)>; PANELS] = [None; PANELS];
        for (slot, entry) in shell.slots.iter_mut().enumerate() {
            let panel = VIEWS + slot;
            let Some(entry) = entry.as_mut().filter(|e| e.anchor != Anchor::Room) else {
                continue;
            };
            match anchor_pose(entry.anchor, &f) {
                Some((_, base)) if state.grabbed(panel) => {
                    entry.relative = base.inverse().mul(state.panels[panel].pose)
                }
                Some((device, base)) => {
                    state.panels[panel].pose = base.mul(entry.relative);
                    attached[panel] = Some((device, entry.relative));
                }
                None => {
                    state.release_panel(panel);
                    state.panels[panel].enabled = false;
                }
            }
        }
        if bar.is_some_and(|(slot, seen)| {
            !state.panels[VIEWS + slot].enabled || tick.duration_since(seen) > BAR_LINGER
        }) {
            bar = None;
            // Next time it opens with only its two small buttons.
            platform.script(BAR, "collapse()")?;
        }
        state.panels[BAR].enabled = bar.is_some();
        if let Some((slot, _)) = bar {
            let capture = state.panels[VIEWS + slot];
            let width = (capture.width * 0.6).clamp(0.4, 1.);
            let height = width * VIEW_SIZE[BAR].1 / VIEW_SIZE[BAR].0;
            // Centred under the capture's lower edge, a little toward the viewer.
            let mut under = Matrix::identity();
            under.m[7] = -(capture.height / 2. + BAR_GAP + height / 2.);
            under.m[11] = 0.01;
            let panel = &mut state.panels[BAR];
            (panel.width, panel.height) = (width, height);
            panel.pose = capture.pose.mul(under);
            attached[BAR] =
                attached[VIEWS + slot].map(|(device, relative)| (device, relative.mul(under)));
        }
        let hits: [Option<Hit>; 2] = std::array::from_fn(|h| {
            if !active || f.hands[h].valid == 0 {
                return None;
            }
            let mut panels = state.panels;
            // The left hand cannot point at its own wrist.
            panels[WRIST].enabled &= h != 0;
            // Nor at an overlay it carries, nor at a panel faded out of sight.
            for (slot, entry) in shell.slots.iter().enumerate() {
                if let Some(entry) = entry {
                    panels[VIEWS + slot].enabled &= entry.anchor.hand() != Some(h);
                }
            }
            for (panel, style) in panels.iter_mut().zip(styled) {
                panel.enabled &= style.is_none_or(|(_, alpha)| alpha > POINTABLE_ALPHA);
            }
            geometry::nearest(f.hands[h].pose, &panels)
        });
        // The quick bar stays with its capture while a laser is on either,
        // otherwise it moves to whichever capture a laser points at.
        let pointed = |panel: usize| hits.iter().flatten().any(|hit| hit.panel == panel);
        let kept = bar.is_some_and(|(slot, _)| pointed(BAR) || pointed(VIEWS + slot));
        if let (Some((_, seen)), true) = (&mut bar, kept) {
            *seen = tick;
        } else if state.ui_owner(BAR).is_none() {
            if let Some(hit) = hits.iter().flatten().find(|hit| hit.panel >= VIEWS) {
                bar = Some((hit.panel - VIEWS, tick));
            }
        }
        let mut keys = [None; 2];
        if loaded && shell.keyboard {
            for h in 0..2 {
                keys[h] = keyboard_pointer(platform, h, hits[h])?;
            }
        }
        // Dispatch runs scripts in the views, so it waits for them to load.
        if loaded {
            let now = start.elapsed().as_millis() as u64;
            let actions = state.update(f.hands, hits, keys, active && !input.blocked, dt);
            if actions.contains(&Action::ToggleKeyboard) {
                button_commands.push(Command::Keyboard);
            }
            if actions.contains(&Action::ToggleOverlays) {
                hidden = !hidden;
                cancel_all(platform, input, keyboard, state, &f.captures)?;
            }
            // A modifier held by one hand stays down in Windows while the
            // other hand types (Alt codes, Win shortcuts).
            input.hold(platform, keyboard.held_modifiers());
            dispatch(actions, platform, input, keyboard, now, &f.captures)?;
            repeat_keys(platform, input, keyboard, now);
            // The scale gesture changes panel widths; remember it per capture.
            for (slot, entry) in shell.slots.iter_mut().enumerate() {
                if let Some(entry) = entry.as_mut().filter(|entry| entry.size.0 > 0) {
                    entry.scale = state.panels[VIEWS + slot].width / natural_width(entry.size.0);
                }
            }
            if input.blocked {
                input.release(platform);
                keyboard.reset();
            }
            // Hover feedback for dashboard and wrist buttons. While a hand
            // presses there, the interaction policy already sends its moves.
            for view in [DASHBOARD, WRIST, BAR] {
                let hover = if state.ui_owner(view).is_some() {
                    hovers[view]
                } else {
                    let (w, h) = VIEW_SIZE[view];
                    hits.iter()
                        .flatten()
                        .find(|hit| hit.panel == view)
                        .map(|hit| ((hit.u * w).round() as i32, (hit.v * h).round() as i32))
                };
                if hover != hovers[view] {
                    let (x, y) = hover.unwrap_or((-100, -100));
                    platform.mouse(view, x, y, 0)?;
                    hovers[view] = hover;
                }
            }
        }
        // Opacity of every panel and shape of every capture. A panel set to fade
        // when turned away dims as its front leaves the viewer's side.
        for panel in 0..PANELS {
            let (opacity, curved, fade) = match panel {
                KEYBOARD => (100, false, shell.settings.fade_keyboard),
                DASHBOARD => (100, false, shell.settings.fade_dashboard),
                WRIST => (100, false, shell.settings.fade_clock),
                BAR => (100, false, false),
                _ => match &shell.slots[panel - VIEWS] {
                    Some(entry) => (entry.opacity, entry.curved, entry.fade),
                    None => {
                        fades[panel] = 1.;
                        continue;
                    }
                },
            };
            let p = &mut state.panels[panel];
            p.curvature = if curved { CURVED } else { 0. };
            let target = if fade && p.enabled {
                let front = geometry::frontness(p.pose, f.hmd.col(3));
                ((front - TURN_GONE) / (TURN_FULL - TURN_GONE)).clamp(0., 1.)
            } else {
                1.
            };
            fades[panel] += (target - fades[panel]) * response(dt, FADE_TIME_CONSTANT);
            // Whole percent steps, so a settled fade stops sending updates.
            let style = (p.curvature, (opacity as f32 * fades[panel]).round() / 100.);
            if styled[panel] != Some(style) {
                platform.style(panel, style.0, style.1)?;
                styled[panel] = Some(style);
            }
        }
        if loaded && tick >= next_refresh {
            // Controller buttons first, then what the views queued.
            let mut commands = std::mem::take(&mut button_commands);
            for view in 0..VIEWS {
                if state.panels[view].enabled {
                    commands.extend(Command::parse_all(
                        &platform.script(view, "drainActions()")?,
                    ));
                }
            }
            for command in commands {
                if diagnostic {
                    println!("Diagnostic command from a view: {command:?}");
                }
                if apply(command, &mut shell, platform, input, keyboard, state, &f)? {
                    if !diagnostic {
                        return Ok(0);
                    }
                    // The run ends below, after its summary.
                    println!("Diagnostic: Quit requested; ending the run.");
                    demo_done = true;
                }
            }
            let script = keyboard.state_script();
            if script != last_keyboard {
                platform.script(KEYBOARD, &script)?;
                last_keyboard = script;
            }
            // Keep the session file current, so a crash or power loss restores
            // the same overlays. Not while a panel is being moved or sized.
            if shell.persist && tick >= next_save && !state.engaged(0) && !state.engaged(1) {
                next_save = tick + Duration::from_secs(1);
                let session = current_layout(&shell, state, platform);
                let text = session.serialize();
                if text != saved_session {
                    match session.save(layout::session_path()) {
                        Ok(()) => saved_session = text,
                        Err(e) => eprintln!("{e}"),
                    }
                }
            }
            let entry = bar.and_then(|(slot, _)| Some((slot, shell.slots[slot].as_ref()?)));
            if let Some((slot, entry)) = entry {
                let script = format!(
                    "setBar({{slot:{slot},opacity:{},curved:{},fade:{},anchor:\"{}\"}})",
                    entry.opacity,
                    entry.curved,
                    entry.fade,
                    entry.anchor.name()
                );
                if script != last_bar {
                    platform.script(BAR, &script)?;
                    last_bar = script;
                }
            }
            let script = shell.state_script(&f.captures);
            if script != last_shell {
                platform.script(DASHBOARD, &script)?;
                last_shell = script;
            }
            let script = format!(
                "setClock(\"{}\",\"{}\")",
                shell.settings.time(f.hour, f.minute),
                settings::duration(f.session_seconds)
            );
            if script != last_clock {
                platform.script(DASHBOARD, &script)?;
                platform.script(WRIST, &script)?;
                last_clock = script;
            }
        }
        // A menu eases in when shown and out when hidden (ui/theme.css). Not
        // the clock, which comes and goes with controller tracking. A hidden
        // page is left eased away, so the next showing starts from nothing.
        for view in [KEYBOARD, DASHBOARD, BAR] {
            let enabled = state.panels[view].enabled;
            if loaded && enabled != shown[view] {
                let class = if enabled { "appear" } else { "vanish" };
                platform.script(view, &format!("document.body.className='{class}'"))?;
                vanish_until[view] = tick + if enabled { Duration::ZERO } else { VANISH };
            }
            shown[view] = enabled;
        }
        // Submit updated transforms in this input iteration.
        for (id, panel) in state.panels.iter().enumerate() {
            let mut placement = panel.enabled.then_some((None, panel.pose, panel.width));
            if id == WRIST {
                placement = match wrist_world {
                    Some(pose) if diagnostic => Some((None, pose, panel.width)),
                    Some(_) => Some((Some(left.device), WRIST_POSE, panel.width)),
                    None => None,
                };
            } else if let (Some((device, relative)), true) = (attached[id], panel.enabled) {
                placement = Some((Some(device), relative, panel.width));
            } else if id < VIEWS && !panel.enabled && tick < vanish_until[id] {
                // Still drawn where it was while its page eases out.
                placement = sent[id];
            }
            if placement != sent[id] {
                let (device, pose, width) = placement.unwrap_or((None, panel.pose, panel.width));
                platform.panel(id, device, pose, width, placement.is_some())?;
                sent[id] = placement;
            }
        }
        for h in 0..2 {
            let wanted = active && (hits[h].is_some() || state.engaged(h));
            laser(platform, h, f.hands[h], hits[h], f.hmd.col(3), wanted)?;
        }
        if tick >= next_refresh {
            let before = Instant::now();
            platform.frame(head, head || !loaded)?;
            frame_pending = true;
            frame_time += before.elapsed();
            frame_worst = frame_worst.max(before.elapsed());
            refreshes += 1;
            // Rendering consumes this interval rather than adding another full
            // interval after its completion. Missed deadlines never catch up in a burst.
            next_refresh = tick + Duration::from_secs_f64(1. / 60.);
        }
        if diagnostic && (demo_done || start.elapsed() >= Duration::from_secs(90)) {
            if !demo_done {
                println!("Diagnostic script did not finish: the headset was never tracked.");
            }
            println!(
                "Diagnostic cadence: {} samples, {} refreshes in {:.3}s (targets: active input 120 Hz, refresh <=60 Hz). Capture/render/submit: mean {:.2} ms, worst {:.2} ms.",
                samples,
                refreshes,
                start.elapsed().as_secs_f64(),
                frame_time.as_secs_f64() * 1000. / refreshes.max(1) as f64,
                frame_worst.as_secs_f64() * 1000.
            );
            if sweep_frames.1 > 0 {
                println!(
                    "Diagnostic sweep: keyboard repainted and resubmitted on {} of {} rendered frames while the laser crossed it.",
                    sweep_frames.0, sweep_frames.1
                );
            }
            println!(
                "Diagnostic finished: own overlays created/submitted/destroyed; physical pose/action delivery unverified unless readiness above was valid."
            );
            return Ok(0);
        }
        let interval = input_interval(active || !loaded);
        // Account for work; no extra fixed delay after a slow frame and no spin.
        std::thread::sleep(interval.saturating_sub(tick.elapsed()));
    }
}

/// One step of the diagnostic script: what the virtual laser does for
/// `DEMO_STEP_SECONDS`. Selectors are resolved in the panel's HTML view.
#[derive(Clone, Copy, Debug)]
enum Demo {
    Click(usize, &'static str),
    Scroll(usize, &'static str),
    /// Point near the top right corner of a panel without pressing.
    Hover(usize),
    /// Click at a UV position, for capture panels, which have no selectors.
    ClickAt(usize, f32, f32),
    /// Cross a panel left to right at height `v` without pressing, so key
    /// highlights can be compared with the laser dot.
    Sweep(usize, f32),
    /// Grip the panel's centre and carry it 0.3 m to the right.
    Grab(usize),
}
const DEMO_STEP_SECONDS: f32 = 1.2;
const DEMO: &[Demo] = &[
    Demo::Click(WRIST, "#summon"),
    Demo::Click(DASHBOARD, "[data-tab=\"add\"]"),
    Demo::Click(DASHBOARD, "#displays .source"),
    Demo::Click(DASHBOARD, "#windows .source"),
    Demo::Scroll(DASHBOARD, "#windows"),
    Demo::Click(DASHBOARD, "[data-tab=\"settings\"]"),
    Demo::Click(DASHBOARD, "[data-set=\"curved:1\"]"),
    Demo::Click(DASHBOARD, "[data-set=\"theme:light\"]"),
    Demo::Click(DASHBOARD, "[data-set=\"clock:12\"]"),
    Demo::Click(DASHBOARD, "[data-tab=\"overlays\"]"),
    Demo::Click(DASHBOARD, "#keyboard"),
    // Hide the dashboard so it cannot catch the sweeping laser. The pause
    // leaves the keyboard where it spawned long enough for a screenshot.
    Demo::Click(WRIST, "#summon"),
    Demo::Hover(WRIST),
    Demo::Hover(WRIST),
    Demo::Sweep(KEYBOARD, 0.66),
    Demo::Sweep(KEYBOARD, 0.66),
    Demo::Sweep(KEYBOARD, 0.66),
    Demo::Sweep(KEYBOARD, 0.66),
    Demo::Sweep(KEYBOARD, 0.66),
    Demo::Click(WRIST, "#summon"),
    Demo::Grab(DASHBOARD),
    // The quick bar under a capture just pointed at, with the dashboard out of the way.
    Demo::Click(WRIST, "#summon"),
    Demo::Hover(VIEWS),
    Demo::Click(BAR, "#options"),
    Demo::Click(BAR, "#less"),
    Demo::Click(BAR, "[data-anchor=\"head\"]"),
    Demo::Hover(BAR),
    Demo::Click(BAR, "[data-anchor=\"head\"]"),
    Demo::Click(WRIST, "#summon"),
    // One overlay's own properties: translucent, curved, then following the head.
    Demo::Click(DASHBOARD, "[data-act=\"less\"]"),
    Demo::Click(DASHBOARD, "[data-act=\"curved\"]"),
    Demo::Click(DASHBOARD, "[data-act=\"head\"]"),
    Demo::Hover(DASHBOARD),
    Demo::Click(DASHBOARD, "[data-act=\"head\"]"),
    // Another window in the same panel.
    Demo::Click(DASHBOARD, "[data-act=\"source\"]"),
    Demo::Click(DASHBOARD, "#windows .source:not(.open)"),
    Demo::Hover(DASHBOARD),
    // Removing takes two clicks.
    Demo::Click(DASHBOARD, "[data-act=\"close\"]"),
    Demo::Click(DASHBOARD, "[data-act=\"close\"]"),
    Demo::Click(DASHBOARD, "[data-tab=\"settings\"]"),
    Demo::Click(DASHBOARD, "#bindings"),
    // Close and reopen from the wrist, then quit from the dashboard.
    Demo::Click(WRIST, "#summon"),
    Demo::Click(WRIST, "#summon"),
    Demo::Click(DASHBOARD, "#quit"),
    Demo::Click(DASHBOARD, "#quit"),
];
/// `--diagnostic-input`: real OS input. Captures the Notepad window titled
/// `wanderoverlay-input-test...`, clicks into it, types "hi" and presses Ctrl+S.
/// Open that file in Notepad first; the caller then checks the saved file.
const DEMO_INPUT: &[Demo] = &[
    Demo::Click(WRIST, "#summon"),
    Demo::Click(DASHBOARD, "[data-tab=\"add\"]"),
    Demo::Click(
        DASHBOARD,
        "#windows .source[data-title^=\"wanderoverlay-input-test\"]",
    ),
    Demo::Click(DASHBOARD, "[data-tab=\"overlays\"]"),
    Demo::Click(DASHBOARD, "#keyboard"),
    Demo::Click(WRIST, "#summon"),
    Demo::ClickAt(VIEWS, 0.5, 0.6),
    Demo::Click(KEYBOARD, "[data-key=\"72\"]"),
    Demo::Click(KEYBOARD, "[data-key=\"73\"]"),
    Demo::Click(KEYBOARD, "[data-key=\"162\"]"),
    Demo::Click(KEYBOARD, "[data-key=\"83\"]"),
    Demo::Hover(VIEWS),
];

/// Shows one hand's beam and endpoint while it points at a panel or holds a
/// press/grab. Both are attached to the controller so the compositor keeps
/// them on it between loop iterations; geometry is computed in world space
/// and then made relative to the device pose.
fn laser(
    platform: &mut Platform,
    h: usize,
    hand: Hand,
    hit: Option<Hit>,
    eye: geometry::Vec3,
    wanted: bool,
) -> Result<(), String> {
    if !(wanted && hand.valid != 0 && hand.pose.rigid() && hand.raw.rigid()) {
        return platform.laser(
            h,
            0,
            Matrix::identity(),
            Matrix::identity(),
            1.,
            false,
            false,
        );
    }
    let length = hit.map_or(ENGAGED_BEAM_LENGTH, |hit| hit.distance);
    let to_device = hand.raw.inverse();
    let beam = to_device.mul(geometry::beam(hand.pose, length, eye));
    let dot = to_device.mul(geometry::endpoint(hand.pose, length, eye));
    platform.laser(
        h,
        hand.device,
        beam,
        dot,
        length,
        true,
        crate::interaction::right_mode(&hand),
    )
}

fn smoke_run(platform: &mut Platform) -> Result<i32, String> {
    let start = Instant::now();
    platform.capture_open(0, false, 0)?;
    let mut f;
    loop {
        platform.frame(true, true)?;
        f = platform.sample()?;
        if f.ui_ready == (1 << VIEWS) - 1
            && f.captures[0].ready != 0
            && start.elapsed() > Duration::from_secs(2)
        {
            break;
        }
        if start.elapsed() > Duration::from_secs(10) {
            return Err(format!(
                "Startup timed out: views={:#05b} capture={:?}",
                f.ui_ready, f.captures[0]
            ));
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    platform.script(KEYBOARD, &keyboard::layout_script())?;
    let mut keyboard = Keyboard::default();
    platform.script(KEYBOARD, &keyboard.state_script())?;
    let count = platform.script(
        KEYBOARD,
        "document.querySelectorAll('[data-key]').length.toString()",
    )?;
    if count.parse::<usize>().ok() != Some(keyboard::layout().len()) {
        return Err("ANSI key layout incomplete".into());
    }
    let fits = platform.script(KEYBOARD, "Array.from(document.querySelectorAll('[data-key]')).every(b=>{const r=b.getBoundingClientRect();return r.left>=0&&r.top>=0&&r.right<=1760&&r.bottom<=540})?'yes':'no'")?;
    if fits != "yes" {
        return Err("Keyboard keys clipped logical view".into());
    }
    // Real rendered DOM hit testing through the same per-pointer presentation
    // bridge used by controllers. Rust owns acceptance and key semantics.
    for (h, vk) in [(0, 65), (1, 66)] {
        let at = locate(platform, KEYBOARD, &format!("[data-key=\"{vk}\"]"))?;
        if keyboard_pointer(platform, h, Some(ui_hit(KEYBOARD, at)))? != Some(vk) {
            return Err("Dual pointer hit test failed".into());
        }
        keyboard.down(h, vk, 0);
        platform.script(KEYBOARD, &format!("pointerPress({h},{vk})"))?;
    }
    if platform.script(
        KEYBOARD,
        "document.querySelectorAll('.pressed').length.toString()",
    )? != "2"
    {
        return Err("Independent visual presses failed".into());
    }
    keyboard.cancel(0);
    platform.script(KEYBOARD, "pointerCancel(0)")?;
    if keyboard.up(0, 65, 0).is_some() {
        return Err("Cancelled press emitted key".into());
    }
    keyboard.reset();
    platform.script(KEYBOARD, "resetPointers()")?;

    // A view is resubmitted to SteamVR only when it was repainted. While a
    // pointer moves, the session loop changes the keyboard right before every
    // frame, and each of those frames must report the repaint or the key
    // feedback stops updating. An unchanged view must not report one.
    let repainted = |platform: &mut Platform, change: &str| {
        // Lets the 60 FPS cap release the next frame.
        std::thread::sleep(Duration::from_millis(17));
        if !change.is_empty() {
            platform.script(KEYBOARD, change)?;
        }
        platform.frame(false, true)?;
        Ok::<_, String>(platform.sample()?.painted & 1 << KEYBOARD != 0)
    };
    repainted(platform, "")?;
    let idle = repainted(platform, "")?;
    let mut moving = true;
    let mut at = String::new();
    for step in 0..6 {
        at = format!("pointerAt(0,{},210,true)", 500 + 9 * step);
        moving &= repainted(platform, &at)?;
    }
    // The session loop repeats the same call every iteration while a pointer rests.
    let resting = repainted(platform, &at)?;
    platform.script(KEYBOARD, "resetPointers()")?;
    if idle || resting || !moving {
        return Err(format!(
            "Keyboard repaint tracking: unchanged frame repainted={idle}, resting-pointer frame repainted={resting}, every moving-pointer frame repainted={moving}"
        ));
    }

    // Full controller path: Interaction -> dispatch -> Ultralight, without
    // SteamVR. The text field is a smoke-only fixture, not product UI. Only a
    // simulated laser click focuses it, never a script call.
    platform.script(KEYBOARD, "window.fixture=document.createElement('textarea');fixture.style.cssText='position:absolute;left:20px;top:503px;width:280px;height:26px';document.body.appendChild(fixture)")?;
    let mut sim = Sim::default();
    for view in 0..VIEWS {
        sim.state.panels[view].enabled = true;
    }
    let field = locate(platform, KEYBOARD, "textarea")?;
    sim.click(platform, KEYBOARD, field)?;
    if sim.input.target != Target::Local || platform.sample()?.local_focus != 1 {
        return Err("Laser click did not select the overlay text field".into());
    }
    // Shift, H, I, Shift, 1, Left, Backspace
    for vk in [160, 72, 73, 161, 49, 37, 8] {
        let at = locate(platform, KEYBOARD, &format!("[data-key=\"{vk}\"]"))?;
        sim.click(platform, KEYBOARD, at)?;
    }
    let text = platform.script(KEYBOARD, "fixture.value")?;
    if text != "H!" || sim.input.target != Target::Local {
        return Err(format!(
            "Laser typing expected \"H!\" (Shift,H,I,Shift,1,Left,Backspace), got {text:?}"
        ));
    }
    if sim.input.blocked || sim.input.pending() {
        return Err("Local typing attempted OS injection".into());
    }
    // Holding H types it on press and again after the repeat delay; the
    // release adds nothing.
    let at = locate(platform, KEYBOARD, "[data-key=\"72\"]")?;
    sim.step(platform, KEYBOARD, at, true)?;
    let pressed = platform.script(KEYBOARD, "fixture.value")?;
    let end = sim.now + keyboard::REPEAT_DELAY_MS + 3 * keyboard::REPEAT_INTERVAL_MS;
    while sim.now < end {
        sim.now += 10;
        repeat_keys(platform, &mut sim.input, &mut sim.keyboard, sim.now);
        platform.frame(false, true)?;
    }
    let held = platform.script(KEYBOARD, "fixture.value")?;
    sim.step(platform, KEYBOARD, at, false)?;
    let released = platform.script(KEYBOARD, "fixture.value")?;
    if pressed.matches('h').count() != 1 || held.matches('h').count() < 3 || released != held {
        return Err(format!(
            "Held key expected \"h\" on press, repeats after the delay and nothing more on release, got {pressed:?}, {held:?} then {released:?}"
        ));
    }
    // Clicking a capture panel selects Windows output and releases the text field.
    dispatch(
        vec![Action::Windows],
        platform,
        &mut sim.input,
        &mut sim.keyboard,
        sim.now,
        &f.captures,
    )?;
    platform.frame(false, true)?;
    if platform.sample()?.local_focus != 0 || sim.input.target != Target::Windows {
        return Err("Capture selection did not release the overlay text field".into());
    }
    // Rust-owned Shift lock presentation.
    keyboard.down(0, 160, 100);
    keyboard.up(0, 160, 100);
    keyboard.down(0, 161, 200);
    keyboard.up(0, 161, 200);
    platform.script(KEYBOARD, &keyboard.state_script())?;
    if platform.script(
        KEYBOARD,
        "document.querySelectorAll('.locked').length.toString()",
    )? != "2"
    {
        return Err("Rust locked Shift presentation failed".into());
    }
    platform.script(KEYBOARD, "fixture.remove();resetPointers()")?;
    platform.script(KEYBOARD, &Keyboard::default().state_script())?;

    // Wrist clock: Rust supplies the text; a laser click on the slim button
    // asks for the dashboard.
    let settings = Settings::default();
    platform.script(
        WRIST,
        &format!(
            "setClock(\"{}\",\"{}\")",
            settings.time(f.hour, f.minute),
            settings::duration(3900)
        ),
    )?;
    let button = locate(platform, WRIST, "#summon")?;
    sim.click(platform, WRIST, button)?;
    if Command::parse_all(&platform.script(WRIST, "drainActions()")?) != [Command::Dashboard] {
        return Err("Laser click on the wrist button did not request the dashboard".into());
    }
    // A saved identity must find the same display again (session restore).
    let identity = platform.capture_identity(0)?;
    if identity.is_empty() || platform.source_find(false, &identity)?.is_none() {
        return Err(format!(
            "Display identity {identity:?} does not find its display"
        ));
    }
    if platform
        .source_find(true, "no-such.exe\tNoClass\tNo title")?
        .is_some()
    {
        return Err("A window identity that matches nothing found a window".into());
    }
    // Quick bar: its buttons act on the capture Rust says it hangs under.
    platform.script(
        BAR,
        "setBar({slot:3,opacity:25,curved:true,fade:false,anchor:\"right\"})",
    )?;
    let mut sent = Vec::new();
    for id in [
        "#options",
        "#less",
        "#more",
        "#shape",
        "[data-anchor=\"right\"]",
        "#fade",
        "[data-anchor=\"left\"]",
        "#close",
        "#close",
    ] {
        let at = locate(platform, BAR, id)?;
        sim.click(platform, BAR, at)?;
        sent.extend(Command::parse_all(&platform.script(BAR, "drainActions()")?));
    }
    if sent
        != [
            Command::Overlay(3, OverlaySet::Opacity(20)),
            Command::Overlay(3, OverlaySet::Opacity(35)),
            Command::Overlay(3, OverlaySet::Curved(false)),
            Command::Overlay(3, OverlaySet::Anchor(Anchor::Room)),
            Command::Overlay(3, OverlaySet::Fade(true)),
            Command::Overlay(3, OverlaySet::Anchor(Anchor::Left)),
            Command::Close(3),
        ]
    {
        return Err(format!("Quick bar sent {sent:?}"));
    }
    let button = locate(platform, WRIST, "#keys")?;
    sim.click(platform, WRIST, button)?;
    if Command::parse_all(&platform.script(WRIST, "drainActions()")?) != [Command::Keyboard] {
        return Err("Laser click on the wrist keyboard button did not request the keyboard".into());
    }

    // Dashboard: real source list, a laser click on the first display, the
    // minimized-window notice, settings and SteamVR bindings requests.
    let sources = platform.sources()?;
    platform.script(DASHBOARD, &format!("setSources({sources})"))?;
    let mut shell = Shell::new(settings);
    platform.script(DASHBOARD, &shell.state_script(&f.captures))?;
    let displays = platform.script(
        DASHBOARD,
        "document.querySelectorAll('#displays .source').length.toString()",
    )?;
    let windows = platform.script(
        DASHBOARD,
        "document.querySelectorAll('#windows .source').length.toString()",
    )?;
    if displays.parse::<usize>().map_or(true, |n| n == 0) {
        return Err(format!("No displays listed on the Add screen: {sources}"));
    }
    let click = |sim: &mut Sim, platform: &mut Platform, selector: &str| {
        let at = locate(platform, DASHBOARD, selector)?;
        sim.click(platform, DASHBOARD, at)?;
        Ok::<_, String>(Command::parse_all(
            &platform.script(DASHBOARD, "drainActions()")?,
        ))
    };
    click(&mut sim, platform, "[data-tab=\"add\"]")?;
    match click(&mut sim, platform, "#displays .source")?.as_slice() {
        [
            Command::Open {
                window: false,
                id: _,
            },
        ] => {}
        other => return Err(format!("Display click produced {other:?}")),
    }
    platform.script(DASHBOARD, "setSources({monitors:[],windows:[{id:\"7\",title:\"Minimized fixture\",app:\"fixture.exe\",minimized:true}]})")?;
    if platform.script(
        DASHBOARD,
        "document.querySelectorAll('#windows .source .minimized').length.toString()",
    )? != "1"
        || click(&mut sim, platform, "#windows .source")?
            != [Command::Open {
                window: true,
                id: 7,
            }]
    {
        return Err("Minimized window notice or window click failed".into());
    }
    platform.script(DASHBOARD, &format!("setSources({sources})"))?;
    click(&mut sim, platform, "[data-tab=\"settings\"]")?;
    if click(&mut sim, platform, "[data-set=\"theme:light\"]")?
        != [Command::Set("theme".into(), "light".into())]
        || click(&mut sim, platform, "#bindings")? != [Command::Bindings]
    {
        return Err("Settings screen commands failed".into());
    }
    // A click in the middle of a slider's track sets about the middle value.
    for (key, middle) in [
        ("smoothing", 45..=55),
        ("opacity", 55..=65),
        ("scale", 150..=175),
        ("distance", 215..=235),
    ] {
        let commands = click(
            &mut sim,
            platform,
            &format!("[data-slider=\"{key}\"] .track"),
        )?;
        let value = match commands.as_slice() {
            [Command::Set(k, v)] if k == key => v.parse::<u32>().ok(),
            _ => None,
        };
        if !value.is_some_and(|v| middle.contains(&v))
            || !shell.settings.set(key, &value.unwrap().to_string())
        {
            return Err(format!("Slider {key} sent {commands:?}"));
        }
    }
    click(&mut sim, platform, "[data-tab=\"presets\"]")?;
    shell.presets[1] = Some(("Old".into(), "1 overlay: DISPLAY1".into()));
    platform.script(DASHBOARD, &shell.state_script(&f.captures))?;
    // Name an empty preset: laser click its name field, type on the overlay
    // keyboard, save. Then rename preset 2 the same way, ending with Enter.
    let type_into = |sim: &mut Sim, platform: &mut Platform, field: &str, keys: &[u32]| {
        let at = locate(platform, DASHBOARD, field)?;
        sim.click(platform, DASHBOARD, at)?;
        if sim.input.target != Target::Local {
            return Err(format!("Laser click on {field} did not focus it"));
        }
        for vk in keys {
            let at = locate(platform, KEYBOARD, &format!("[data-key=\"{vk}\"]"))?;
            sim.click(platform, KEYBOARD, at)?;
        }
        Ok::<_, String>(())
    };
    type_into(&mut sim, platform, "[data-name=\"1\"]", &[72, 73])?;
    let saved = click(&mut sim, platform, "[data-preset=\"save:1\"]")?;
    type_into(&mut sim, platform, "[data-name=\"2\"]", &[88, 13])?;
    let renamed = Command::parse_all(&platform.script(DASHBOARD, "drainActions()")?);
    if saved != [Command::Preset(PresetAction::Save("hi".into()), 1)]
        || renamed != [Command::Preset(PresetAction::Rename("Oldx".into()), 2)]
    {
        return Err(format!(
            "Preset naming: save sent {saved:?}, rename sent {renamed:?}"
        ));
    }
    if sim.input.blocked || sim.input.pending() {
        return Err("Typing a preset name attempted OS injection".into());
    }
    if click(&mut sim, platform, "[data-preset=\"load:2\"]")?
        != [Command::Preset(PresetAction::Load, 2)]
        || click(&mut sim, platform, "[data-preset=\"delete:2\"]")?
            != [Command::Preset(PresetAction::Delete, 2)]
    {
        return Err("Presets screen commands failed".into());
    }
    // Inspection images of every screen. The Free SDK paints at most ~60
    // times per second; pace frames so each image shows the current state.
    let image = |platform: &mut Platform, view: usize, name: &str| {
        // Long enough for a tab's ease-in to finish.
        for _ in 0..12 {
            std::thread::sleep(Duration::from_millis(20));
            platform.frame(false, true)?;
        }
        platform.smoke_image(view, name)
    };
    shell.settings.set("theme", "light");
    shell.slots[0] = Some(Slot {
        window: false,
        id: 0,
        placed: true,
        opacity: 100,
        ..Slot::default()
    });
    platform.script(DASHBOARD, &shell.state_script(&f.captures))?;
    click(&mut sim, platform, "[data-tab=\"overlays\"]")?;
    // One overlay's own options, choosing another source for it, and removing
    // it or quitting only on a second click.
    let less = click(&mut sim, platform, "[data-act=\"less\"]")?;
    if less != [Command::Overlay(0, OverlaySet::Opacity(90))]
        || click(&mut sim, platform, "[data-act=\"curved\"]")?
            != [Command::Overlay(0, OverlaySet::Curved(true))]
        || click(&mut sim, platform, "[data-act=\"fade\"]")?
            != [Command::Overlay(0, OverlaySet::Fade(true))]
        || click(&mut sim, platform, "[data-act=\"left\"]")?
            != [Command::Overlay(0, OverlaySet::Anchor(Anchor::Left))]
        || click(&mut sim, platform, "[data-set=\"fade_clock\"]")?
            != [Command::Set("fade_clock".into(), "0".into())]
    {
        return Err(format!(
            "Overlay options: less opaque sent {less:?}, or shape, fade or attachment failed"
        ));
    }
    image(platform, DASHBOARD, "smoke-dashboard-overlays.png")?;
    click(&mut sim, platform, "[data-act=\"source\"]")?;
    platform.script(DASHBOARD, "setSources({monitors:[],windows:[{id:\"7\",title:\"Other fixture\",app:\"fixture.exe\",minimized:false}]})")?;
    image(platform, DASHBOARD, "smoke-dashboard-replace.png")?;
    let replaced = click(&mut sim, platform, "#windows .source")?;
    if replaced
        != [Command::Overlay(
            0,
            OverlaySet::Source {
                window: true,
                id: 7,
            },
        )]
    {
        return Err(format!("Choosing another source sent {replaced:?}"));
    }
    platform.script(DASHBOARD, &format!("setSources({sources})"))?;
    let first = [
        click(&mut sim, platform, "[data-act=\"close\"]")?,
        click(&mut sim, platform, "#quit")?,
    ];
    if first != [vec![], vec![]]
        || click(&mut sim, platform, "[data-act=\"close\"]")? != [Command::Close(0)]
        || click(&mut sim, platform, "#quit")? != [Command::Quit]
    {
        return Err(format!(
            "Remove and Quit must act on the second click only; the first sent {first:?}"
        ));
    }
    if sim.input.blocked || sim.input.pending() {
        return Err("HTML panel clicks attempted OS injection".into());
    }

    image(platform, KEYBOARD, "smoke-keyboard.png")?;
    image(platform, WRIST, "smoke-wrist.png")?;
    image(platform, BAR, "smoke-bar.png")?;
    platform.script(BAR, "collapse()")?;
    image(platform, BAR, "smoke-bar-collapsed.png")?;
    for (view, theme) in [(DASHBOARD, "dark"), (KEYBOARD, "light"), (WRIST, "light")] {
        platform.script(view, &format!("setTheme(\"{theme}\")"))?;
    }
    image(platform, KEYBOARD, "smoke-keyboard-light.png")?;
    image(platform, WRIST, "smoke-wrist-light.png")?;
    for tab in ["add", "settings", "presets"] {
        platform.script(DASHBOARD, &format!("showTab(\"{tab}\")"))?;
        image(platform, DASHBOARD, &format!("smoke-dashboard-{tab}.png"))?;
    }
    platform.capture_close(0)?;
    println!(
        "GPU/UI smoke: {count} ANSI keys fit; dual-pointer hits/presses; cancellation; laser-click focus of an overlay text field; laser typing with Shift/navigation/editing; held-key repeat; Shift lock presentation; wrist button; dashboard display/window/minimized/settings/bindings/close/quit commands; zero OS injection; WGC primary display {}x{}; listed {displays} displays, {windows} windows. Images: build/Release/smoke-*.png",
        f.captures[0].width, f.captures[0].height
    );
    Ok(0)
}

fn ui_hit(panel: usize, (x, y): (f32, f32)) -> Hit {
    Hit {
        panel,
        distance: 1.,
        u: x / VIEW_SIZE[panel].0,
        v: y / VIEW_SIZE[panel].1,
    }
}

/// Centre of the first element matching `selector`, scrolled into view.
fn locate(platform: &mut Platform, view: usize, selector: &str) -> Result<(f32, f32), String> {
    let s = platform.script(view, &format!(
        "(()=>{{const e=document.querySelector({});if(!e)return '';e.scrollIntoView({{block:'nearest'}});const r=e.getBoundingClientRect();return (r.x+r.width/2)+','+(r.y+r.height/2)}})()",
        keyboard::quote(selector)
    ))?;
    let mut parts = s.split(',').map(|p| p.parse::<f32>().ok());
    match (parts.next().flatten(), parts.next().flatten()) {
        (Some(x), Some(y)) => Ok((x, y)),
        _ => Err(format!("Cannot locate {selector}")),
    }
}

/// One simulated left-hand controller driving the real interaction policy.
#[derive(Default)]
struct Sim {
    state: Interaction,
    input: OwnedInput,
    keyboard: Keyboard,
    now: u64,
}
impl Sim {
    fn step(
        &mut self,
        platform: &mut Platform,
        panel: usize,
        at: (f32, f32),
        trigger: bool,
    ) -> Result<(), String> {
        let hit = Some(ui_hit(panel, at));
        let key = keyboard_pointer(platform, 0, hit)?;
        let hand = Hand {
            valid: 1,
            device: 1,
            actions_active: 1,
            trigger: trigger as u32,
            ..Default::default()
        };
        let actions = self.state.update(
            [hand, Hand::default()],
            [hit, None],
            [key, None],
            true,
            0.016,
        );
        // Well outside the Shift double-press window between separate clicks.
        self.now += 400;
        let none = [Capture::default(); CAPTURES];
        dispatch(
            actions,
            platform,
            &mut self.input,
            &mut self.keyboard,
            self.now,
            &none,
        )?;
        platform.frame(false, true)?;
        let local = platform.sample()?.local_focus != 0;
        route(
            platform,
            &mut self.input,
            &mut self.keyboard,
            &mut self.state,
            local,
            &none,
        )
    }
    fn click(
        &mut self,
        platform: &mut Platform,
        panel: usize,
        at: (f32, f32),
    ) -> Result<(), String> {
        self.step(platform, panel, at, false)?;
        self.step(platform, panel, at, true)?;
        self.step(platform, panel, at, false)
    }
}

/// Input deadlines are separate from the GPU/UI refresh deadline.
fn input_interval(active: bool) -> Duration {
    if active {
        Duration::from_secs_f64(1. / 120.)
    } else {
        Duration::from_millis(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn work_is_included_in_input_budget() {
        let budget = input_interval(true);
        assert!(budget < Duration::from_millis(9));
        assert_eq!(
            budget.saturating_sub(Duration::from_millis(10)),
            Duration::ZERO
        );
        assert_eq!(input_interval(false), Duration::from_millis(100));
    }
    #[test]
    fn wrist_pose_is_rigid() {
        assert!(WRIST_POSE.rigid());
    }
}
