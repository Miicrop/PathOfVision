use std::ffi::c_void;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use pov_game::Rect;
use pov_overlay::{hide, make_overlay, show_at};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GUITHREADINFO, GWL_EXSTYLE,
    GetForegroundWindow, GetGUIThreadInfo, GetMessageW, GetWindowLongPtrW,
    GetWindowThreadProcessId, IsWindowVisible, MSG, PostMessageW, PostQuitMessage, RegisterClassW,
    SW_SHOWNOACTIVATE, SetForegroundWindow, ShowWindow, TranslateMessage, WINDOW_EX_STYLE,
    WM_CLOSE, WM_DESTROY, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_OVERLAPPEDWINDOW,
};
use windows::core::w;

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_DESTROY {
        unsafe { PostQuitMessage(0) };
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// A top-level window owned by its own message-pumping thread; `visible` shows it without activation.
struct TestWindow {
    hwnd: isize,
    thread: Option<JoinHandle<()>>,
}

impl TestWindow {
    fn spawn(visible: bool) -> TestWindow {
        let (tx, rx) = mpsc::channel();
        let thread = thread::spawn(move || unsafe {
            let hinst = GetModuleHandleW(None).unwrap();
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: hinst.into(),
                lpszClassName: w!("PovOverlayTestWindow"),
                ..Default::default()
            };
            // Fails harmlessly when another test registered it already.
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("PovOverlayTestWindow"),
                w!("pov overlay test"),
                WS_OVERLAPPEDWINDOW,
                100,
                100,
                400,
                300,
                None,
                None,
                Some(hinst.into()),
                None,
            )
            .unwrap();
            if visible {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            tx.send(hwnd.0 as isize).unwrap();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
        TestWindow {
            hwnd: rx.recv().unwrap(),
            thread: Some(thread),
        }
    }

    fn handle(&self) -> HWND {
        HWND(self.hwnd as *mut c_void)
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(Some(self.handle()), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The active window of `w`'s own GUI thread (what activation would change even if the process is in the background).
fn thread_active(w: &TestWindow) -> HWND {
    unsafe {
        let tid = GetWindowThreadProcessId(w.handle(), None);
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(tid, &mut info).unwrap();
        info.hwndActive
    }
}

fn ex_style(w: &TestWindow) -> u32 {
    unsafe { GetWindowLongPtrW(w.handle(), GWL_EXSTYLE) as u32 }
}

#[test]
fn show_does_not_take_focus() {
    let a = TestWindow::spawn(true);
    let b = TestWindow::spawn(false);
    unsafe {
        let _ = SetForegroundWindow(a.handle());
    }
    thread::sleep(Duration::from_millis(200));
    // Windows may refuse SetForegroundWindow for a background process; then the contract is
    // that show_at leaves whatever was foreground untouched.
    let before = unsafe { GetForegroundWindow() };
    if before == a.handle() {
        println!("A is foreground; asserting it stays foreground");
    } else {
        println!("SetForegroundWindow refused; asserting foreground is unchanged");
    }

    make_overlay(b.hwnd).unwrap();
    show_at(
        b.hwnd,
        Rect {
            x: 200,
            y: 200,
            w: 300,
            h: 100,
        },
    )
    .unwrap();
    thread::sleep(Duration::from_millis(200));

    assert_eq!(unsafe { GetForegroundWindow() }, before);
    assert_ne!(unsafe { GetForegroundWindow() }, b.handle());
    assert_ne!(
        thread_active(&b),
        b.handle(),
        "show_at activated the overlay window"
    );
    assert!(unsafe { IsWindowVisible(b.handle()) }.as_bool());

    hide(b.hwnd).unwrap();
    assert!(!unsafe { IsWindowVisible(b.handle()) }.as_bool());
}

#[test]
fn overlay_window_is_click_through() {
    let b = TestWindow::spawn(false);
    make_overlay(b.hwnd).unwrap();
    let ex = ex_style(&b);
    for flag in [
        WS_EX_TRANSPARENT,
        WS_EX_NOACTIVATE,
        WS_EX_LAYERED,
        WS_EX_TOOLWINDOW,
        WS_EX_TOPMOST,
    ] {
        assert_eq!(ex & flag.0, flag.0, "missing ex style {:#x}", flag.0);
    }
}
