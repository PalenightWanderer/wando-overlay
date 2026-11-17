# Wander Overlay: technical notes

Wander Overlay aims to be a fast, minimal, modular Windows SteamVR overlay system for
interacting with desktops and individual windows and typing with a VR keyboard.
The MVP must work in VR space while using applications such as VRChat, including
operating OBS and browser chat through their existing windows.

The current implementation is a **Stage 2 prototype**: a clock on the left wrist
whose button opens an in-space dashboard. From the dashboard you place captures of
displays and windows, show the keyboard and change settings. It does not use the
SteamVR dashboard. The panels were exercised in a live SteamVR session by a
scripted laser and checked by headset screenshot; no person has used them with
real controllers yet. See the checklist at
the end.

The overall plan, product constraints, and agent working rules live in
[AGENTS.md](../AGENTS.md). Codex reads it directly; Claude's entry point is
[.claude/CLAUDE.md](../.claude/CLAUDE.md). Planned capabilities are not claims about
what this prototype currently supports.

## Build and run

Requires Rust, Visual Studio C++ desktop tools (including CMake and a Windows 10
SDK 10.0.19041 or newer, which supplies the C++/WinRT headers), and the
Ultralight 2.0.0-beta.2 Free SDK for Windows x64, downloaded from
<https://ultralig.ht/download> and extracted to the repository root as
`ul-sdk-2.0.0-beta.2-win-x64-free` (git-ignored; not redistributed here).
Capture uses Windows.Graphics.Capture only: Windows 11, or Windows 10 version
2004 or newer.

```powershell
./scripts/build.ps1      # native adapter (CMake), then cargo build
cargo test               # policy unit tests, no GPU/SteamVR/OS input
cargo run -- --smoke     # GPU/UI/capture smoke, no SteamVR, no OS input
cargo run -- --diagnostic  # ~30 s scripted run against SteamVR, no OS input
cargo run -- --diagnostic-input  # scripted typing into a test Notepad window, real OS input
cargo run                # normal use
```

`build.rs` links `build/Release/overlay-platform.dll` and copies the DLLs next to
the executable whenever the adapter DLL changes, so run the build script before the
first `cargo` command and after native changes. UI edits under `ui/`
and input manifests under `input/` are copied on each native build. The worker
runs with `build/Release` as its working directory.

The smoke test drives the real interaction policy and the three HTML views with a
simulated controller. It checks the keyboard (keys fit, dual-pointer hit testing,
press cancellation, repaint reporting while a pointer moves, Shift, typing and a
held key repeating in a smoke-only text field), the wrist button, and the
dashboard (real display/window list, opening a display, the minimized notice,
settings, bindings, close and quit requests). It captures the primary
display through Windows.Graphics.Capture and requires zero OS input injection. It
writes `build/Release/smoke-*.png`, one image per screen; captured desktop frames
stay in GPU memory.

The diagnostic needs no person and no controllers. In the live SteamVR session a
scripted virtual laser (the `DEMO` list in `src/runtime.rs`) presses the clock's
button, adds the first display and the first window, scrolls, changes settings,
shows the keyboard, sweeps across the keyboard's bottom row, grips the dashboard
and carries it 0.3 m to the right, closes a capture, opens the SteamVR bindings,
closes and reopens the dashboard from the clock and presses Quit. For the sweep it hides the
dashboard and moves the keyboard to eye level, because the SteamVR dashboard stays
open in an unattended run and covers the keyboard's usual place. It prints each
step and each command a view sent, how many rendered frames resubmitted the
keyboard during the sweep, how far the grab moved the dashboard, then measured
loop and frame timings. It ignores real
controllers and the SteamVR dashboard, injects no OS input, does not save
settings, and stands the clock beside the dashboard instead of on a wrist. The
first listed window is restored if it was minimized, and SteamVR's binding editor
is left open in the headset afterwards.

`--diagnostic-input` runs a shorter script with real OS input. Open a file named
`wanderoverlay-input-test.txt` in Notepad first; the script captures that window,
clicks into it, types `hi` and presses Ctrl+S, so the file should then contain
`hi`. Do not use the mouse or keyboard while it runs.

