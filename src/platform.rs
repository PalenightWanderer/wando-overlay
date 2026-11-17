use crate::geometry::Matrix;
use std::ffi::{CStr, CString, c_char};
// Panel numbering shared with native/platform.h: HTML views, then capture slots.
pub const VIEWS: usize = 4;
pub const CAPTURES: usize = 8;
pub const PANELS: usize = VIEWS + CAPTURES;
pub const KEYBOARD: usize = 0;
pub const DASHBOARD: usize = 1;
pub const WRIST: usize = 2;
/// Quick options bar shown under the capture a laser points at.
pub const BAR: usize = 3;
/// Logical size of each view in CSS pixels (native VIEWS table).
pub const VIEW_SIZE: [(f32, f32); VIEWS] =
    [(1760., 540.), (1200., 760.), (440., 170.), (700., 190.)];
const NO_DEVICE: u32 = u32::MAX;
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Hand {
    /// Pointer ray (`pose/tip`); -Z is forward.
    pub pose: Matrix,
    /// Device tracking pose. Laser overlays are attached relative to this.
    pub raw: Matrix,
    pub valid: u32,
    pub device: u32,
    pub actions_active: u32,
    pub trigger: u32,
    pub grip: u32,
    /// Right-click mode: held while the thumb rests on its button.
    pub right: u32,
    pub scroll: f32,
    /// "Show or hide keyboard" button.
    pub keyboard: u32,
    /// "Hide or show all overlays" button.
    pub toggle: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Capture {
    /// 0 free, 1 capturing, 2 source closed.
    pub open: u32,
    /// A frame has arrived; `width`/`height` are valid.
    pub ready: u32,
    pub width: u32,
    pub height: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Frame {
    pub size: u32,
    pub dashboard: u32,
    pub quit: u32,
    pub recenter: u32,
    pub hmd_valid: u32,
    /// Bit per view.
    pub ui_ready: u32,
    /// 1 + view whose editable element has focus, else 0.
    pub local_focus: u32,
    pub action_error: u32,
    /// Seconds since SteamVR's vrserver started.
    pub session_seconds: u32,
    pub hour: u32,
    pub minute: u32,
    /// Bit per view repainted by the last `frame()` call.
    pub painted: u32,
    pub hmd: Matrix,
    pub hands: [Hand; 2],
    pub captures: [Capture; CAPTURES],
}
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Input {
    pub kind: u32,
    pub code: u32,
    pub down: u32,
    pub extended: u32,
    pub x: i32,
    pub y: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Key {
    pub vk: u32,
    pub modifiers: u32,
    pub character: u32,
}
unsafe extern "C" {
    fn lo_create(vr: u32, manifest: *const c_char) -> i32;
    fn lo_destroy() -> i32;
    fn lo_error() -> *const c_char;
    fn lo_sample(frame: *mut Frame, size: u32) -> i32;
    fn lo_frame(capture: u32, render: u32) -> i32;
    fn lo_panel(panel: u32, device: u32, m: *const Matrix, width: f32, visible: u32) -> i32;
    fn lo_style(panel: u32, curvature: f32, alpha: f32) -> i32;
    fn lo_laser(
        hand: u32,
        device: u32,
        m: *const Matrix,
        endpoint: *const Matrix,
        length: f32,
        visible: u32,
        right: u32,
    ) -> i32;
    fn lo_script(view: u32, source: *const c_char, result: *mut c_char, capacity: u32) -> i32;
    fn lo_local_key(key: *const Key) -> i32;
    fn lo_mouse(view: u32, x: i32, y: i32, kind: u32) -> i32;
    fn lo_inject(input: *const Input) -> i32;
    fn lo_sources(result: *mut c_char, capacity: u32) -> i32;
    fn lo_capture_open(slot: u32, kind: u32, id: u64) -> i32;
    fn lo_capture_close(slot: u32) -> i32;
    fn lo_capture_identity(slot: u32, result: *mut c_char, capacity: u32) -> i32;
    fn lo_source_find(kind: u32, identity: *const c_char, id: *mut u64) -> i32;
    fn lo_capture_placeholder(slot: u32, text: *const c_char, width: u32, height: u32) -> i32;
    fn lo_open_bindings() -> i32;
    fn lo_smoke_image(view: u32, path: *const c_char) -> i32;
}
fn checked(code: i32) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(unsafe { CStr::from_ptr(lo_error()) }
            .to_string_lossy()
            .into_owned())
    }
}
pub struct Platform {
    /// Diagnostic: OS input events are accepted but not injected.
    pub dry_run: bool,
}
pub enum CreateError {
    /// SteamVR is not running. Nothing was launched; retry later.
    RuntimeUnavailable,
    Failed(String),
}
impl Platform {
    pub fn new(vr: bool) -> Result<Self, CreateError> {
        let manifest = std::env::current_dir()
            .map_err(|e| CreateError::Failed(e.to_string()))?
            .join("assets/input/actions.json");
        let path = CString::new(manifest.to_string_lossy().as_bytes()).unwrap();
        match unsafe { lo_create(vr as u32, path.as_ptr()) } {
            0 => Ok(Self { dry_run: false }),
            -2 => Err(CreateError::RuntimeUnavailable),
            code => Err(CreateError::Failed(checked(code).unwrap_err())),
        }
    }
    pub fn sample(&mut self) -> Result<Frame, String> {
        let mut f = Frame::default();
        checked(unsafe { lo_sample(&mut f, std::mem::size_of::<Frame>() as u32) })?;
        Ok(f)
    }
    pub fn frame(&mut self, capture: bool, render: bool) -> Result<(), String> {
        checked(unsafe { lo_frame(capture as u32, render as u32) })
    }
    /// `device` None: `m` is absolute. Otherwise relative to that tracked device.
    pub fn panel(
        &mut self,
        id: usize,
        device: Option<u32>,
        m: Matrix,
        width: f32,
        visible: bool,
    ) -> Result<(), String> {
        checked(unsafe {
            lo_panel(
                id as u32,
                device.unwrap_or(NO_DEVICE),
                &m,
                width,
                visible as u32,
            )
        })
    }
    /// Opacity (0..1) of one panel and, for a capture, its curvature (0 flat .. 1).
    pub fn style(&mut self, panel: usize, curvature: f32, alpha: f32) -> Result<(), String> {
        checked(unsafe { lo_style(panel as u32, curvature, alpha) })
    }
    /// `m` and `endpoint` are relative to tracked device `device`. `right`
    /// colours the laser for right-click mode.
    #[allow(clippy::too_many_arguments)]
    pub fn laser(
        &mut self,
        id: usize,
        device: u32,
        m: Matrix,
        endpoint: Matrix,
        length: f32,
        visible: bool,
        right: bool,
    ) -> Result<(), String> {
        checked(unsafe {
            lo_laser(
                id as u32,
                device,
                &m,
                &endpoint,
                length,
                visible as u32,
                right as u32,
            )
        })
    }
    pub fn script(&mut self, view: usize, source: &str) -> Result<String, String> {
        let s = CString::new(source).map_err(|e| e.to_string())?;
        let mut output = vec![0u8; 32768];
        checked(unsafe {
            lo_script(
                view as u32,
                s.as_ptr(),
                output.as_mut_ptr().cast(),
                output.len() as u32,
            )
        })?;
        Ok(unsafe { CStr::from_ptr(output.as_ptr().cast()) }
            .to_string_lossy()
            .into_owned())
    }
    /// JSON list of capturable displays and windows (native/sources.h).
    pub fn sources(&mut self) -> Result<String, String> {
        let mut output = vec![0u8; 262144];
        checked(unsafe { lo_sources(output.as_mut_ptr().cast(), output.len() as u32) })?;
        Ok(unsafe { CStr::from_ptr(output.as_ptr().cast()) }
            .to_string_lossy()
            .into_owned())
    }
    /// `window` false: `id` is a display handle. True: a window handle.
    pub fn capture_open(&mut self, slot: usize, window: bool, id: u64) -> Result<(), String> {
        checked(unsafe { lo_capture_open(slot as u32, window as u32, id) })
    }
    pub fn capture_close(&mut self, slot: usize) -> Result<(), String> {
        checked(unsafe { lo_capture_close(slot as u32) })
    }
    /// Name of the slot's source that still identifies it after a restart.
    pub fn capture_identity(&mut self, slot: usize) -> Result<String, String> {
        let mut output = vec![0u8; 2048];
        checked(unsafe {
            lo_capture_identity(slot as u32, output.as_mut_ptr().cast(), output.len() as u32)
        })?;
        Ok(unsafe { CStr::from_ptr(output.as_ptr().cast()) }
            .to_string_lossy()
            .into_owned())
    }
    /// Current handle of the display or window with that identity, if present.
    pub fn source_find(&mut self, window: bool, identity: &str) -> Result<Option<u64>, String> {
        let identity = CString::new(identity).map_err(|e| e.to_string())?;
        let mut id = 0u64;
        checked(unsafe { lo_source_find(window as u32, identity.as_ptr(), &mut id) })?;
        Ok((id != 0).then_some(id))
    }
    /// Grey panel with `text` in a slot whose source is missing; `size` gives its aspect.
    pub fn capture_placeholder(
        &mut self,
        slot: usize,
        text: &str,
        size: (u32, u32),
    ) -> Result<(), String> {
        let text = CString::new(text).map_err(|e| e.to_string())?;
        checked(unsafe { lo_capture_placeholder(slot as u32, text.as_ptr(), size.0, size.1) })
    }
    pub fn open_bindings(&mut self) -> Result<(), String> {
        checked(unsafe { lo_open_bindings() })
    }
    pub fn local_key(&mut self, key: Key) -> bool {
        match checked(unsafe { lo_local_key(&key) }) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("{e}");
                false
            }
        }
    }
    /// kind: 0 move, 1 down, 2 up, 3 scroll by `y` pixels.
    pub fn mouse(&mut self, view: usize, x: i32, y: i32, kind: u32) -> Result<(), String> {
        checked(unsafe { lo_mouse(view as u32, x, y, kind) })
    }
    pub fn inject(&mut self, event: Input) -> bool {
        if self.dry_run {
            return true;
        }
        match checked(unsafe { lo_inject(&event) }) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("{e}");
                false
            }
        }
    }
    pub fn smoke_image(&mut self, view: usize, path: &str) -> Result<(), String> {
        let path = CString::new(path).map_err(|e| e.to_string())?;
        checked(unsafe { lo_smoke_image(view as u32, path.as_ptr()) })
    }
}
impl Drop for Platform {
    fn drop(&mut self) {
        if let Err(e) = checked(unsafe { lo_destroy() }) {
            eprintln!("Adapter teardown: {e}");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn abi_sizes() {
        assert_eq!(std::mem::size_of::<Frame>(), 488);
        assert_eq!(std::mem::size_of::<Capture>(), 16);
        assert_eq!(std::mem::offset_of!(Frame, captures), 360);
        assert_eq!(std::mem::size_of::<Hand>(), 132);
        assert_eq!(std::mem::size_of::<Input>(), 24);
        assert_eq!(std::mem::size_of::<Key>(), 12);
        assert_eq!(std::mem::offset_of!(Frame, hands), 96);
        assert_eq!(std::mem::offset_of!(Hand, raw), 48);
        assert_eq!(std::mem::offset_of!(Hand, scroll), 120);
    }
}
