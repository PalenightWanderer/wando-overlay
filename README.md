# Wando Overlay

**Wando Overlay** (Wander's Overlay) is a lightweight, open source SteamVR overlay for Windows 11/10. It lets you see and use your desktop and individual windows from inside your headset!

> **This is an early prototype.** It works, but it has mostly been tested with one headset on my one PC, so expect some rough edges.

This is for anyone who is in VR and still needs to use their PC: like using a browser, managing OBS while streaming, checking Discord, or anything else.
If you play performance-heavy games (namely VRChat... The other ones aren't real) and want an overlay that stays out of the way, try this out.

## Why yet another overlay?

Most popular VR overlays run inside a game engine. Wando doesn't. It is 
a native program that hands captured windows straight to SteamVR on the 
GPU, and only draws its own menus when something changes. The goal is to leave 
as much performance as possible to the game you're actually playing.

It also tries to stay small: a few things you need, without things you don't need. No drivers, no re-inventing the wheel, that's up to you to figure out. :P
AKA What devs like to call Scope/Feature creep.

## What it does

- **Desktop and window capture.** Place any display or individual window in the
  room, attach it to your head or a hand, resize it, make it curved or
  see-through.
- **Point and click.** Use your controllers as lasers to click, drag, right-click
  and scroll in captured windows. Both hands work at once.
- **VR keyboard.** Type into whatever window you clicked, with Shift, Ctrl, Alt,
  Win and key repeat.
- **Wrist clock and dashboard.** A small clock on your left wrist opens a
  dashboard for adding captures, saving layouts and changing settings.
- **Remembers your setup.** What you had open comes back next time, and you can
  save up to four layouts as presets.
- **Recovers from SteamVR restarts.** It waits for SteamVR and reconnects on its
  own.

## Requirements

- Windows 11, or Windows 10 version 2004 or newer (Windows 10 is untested. But you IOT LTSC freaks probably have a solution already)
- SteamVR and a supported headset
- Controller bindings are included for PICO (tested on PICO 4), Meta Quest Touch, Valve Index and
  Steam Frame controllers. Others can be bound in SteamVR's binding editor.

## Getting started

There are no prebuilt releases yet, so for now you build it yourself.

## That part of the README everyone complains about
The project is primarily developed via steering agentic AI models in the right direction. I plan to create a similar program by hand using C++ rather than rust in the future, so you may look out for that if this turns you off so hard.

### What you need to build it

- [Rust](https://rustup.rs) 1.85 or newer, installed with rustup
- [Visual Studio 2022](https://visualstudio.microsoft.com/downloads/) (the free
  Community edition or Build Tools is fine) with the **Desktop development with
  C++** workload. Make sure these are ticked:
  - **C++ CMake tools for Windows**
  - A **Windows 10 or 11 SDK**, version 10.0.19041 or newer
- The [Ultralight SDK](https://ultralig.ht/download), version
  **2.0.0-beta.2, Free, Windows x64**. Extract it into the repository folder so
  that you end up with a folder named `ul-sdk-2.0.0-beta.2-win-x64-free` next to
  this README. It isn't included here because its license doesn't allow us to
  redistribute the SDK itself.

### Building

From PowerShell in the repository folder:

```powershell
powershell -File scripts/package.ps1
```

This builds everything and puts a ready-to-use copy in `dist\Wander Overlay`.
Start `Wander Overlay.exe` from there; you can move that folder anywhere. Your
settings are kept in `%APPDATA%\Wander Overlay`.

Developer commands, tests and diagnostics are in
[docs/technical.md](docs/technical.md#build-and-run).

### First steps in VR

1. Start SteamVR and put on your headset.
2. Look at the back of your left wrist for the clock "wristwatch".
3. Point your right controller at the triple-lined "hamburger" button on the clock's edge and pull the
   trigger to open the dashboard.
4. Go to **Add** and pick a display or window.

Hold the grip on any panel to move it, and use the thumbstick while gripping to
push it away or pull it closer.

The UI is rough right now, it will be worked on.

## Documentation

- [docs/technical.md](docs/technical.md): building, controls in detail, how
  things work internally, and known limitations.
- [AGENTS.md](AGENTS.md): project goals and design principles.

## Third-party software

Wander Overlay's menus are drawn with [Ultralight](https://ultralig.ht), which
is proprietary and used under its Free license. The SDK is not included in this
repository; download it yourself to build. Builds of Wander Overlay include
Ultralight's DLLs, which are subject to the Ultralight EULA; it and Ultralight's
third-party notices are in the `licenses` folder of every build.

SteamVR integration uses Valve's
[OpenVR](https://github.com/ValveSoftware/openvr) (see `vendor/openvr/LICENSE`).

## License

Wander Overlay is released under the [MIT License](LICENSE). Ultralight and
OpenVR keep their own licenses.