`cargo run` supervises a worker process (the same executable with `--worker`).
If SteamVR is not running, the worker exits without launching it and the
supervisor waits, retrying every two seconds. SteamVR quitting returns to that
wait. Three consecutive unexpected worker exits, each within a minute of its
start, stop the supervisor. Do not launch a second instance while one is running.

### A copy to use every day

```powershell
powershell -File scripts/package.ps1
```

builds the release executable and puts everything it needs in
`dist\Wander Overlay`. Start `Wander Overlay.exe` there (the folder can be moved
anywhere). It waits for SteamVR, reconnects when SteamVR restarts and keeps a
console window with its log. Settings, the session and presets stay in
`%APPDATA%\Wander Overlay`, shared with development builds. It is not registered
with SteamVR, so it does not start by itself.

## Using it in VR

1. Start SteamVR, put on the headset, close the SteamVR dashboard, then `cargo run`.
2. A small clock sits behind your left controller, on the back of the wrist. It
   shows the system time and how long SteamVR has been running.
3. Point the right controller at the slim button on the clock's right edge and
   pull the trigger. The dashboard appears in front of you. The same button, or
   **Close dashboard**, hides it again.
4. Point either controller at a panel to show that controller's laser. Both
   lasers work at once.

### Dashboard

The header shows the time and how long SteamVR has run, and **✕** closes the
dashboard. **Quit** asks for a second click.

| Screen | What it does |
| --- | --- |
| Overlays | Each open capture has its options in one strip: opacity **−** / **+** (steps of 10%), **Curved**, **Fade**, attach to **Head**, **L hand** or **R hand** (the lit one puts it back in the room), and **Swap…**, which opens the Add screen to pick another display or window for the same panel. **✕** removes it on a second click within three seconds. Below: show or hide the keyboard, and **Fade** for the keyboard, wrist clock and dashboard. |
| Add | Lists displays and running windows, reread every three seconds while shown; pull the trigger on one to place it ahead of you at the default distance and size. Up to 8 captures. |
| Presets | Four named slots. Click a name field and type with the overlay keyboard (it opens by itself); **Save current** stores what is open under that name, Enter or clicking elsewhere renames a saved preset. **Load** replaces what is open with the preset; **Overwrite** and **Delete** as named. |
| Settings | Theme (dark, light), 12/24-hour clock, pointer steadying (off to 100%), the shape, opacity, size (25-300%) and distance (0.5-4.0 m) that newly added captures start with, keyboard layout (only the full US layout exists), and **Open SteamVR bindings**. Sliders: drag, or click a spot on the track. |

A minimized window is marked with an orange notice. Adding it restores the window
on your desktop, because a minimized window produces no picture. Settings are saved
at once to `%APPDATA%\Wander Overlay\settings.txt`.

**Quick bar.** Point a laser at a capture and two small buttons appear under
it: a gear and an X. The X removes the capture on a second click. The gear
opens a panel below them, so nothing moves under the laser: **−** and **+**
change opacity in steps of 10%, **Curved** switches its shape, **Fade** makes
it fade when turned away, and three slim buttons attach it to your head, left
hand or right hand (the lit one puts it back in the room).

**Attaching.** A capture attached to the head keeps its place in your view as
you turn. Attaching one to a hand shrinks it to at most 0.35 m wide and puts it
just above that controller, facing you; grab it with the other hand to move or
size it, and it stays attached. Taken off the controller, it gets back the size
it had before (not across a restart). A hand cannot point at the capture it carries.
While the headset or that controller is not tracked, the capture is hidden.
**Room** leaves it where it is at that moment.

**Fade.** A panel with Fade on dims as its front turns away from you and is
gone when you would see it nearly edge-on or from behind (fully shown within
about 63 degrees, gone beyond about 81). It then takes no laser hits. It is set
per capture, and for the keyboard, wrist clock (on by default) and dashboard on
the Overlays screen.

