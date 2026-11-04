//! Win32 helpers for the HUD window: click-through, topmost, never focused.

use pov_game::Rect;

use std::ffi::c_void;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, HWND_TOPMOST, SET_WINDOW_POS_FLAGS, SW_HIDE, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};

/// Flags for showing the HUD: visible, never activated.
const SHOW_FLAGS: SET_WINDOW_POS_FLAGS = SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0 | SWP_SHOWWINDOW.0);

fn handle(hwnd: isize) -> HWND {
    HWND(hwnd as *mut c_void)
}

fn to_i32(v: i64) -> i32 {
    v.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// Bottom-center of the client area (physical pixels), moved by the user's offsets.
/// A HUD wider than the client overhangs both sides equally.
pub fn hud_rect(client: Rect, size: (i32, i32), offset_x: i64, bottom: i64) -> Rect {
    let (w, h) = size;
    let x = client.x as i64 + (client.w as i64 - w as i64) / 2 + offset_x;
    let y = client.y as i64 + client.h as i64 - h as i64 - bottom;
    Rect {
        x: to_i32(x),
        y: to_i32(y),
        w,
        h,
    }
}

/// Makes the window click-through, topmost, activation-free and hidden from the taskbar/Alt-Tab.
pub fn make_overlay(hwnd: isize) -> anyhow::Result<()> {
    let h = handle(hwnd);
    let add = (WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW).0 as isize;
    // tao keeps WS_EX_APPWINDOW, which would put the HUD into Alt+Tab despite WS_EX_TOOLWINDOW.
    let remove = WS_EX_APPWINDOW.0 as isize;
    unsafe {
        let cur = GetWindowLongPtrW(h, GWL_EXSTYLE);
        SetWindowLongPtrW(h, GWL_EXSTYLE, (cur | add) & !remove);
        let now = GetWindowLongPtrW(h, GWL_EXSTYLE);
        anyhow::ensure!(
            now & add == add && now & remove == 0,
            "could not set the overlay window style"
        );
        // WS_EX_TOPMOST is ignored by SetWindowLongPtr; it only takes effect through the z-order.
        SetWindowPos(
            h,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )?;
    }
    Ok(())
}

/// Shows the window topmost at `rect` without activating it.
pub fn show_at(hwnd: isize, rect: Rect) -> anyhow::Result<()> {
    unsafe {
        SetWindowPos(
            handle(hwnd),
            Some(HWND_TOPMOST),
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            SHOW_FLAGS,
        )?;
    }
    Ok(())
}

/// Hides the window; it keeps its overlay styles for the next `show_at`.
pub fn hide(hwnd: isize) -> anyhow::Result<()> {
    // ShowWindow returns the previous visibility, not success.
    let _ = unsafe { ShowWindow(handle(hwnd), SW_HIDE) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_flags_never_activate() {
        assert_eq!(SHOW_FLAGS.0 & SWP_NOACTIVATE.0, SWP_NOACTIVATE.0);
        assert_eq!(SHOW_FLAGS.0 & SWP_SHOWWINDOW.0, SWP_SHOWWINDOW.0);
    }

    #[test]
    fn hud_rect_follows_the_client_rect() {
        let r = hud_rect(
            Rect {
                x: 0,
                y: 0,
                w: 3440,
                h: 1440,
            },
            (800, 300),
            0,
            60,
        );
        assert_eq!(
            r,
            Rect {
                x: 1320,
                y: 1080,
                w: 800,
                h: 300
            }
        );

        let left = Rect {
            x: -2560,
            y: 0,
            w: 2560,
            h: 1440,
        };
        let r = hud_rect(left, (800, 300), -100, 0);
        assert_eq!(r.x, -2560 + 880 - 100);
        assert_eq!(r.y, 1140);

        let narrow = Rect {
            x: 50,
            y: 0,
            w: 800,
            h: 600,
        };
        let r = hud_rect(narrow, (1000, 100), 0, 0);
        assert_eq!(r.x, narrow.x - 100);
    }
}
