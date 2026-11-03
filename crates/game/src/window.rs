//! Locates the Path of Exile window (read-only Win32 queries, no input is ever sent).
use std::ffi::c_void;
use std::path::PathBuf;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GW_OWNER, GWL_EXSTYLE, GetClientRect, GetForegroundWindow, GetWindow, GetWindowLongW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, PWSTR, w};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Debug)]
pub struct PoeWindow {
    pub hwnd: isize,
    pub pid: u32,
    pub exe: PathBuf,
    /// Client area in screen coordinates, physical pixels.
    pub client: Rect,
    pub foreground: bool,
    pub minimized: bool,
}

pub fn enable_dpi_awareness() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

fn to_hwnd(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

fn exe_of(pid: u32) -> Option<PathBuf> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }
}

fn is_poe_exe(exe: &std::path::Path) -> bool {
    exe.file_name()
        .map(|n| n.to_string_lossy().to_lowercase().starts_with("pathofexile"))
        .unwrap_or(false)
}

fn client_rect(hwnd: HWND) -> Option<Rect> {
    unsafe {
        let mut r = RECT::default();
        GetClientRect(hwnd, &mut r).ok()?;
        let mut p = POINT { x: 0, y: 0 };
        if !ClientToScreen(hwnd, &mut p).as_bool() {
            return None;
        }
        Some(Rect { x: p.x, y: p.y, w: r.right - r.left, h: r.bottom - r.top })
    }
}

fn build(hwnd: HWND, pid: u32, exe: PathBuf) -> Option<PoeWindow> {
    unsafe {
        let minimized = IsIconic(hwnd).as_bool();
        let client = client_rect(hwnd).unwrap_or(Rect { x: 0, y: 0, w: 0, h: 0 });
        Some(PoeWindow {
            hwnd: hwnd.0 as isize,
            pid,
            exe,
            client,
            foreground: GetForegroundWindow() == hwnd,
            minimized,
        })
    }
}

fn info_for(hwnd: HWND) -> Option<PoeWindow> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    let exe = exe_of(pid)?;
    build(hwnd, pid, exe)
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let out = unsafe { &mut *(lparam.0 as *mut Vec<PoeWindow>) };
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        // Skip tool windows and owned windows (overlays, dialogs).
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TOOLWINDOW.0 != 0 {
            return BOOL(1);
        }
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.0.is_null()) {
            return BOOL(1);
        }
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid != 0
        && let Some(exe) = exe_of(pid)
        && is_poe_exe(&exe)
        && let Some(w) = build(hwnd, pid, exe)
    {
        out.push(w);
    }
    BOOL(1)
}

/// Finds the game window by class name, falling back to an exe-name search
/// (largest client area among matching top-level windows).
pub fn find_poe_window() -> Option<PoeWindow> {
    unsafe {
        if let Ok(h) = FindWindowW(w!("POEWindowClass"), None)
            && let Some(win) = info_for(h)
        {
            return Some(win);
        }
        let mut found: Vec<PoeWindow> = Vec::new();
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut found as *mut _ as isize));
        found.into_iter().max_by_key(|w| w.client.w as i64 * w.client.h as i64)
    }
}

/// Cheap per-call refresh of a cached window. `None` if the window is gone.
pub fn refresh_window(prev: &PoeWindow) -> Option<PoeWindow> {
    let hwnd = to_hwnd(prev.hwnd);
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return None;
        }
    }
    build(hwnd, prev.pid, prev.exe.clone())
}