**Pointer steadying.** Each laser is smoothed more the slower the hand moves, so
a tremor or the twitch of pulling the trigger moves it less, while a quick sweep
is followed almost directly. On the keyboard a laser also keeps the key it is on
until it is clearly on the next one, unless the laser is moving fast toward
another key; the gaps between keys count as the nearest key.

**Menus** ease in and out over about 150 ms. Captures do not.

**Sessions and presets.** What is open is written to `session.txt` in the same
folder about a second after it changes, and reopened on the next start: each
display and window, where its panel is and how big, its opacity, shape and
attachment, and the keyboard. A preset
(`preset1.txt` to `preset4.txt`) stores the same things on request. The file
format is described at the top of `src/layout.rs`.

- A display is found again by its hardware interface path, not by its
  `\\.\DISPLAYn` name, which Windows renumbers when displays come and go
  (virtual display drivers do this often). If the path changed too, a display of
  the same model that is not captured yet is used.
- A window is found again by application, window class and title. If the title
  changed, the first window of the same application and class is used.
- A display that is missing, at start or because it went away while open, keeps
  its panel as a grey placeholder reading "Display lost". A saved window that is
  not open shows "Window not open". The source is looked for every two seconds
  and the panel comes back by itself. A placeholder can be moved and sized but
  takes no clicks; close it from the Overlays screen. A window you close while
  it is captured is removed instead.
- Positions are room positions. After a SteamVR play-area change or recenter the
  open panels are brought back in front of you instead.
- Restoring a minimized window restores it on the desktop.

**Open SteamVR bindings** opens SteamVR's own binding editor, which appears in the
SteamVR dashboard. Wander Overlay's panels hide while that dashboard is open.

### Controls

| Control | Action |
| --- | --- |
| Trigger on a capture | Left click or drag. Clicking a window capture first brings that window to the front. |
| Thumb resting on B/Y, then trigger on a capture | Right click or right drag. The laser turns from blue to orange while the thumb is on the button. |
| Thumbstick up/down | Scroll the capture, or the dashboard list, under the laser |
| Trigger on keyboard | Type the key under the laser on press; holding it repeats. Shift, Ctrl, Alt and Win act on release instead |
| Trigger on dashboard or wrist button | Ordinary click |
| Grip on a panel | Grab it; it follows the hand with modest smoothing until released. The wrist clock cannot be grabbed. |
| Thumbstick up/down while gripping | Push the grabbed panel away or pull it closer along the laser, 1.2 m/s, between 0.15 m and 6 m from the hand |
| Trigger + thumbstick up/down while gripping | Make the grabbed panel bigger or smaller, between 0.15 m and 5 m wide. The trigger does not click while you grip. |
| Hold a modifier key with one laser, type with the other | The modifier stays down in Windows until you let go: Alt + number pad codes, Win shortcuts, Ctrl or Alt chords |
| Hold the hide button | Hide or show everything except the wrist clock |
| Hold the keyboard button, or trigger on the clock's keyboard button | Show or hide the keyboard. It appears below eye level, tilted to face you. |

Default bindings ship for these controller types (`input/*.json`, generated by
`scripts/make_bindings.py`):

| Controller | SteamVR type | Mode button: touch for right-click mode, hold for the keyboard | Hide button (hold) | Tested |
| --- | --- | --- | --- | --- |
| PICO (PICO Connect) | `pico_controller` | Y left, B right | X left, A right | All six actions report bound; feel not confirmed |
| Meta Quest Touch, and PICO when presented as Quest | `oculus_touch` | Y left, B right | X left, A right | No |
| Valve Index | `knuckles` | B on both | A on both | No |
| Steam Frame | `frame_controller` | D-pad right left, B right | D-pad down left, A right | No hardware available |

The worker prints each controller's SteamVR type when it is first tracked, which
tells you which file SteamVR picked. Its readiness line lists the bound actions per
hand; 31 means all five. SteamVR reads the action list (`input/actions.json`) once
per SteamVR session: after adding or renaming an action, restart SteamVR, or the
new action stays unbound even though the binding file loads. Other controllers need bindings made in
SteamVR's binding editor.

