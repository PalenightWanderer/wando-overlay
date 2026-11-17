"""Regenerates the default SteamVR binding files in input/.

Every supported controller gets the same six actions. Only the names of the
thumbstick, the mode button and the hide button differ between controller
types. A long press of the hide button hides or shows all overlays. Resting the
thumb on the mode button (touch) is right-click mode; a long press of the same
button shows or hides the keyboard, so a short press stays free for the
running game (B/Y is often its menu).
Input paths were taken from each driver's input profile under
SteamVR/drivers/*/resources/input (SteamVR build 1794604515).

Run from the repository root:  python scripts/make_bindings.py
"""
import json

# controller_type: (display name, thumbstick, mode button left/right hand, hide button left/right hand)
CONTROLLERS = {
    "pico_controller": ("PICO", "joystick", "y", "b", "x", "a"),
    "oculus_touch": ("Meta Quest Touch", "joystick", "y", "b", "x", "a"),
    "knuckles": ("Valve Index", "thumbstick", "b", "b", "a", "a"),
    # Steam Frame: A/B/X/Y are all on the right controller; the left one has a
    # D-pad whose "down" mirrors A and "right" mirrors B. Untested on hardware.
    "frame_controller": ("Steam Frame", "thumbstick", "dpad_right", "b", "dpad_down", "a"),
}
THRESHOLDS = {"click_activate_threshold": "0.65", "click_deactivate_threshold": "0.55"}


def source(hand, component, action, mode="button", output="click", parameters=None, **more):
    """`more` maps further outputs of the same source to actions."""
    outputs = {output: action, **more}
    entry = {
        "path": f"/user/hand/{hand}/input/{component}",
        "mode": mode,
        "inputs": {o: {"output": f"/actions/overlay/in/{a}"} for o, a in outputs.items()},
    }
    if parameters:
        entry["parameters"] = parameters
    return entry


for controller, (name, stick, left_button, right_button, left_hide, right_hide) in CONTROLLERS.items():
    sources = []
    for hand, button, hide in (("left", left_button, left_hide), ("right", right_button, right_hide)):
        sources += [
            source(hand, "trigger", "trigger", parameters=THRESHOLDS),
            source(hand, "grip", "grip", parameters=THRESHOLDS),
            source(hand, button, "right", output="touch", long="keyboard"),
            source(hand, hide, "toggle", output="long"),
            source(hand, stick, "scroll", mode="joystick", output="position"),
        ]
    binding = {
        "action_manifest_version": 0,
        "name": f"Wander Overlay {name}",
        "description": "Trigger click, grip move, thumb on B/Y for right-click mode, long press for keyboard, thumbstick scroll",
        "controller_type": controller,
        "bindings": {
            "/actions/overlay": {
                "poses": [
                    {"path": f"/user/hand/{hand}/pose/tip", "output": "/actions/overlay/in/pose"}
                    for hand in ("left", "right")
                ],
                "sources": sources,
            }
        },
    }
    with open(f"input/{controller}.json", "w", encoding="utf-8", newline="\n") as f:
        json.dump(binding, f, indent=2)
        f.write("\n")

with open("input/actions.json", encoding="utf-8") as f:
    manifest = json.load(f)
manifest["default_bindings"] = [
    {"controller_type": controller, "binding_url": f"{controller}.json"} for controller in CONTROLLERS
]
with open("input/actions.json", "w", encoding="utf-8", newline="\n") as f:
    json.dump(manifest, f, indent=2)
    f.write("\n")
