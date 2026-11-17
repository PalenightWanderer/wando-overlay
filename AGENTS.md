# Agent instructions for working with Wander Overlay

## Working with the user

- When the user is just asking questions, answer without making changes. A question,
  hypothetical, or request for an assessment is not authorization to implement it.
- Check with a human first, do not independently add features, settings, dependencies, or speculative frameworks.
- Distinguish implemented behavior, measured results, proposals, and unverified claims.
  A build or GPU smoke test does not establish in-headset usability or recovery.
- All code and user-facing text (ui, documentation) must be written so that any human person is able to open the code project and continue working, completely independent of any agent being involved.

## Agent workflow

- Write clean, simple code:
  - Only add abstractions when they provide clear value. Don't future-proof prematurely
  - Only handle cases that are relevant. Don't over-engineer for hypothetical edge-cases
- Before adding common logic, search the codebase (especially the utils folder) for existing implementations that can be reused. If one exists but isn't exported, extract it into a util.
- Don't add comments unless they provide valuable context or explanation that isn't immediately clear. Use single line style comments.

- For substantial implementation or architectural changes, have a strong reasoning model (of Mythos class when available) examine the code and write a task spec in `.adoc/specs/<task>.md` before handing execution to a separate model. Prefer a capable workhorse executor, such as an appropriate Opus or Codex model. These are roles and not permanent model assignments; use the available catalog and respect mentioned choices and usage limits.
- Each spec must include the goal, relevant scope, constraints, relevant files, what not to touch/modify, implementation steps, and concrete acceptance checks. Record verified API names, SDK versions, quirks, and source references explicitly;
  distinguish facts from your assumptions and unresolved questions. Do not rely on the executor's memory for SteamVR/OpenVR, graphics, niche knowledge or native integration details.
- Specs must stand alone for a human developer without access to agents or that prefers not to use them.
  Keep this file authoritative for the overall plan; task specs describe bounded
  changes and must not silently expand that plan or the user's authorization.
- The executor reads the spec and relevant code, implements within that scope,
  and flags material ambiguity or conflicting evidence to the planner instead of
  guessing. Ask the user when resolving it would change requirements or scope.
  Report changed files, validation results, and remaining limitations at handoff.
- Use subagents / worktrees when they are necessary or more efficient. Assign bounded tasks with clear outputs; use lower-cost capable Claude/Codex models for mechanical searches, documentation lookup, log summaries, specified boilerplate, and test runs. Keep dependent work sequential and avoid overlapping file ownership or having several agents repeat the same investigation.
- The coordinating agent remains responsible for checking delegated results and
  integrating them. Use a short independent reasoning-model review of the final
  diff when design risk warrants it; do not require an expensive review loop for
  every edit. Evidence from builds and tests remains distinct from headset testing.
- Keep small, clear fixes and documentation edits direct when a separate spec or
  delegation would cost more than it saves. Questions remain informational unless
  the user authorizes changes; delegation does not extend that authorization.

## Product goal

Build a fast, minimal, modular Windows SteamVR overlay system that is useful in VR
space. The current dashboard overlay is a feasibility prototype, not the product
boundary. The MVP must let the user place and use desktop/window overlays and a
VR keyboard with the SteamVR dashboard closed while another VR application runs.
Keep a minimal dashboard for configuration. The in-world overlay must not conflict with the system dashboard. 

The primary workflow is interacting with desktops and individual windows, typing
into them, and managing livestreams and browser chat windows either through a dedicated window capture session or the full desktop monitor capture.
Desktop/window capture and the keyboard are the MVP. The left-wrist clock is the entry point to the in-space dashboard. A battery widget and similar extras are add-ons and can be added later.
Dedicated OBS/chat integrations are unnecessary unless ordinary window interaction
proves insufficient and the user requests them.

The software is modular. Beyond the required functional foundation, most
functionality belongs in either a Core addon or a community/user addon. Core
addons are maintained and shipped with the application and are always present;
some may become toggleable later. Community/user addons are separately supplied
extensions. Bundled presence does not require every addon to be active or visible.
Desktop/window capture and the keyboard must have clear module boundaries even
though they are MVP requirements. Keep the required lifecycle, configuration,
rendering, and input foundation straightforward. This direction does not authorize
new features or require a general public addon framework before the MVP works.

## Product principles
These are not necessarily agent guidelines, but they are an initial design philosphy for how the overlay software should be developed and treated

- Do not bombard the user with features they do not need. Keep the initial UI small.
- Optional modules must stop their rendering, timers, subscriptions, and background
  tasks when disabled. Hiding a view alone is not disabling it.
- Updates must not silently enable new optional features.
- Treat modularity as a way to contain scope, not a reason to expand it. Keep required
  lifecycle, configuration, rendering, and input infrastructure straightforward.
- Track hidden CPU/GPU activity, memory, wakeups, input latency, frame pacing,
  compositor (and possibly VRChat) impact, startup, and recovery. Establish budgets from measurements.
