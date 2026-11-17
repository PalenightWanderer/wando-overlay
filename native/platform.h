#pragma once
#include <stdint.h>

// Version 7, Windows x64, C layout. All booleans are uint32_t. No SDK structs
// cross this boundary. Calls return 0 on success, -1 on error (lo_error).
// lo_create returns -2 when SteamVR is not running (nothing was launched).
//
// Panels 0..LO_VIEWS-1 are the HTML views (0 keyboard, 1 dashboard, 2 wrist
// clock, 3 quick bar). Panels LO_VIEWS.. are capture slots 0..LO_CAPTURES-1.
#ifdef __cplusplus
extern "C" {
#endif
#define LO_VIEWS 4
#define LO_CAPTURES 8
#define LO_PANELS (LO_VIEWS + LO_CAPTURES)
#define LO_NO_DEVICE 0xffffffffu
typedef struct { float m[12]; } LoMatrix;
typedef struct {
  LoMatrix pose; // pointer ray (pose/tip); -Z is forward
  LoMatrix raw;  // device tracking pose; device-relative overlays attach to this
  uint32_t valid, device, actions_active, trigger, grip, right; // readiness bits: 1 trigger, 2 grip, 4 right, 8 scroll, 16 keyboard, 32 toggle
  float scroll;
  uint32_t keyboard; // "show or hide keyboard" button
  uint32_t toggle;   // "hide or show all overlays" button
} LoHand;
typedef struct {
  uint32_t open;  // 0 free, 1 capturing, 2 source closed (window closed, display removed)
  uint32_t ready; // a frame has been captured; width/height are valid
  uint32_t width, height;
} LoCapture;
typedef struct {
  uint32_t size, dashboard, quit, recenter, hmd_valid;
  uint32_t ui_ready;        // bit per view
  uint32_t local_focus;     // 1 + view whose editable element has focus, else 0
  uint32_t action_error;
  uint32_t session_seconds; // since vrserver.exe started; 0 without SteamVR
  uint32_t hour, minute;    // local wall clock
  uint32_t painted;         // bit per view repainted by the last lo_frame
  LoMatrix hmd;
  LoHand hands[2];
  LoCapture captures[LO_CAPTURES];
} LoFrame;
typedef struct {
  uint32_t kind, code, down, extended;
  int32_t x, y;
} LoInput; // kind: 0 virtual key, 1 mouse button (code 0 left/1 right), 2 move to pixel x,y of capture slot `code`,
           // 3 wheel (y), 4 UTF-16 unit, 5 bring the window of capture slot `code` to the foreground
typedef struct { uint32_t vk, modifiers, character; } LoKey;
int32_t lo_create(uint32_t vr, const char* manifest);
int32_t lo_destroy(void);
const char* lo_error(void);
int32_t lo_sample(LoFrame*, uint32_t size);
// capture: copy new frames of every open slot. render: repaint and submit changed views.
int32_t lo_frame(uint32_t capture, uint32_t render);
// device LO_NO_DEVICE: matrix is absolute (standing universe). Otherwise it is
// relative to that tracked device. Hiding a view panel also pauses its view.
int32_t lo_panel(uint32_t panel, uint32_t device, const LoMatrix*, float width, uint32_t visible);
// One panel. alpha 0 invisible .. 1. curvature 0 flat .. 1 cylinder, applied to capture panels only.
int32_t lo_style(uint32_t panel, float curvature, float alpha);
// beam/endpoint are relative to tracked device `device` (its raw pose).
// right: draw it in the right-click mode colour.
int32_t lo_laser(uint32_t hand, uint32_t device, const LoMatrix* beam, const LoMatrix* endpoint, float length, uint32_t visible, uint32_t right);
int32_t lo_script(uint32_t view, const char* source, char* result, uint32_t capacity);
int32_t lo_local_key(const LoKey*); // to the view that has editable focus
int32_t lo_mouse(uint32_t view, int32_t x, int32_t y, uint32_t kind); // 0 move, 1 down, 2 up, 3 scroll by y pixels
int32_t lo_inject(const LoInput*); // one event; Rust records each successful down
// JSON list of displays and windows; see native/sources.h.
int32_t lo_sources(char* result, uint32_t capacity);
int32_t lo_capture_open(uint32_t slot, uint32_t kind, uint64_t id); // kind 0 monitor, 1 window
int32_t lo_capture_close(uint32_t slot);
// A name for the slot's source that still identifies it after a restart; see native/sources.h.
int32_t lo_capture_identity(uint32_t slot, char* result, uint32_t capacity);
// Current id of the source with that identity, or 0 when none is present. Windows already captured are skipped.
int32_t lo_source_find(uint32_t kind, const char* identity, uint64_t* id);
// Shows a grey panel with `text` in a free slot whose source is missing. width/height give its aspect.
int32_t lo_capture_placeholder(uint32_t slot, const char* text, uint32_t width, uint32_t height);
int32_t lo_open_bindings(void); // SteamVR controller binding UI for this application
int32_t lo_smoke_image(uint32_t view, const char* path);
#ifdef __cplusplus
}
#endif