Only one OS cursor exists. The first hand to press on a capture owns the drag
until release, and the other hand cannot move or release it. Both hands can type
at the same time. One hand at a time can press dashboard or wrist buttons.

Capture presses hold the initial cursor position until the ray moves more than
12 mm across the panel; crossing that threshold starts a drag until release.
Hover uses a 12 ms time constant and grabbing uses 20 ms. These initial tuning
values need headset evaluation.

The active input loop targets 120 Hz with work included in its interval; capture
and UI refresh run at most 60 Hz. Hidden panels use a 100 ms loop interval. In a
diagnostic run on the development PC (a 1920x1080 display capture, a window
capture, dashboard and keyboard shown) the capture copy, UI render and texture submission averaged
0.3 ms per refresh. That is one measurement, not a budget.

**Typing destination.** Keys go to the Windows foreground application. Click
into the app or text field on a capture first. If an editable field inside the
overlay UI has focus, which only a laser click can give it, keys go there
instead and never reach Windows. Clicking a capture returns to Windows. The
keyboard header shows `Windows foreground` or `Overlay text`. The overlay UI has
no product text field yet.

**Modifiers.**

- **Shift:** one tap shifts the next key, including digits, punctuation and
  Shift+arrow selection. A second tap within 500 ms, with no key between, locks
  Shift until the next tap. A second tap after that window turns Shift off.
- **Caps Lock** affects letters only and is the keyboard's own state, not the
  Windows Caps Lock.
- **Ctrl and Alt:** a tap latches them for the next key. Alternatively, hold one
  with a laser and press a key with the other.
- **Win:** a tap alone sends Win, which opens Start. For Win+key, hold Win with
  one laser and press the key with the other.
- **Menu** sends the context-menu key.
- **Output:** printable text without Ctrl/Alt/Win is sent as Unicode, so the
  Windows keyboard layout cannot change the glyph. Space, editing/navigation keys
  and shortcuts are sent as virtual keys. Modifiers are pressed only for the
  duration of each key; none stay held while idle.

**Typing and key repeat.** A key types the moment the trigger is pulled, like a
hardware keyboard. Holding it types the same keystroke again after 0.5 s and then
about 30 times a second; releasing types nothing more, and leaving the keyboard
panel ends the hold. Each repeat is the first keystroke again, so a latched Shift
or a held Ctrl applies to every repeat. Shift, Ctrl, Alt and Win act on release,
because only then is a tap told apart from a hold. Caps Lock toggles on press and
does not repeat. A mistaken pull types its key, and sliding off the key or the
panel cannot take it back. Both timings follow the Windows defaults and need
headset evaluation.

**Leaving.** Opening the SteamVR dashboard hides the panels and lasers and releases
held input. Every press after it closes needs a fresh trigger pull. The same
applies to tracking loss per hand, a capture changing size or closing, and context
switches. Use **Quit** on the dashboard for an orderly exit. Ctrl+C or
forced termination does not guarantee input cleanup.

## Boundaries

- `src/main.rs`: supervisor and worker entry.
- `src/runtime.rs`: the loop. It samples, places panels, routes the keyboard
  target, draws lasers, dispatches actions and applies dashboard commands. It
  also contains the smoke test.
- `src/shell.rs`: dashboard policy. The `Command` list is everything an HTML view
  may ask for; anything else a page sends is dropped.
- `src/settings.rs`: settings, their allowed values and the settings file.
- `src/layout.rs`: the saved layout (session and presets) and its file format.
- `src/interaction.rs`: per-hand pointer state, press/drag/grab ownership, button
  edge gating and cancellation for capture panels and HTML panels.
- `src/geometry.rs`: ray hits on flat and curved panels, placement and laser geometry.
- `src/keyboard.rs`: ANSI layout, Shift/Caps/modifier state, key repeat and labels.
- `src/input.rs`: Windows versus overlay routing and owned-input tracking.
  Partial `SendInput` failures release only what this process pressed.
- `src/platform.rs` and `native/platform.h`: the versioned C ABI, including the
  panel numbering (views 0-2, then capture slots).