- Crash resilliance/recovery is part of the MVP, but don't obsess over it. SteamVR as well as graphics drivers are both often known to crash or timeout in the case of the gpu driver. Recover our overlay when SteamVR becomes
  available again; do not promise to recover SteamVR, VRChat, or wireless streaming.

## Current implementation and architectural direction

Read docs/technical.md for build commands and concrete prototype limitations.

- Rust is the main language. It owns the application loop, supervisor, controller
  and pointer policy, placement, keyboard layout/modifiers, output routing and input
  cleanup (`src/`). A small C ABI adapter (`native/`, built as `overlay-platform.dll`)
  wraps the SDK integrations Rust calls into: Windows.Graphics.Capture (the only
  capture path; current Windows 10 and Windows 11), Ultralight, OpenVR and
  Win32 `SendInput`. It holds resources only, with no loop or interaction state.
  `ui/` holds one HTML page per panel: keyboard, dashboard and wrist clock. Moving more native code to
  Rust is fine when it pays off; it is not a goal in itself.
- Keep desktop/window capture on its own GPU texture path into SteamVR, independent
  of HTML repainting. Avoid routine GPU-to-CPU readback; explicit diagnostic image
  capture is a separate case.
- We'll use Ultralight for keyboard, dashboard, and controls. Keep its integration contained
  and SDK version pinned. The bundled 2.0.0-beta Free SDK caps UI views at 60 FPS;
  assess keyboard feedback in-headset rather than assuming this meets the target. It should be sufficient, but keep an eye on it when working on it.
- Preserve the HTML/CSS design workflow. If measured problems justify reevaluation,
  CEF is the full-web fallback; RmlUi is an option for lower-overhead, web-like UI
  that would require porting. Do not build alternate renderers or a universal
  renderer framework in advance.
- Keep keyboard layouts, modifiers, repeat, target-window handling, and held-input
  cleanup outside renderer-specific presentation. Pages send validated actions
  through a narrow bridge rather than calling OS APIs directly.
- Use OpenVR overlay APIs; a SteamVR driver is outside the MVP.
- Keep SDK/vendor code separate from application changes and preserve license notices.
  Verify applicable redistribution terms before packaging a release.

## MVP delivery plan
These are target capabilities

1. **Prove the in-space workflow.** Take the dashboard experiment to a captured
   desktop/window and keyboard usable with the dashboard closed. Verify controller
   interaction, texture lifetime, transparency, placement, and UI/capture pacing on the actual headset. Establish a performance baseline.
2. **Make capture and placement useful.** Support monitor and individual-window
   selection, repositioning, resizing, and saved layouts. Handle source closure,
   resizing, window movement, DPI scaling, and capture/device loss deliberately.
3. **Complete interaction and typing.** Point, click, right-click, drag, and scroll;
   type with modifiers, editing/navigation keys, repeat, and common shortcuts.
   Preserve the intended target's focus without keyboard UI stealing it. Release
   held inputs on hide, disconnect, and recovery. Test input restrictions with
   elevated applications such as OBS; do not claim universal compatibility.
4. **Make recovery dependable.** Recreate sessions, overlays, and GPU/capture
   resources after SteamVR loss; restore saved preferences and layout. Detect
   crashed and hung workers, bound retries, and provide a minimal recovery mode
   after repeated failures. Intentional quit must stay quit. Persist configuration
   atomically rather than relying on shutdown callbacks. If a display is unavailable or becomes unavailable, simply darken the texture used to display it with a simple few words until it possibly returns. There is no guarantee it will.
5. **Establish modularity and localization.** Keep desktop and keyboard features
   behind small lifecycle boundaries, with optional features doing no work when
   disabled. Externalize strings, provide fallback/pluralization, and exercise
   layouts with pseudo-localization. A public third-party addon SDK comes later;
   prove isolation before accepting arbitrary user code. A separate process alone
   is not a security sandbox.
6. **Validate and package.** Test repeated SteamVR restarts, worker crashes/hangs,
   source loss, sleep/resume, invalid settings, and long VR sessions. Compare
   performance under equivalent workloads and produce a build that runs outside
   the development environment. Record remaining limitations honestly.

The MVP is not complete until capture, placement, click/drag/scroll, typing, saved
layout restoration, and recovery work together in-space on the user's hardware.
The existing experiment alone cannot satisfy this acceptance condition.

## Iteration and deferred scope

Keep the edit/build loop short: isolate native integration, keep dependencies
modest, and aim for UI/translation iteration without recompiling the backend.
Measure clean and incremental Rust and native build times separately before adding
caches or changing toolchains.

Defer non-core plugin directory, driver features, unrelated widgets,
dedicated streaming integrations, and a general public addon SDK. User-created
addons remain a longer-term goal, not a prerequisite for the first useful MVP.