- `native/`: resources only. `capture.h` holds Windows.Graphics.Capture sessions
  on their own D3D11 device, `sources.h` lists displays and windows, `ui.h` and
  `ui_driver.h` handle Ultralight, `vr.h` handles OpenVR overlays/actions, and
  `main.cpp` holds the exported calls and `SendInput`.
- `ui/`: `keyboard.html`, `dashboard.html`, `wrist.html` and the shared
  `theme.css`. Pages are presentation only. They receive state from Rust, never
  call OS APIs, and queue plain action strings that Rust validates.
- `input/`: the SteamVR action manifest and default bindings.
- `vendor/openvr/REVISION`: exact upstream commit for Valve headers and binaries.
- Ultralight's original SDK and platform sources are not modified.

The normal rendering path does not read GPU textures back to CPU. Only the explicit
smoke test reads the view textures back to write its inspection PNGs.

## Current limits / headset acceptance checklist

- The wrist clock's position and angle on the controller are a first guess
  (`WRIST_POSE` in `src/runtime.rs`) and are always shown while the left
  controller is tracked.
- At 100% a capture is 1 m wide per 960 source pixels, between 0.3 m and 2.4 m.
  Presets do not store settings such as theme. The dashboard's own position is
  not saved; it opens in front of you.
- Not confirmed with real controllers in the headset: pointer steadying and
  sticky keys (tuning values in `src/interaction.rs` and `ui/keyboard.html` are
  first guesses), attaching a capture to a hand, the fade angles (`TURN_FULL`,
  `TURN_GONE` in `src/runtime.rs`), Alt codes, and the hide button.
- Only captures can be attached, not the keyboard or dashboard.
- The quick bar hangs under the capture, where another panel can cover it; it is
  not moved to a free side. Its size and linger time are first guesses
  (`BAR_LINGER` in `src/runtime.rs`). Not tried with real controllers.
- Windows 10 draws a yellow border around captured content; it can only be
  turned off on Windows 11. There is no HDR handling. Capture and the UI use the
  default GPU; headset/compositor adapter selection is pending.
- Bringing a captured window to the front can be refused by Windows. The click
  then lands on whatever covers that spot on the desktop. A window on another
  virtual desktop is not listed.
- Scene input overlap is unverified. The running game probably also receives
  trigger, grip, button and thumbstick input while you use the panels.
- There is no IME, numpad or Print Screen/Scroll Lock/Pause key.
  Modifier chords apply to keys only, not to mouse clicks. Alt+Tab switches only
  between two windows because Alt is released after each key.
- The UI views paint at most about 60 times per second, the Free SDK cap.
- Unrelated Windows foreground changes can redirect typing.
- The supervisor handles exits but does not yet detect a hung worker. Abrupt
  termination while dragging can leave an injected mouse button down.
- Elevated applications such as an elevated OBS may reject input from this
  normally launched process. A rejected injection disables further injection for
  that worker session. Restart after resolving the restriction.

Before treating this as daily-use software, test the following in the headset:
the wrist clock's placement and readability, opening and closing the dashboard
from the wrist button, adding a display and a window (including a minimized one),
closing them, every setting, the SteamVR bindings button, scrolling the window
list, both lasers, clicking each corner of a capture, typing into Notepad and a
browser, holding keys to repeat, two-hand typing, Shift once/double/unlock, the
Win key and Win+key, grip move/rotate/release, and opening the SteamVR dashboard
during a held input. Also check scene input overlap, tracking-loss cleanup,
repeated SteamVR restarts, and long-session memory/CPU/GPU behavior. A passing
smoke test or diagnostic does not validate any of these.

Task specs and validation notes are kept locally under `.adoc/` (git-ignored).
Stage 2 is described in `.adoc/specs/stage2-dashboard.md` and
`.adoc/validation/stage2-dashboard.md`.

## Licensing

Ultralight is a separately supplied proprietary dependency; keep its license and
notices with distributions and check the applicable edition terms. Its bundled
platform reference sources use the Zlib license. Valve's OpenVR license is included
under `vendor/openvr/LICENSE`.
