use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use pov_capture::{FrameSource, ReplaySource, WindowCapture};
use pov_game::{PoeWindow, Rect};
use vision::Rgb;
use windows::Graphics::Capture::GraphicsCaptureSession;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{ClientToScreen, CreateSolidBrush, HBRUSH, InvalidateRect};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
    SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW, MSG, PostMessageW, PostQuitMessage,
    RegisterClassW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, ShowWindow, TranslateMessage,
    WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};
use windows::core::w;

const COLOR: [u8; 3] = [200, 30, 60];

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_DESTROY {
        unsafe { PostQuitMessage(0) };
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// A solid-colored top-level window owned by its own message-pumping thread.
struct TestWindow {
    hwnd: isize,
    thread: Option<JoinHandle<()>>,
}

impl TestWindow {
    fn spawn(cw: i32, ch: i32) -> TestWindow {
        let (tx, rx) = mpsc::channel();
        let thread = thread::spawn(move || unsafe {
            let hinst = GetModuleHandleW(None).unwrap();
            let brush: HBRUSH =
                CreateSolidBrush(COLORREF(COLOR[0] as u32 | (COLOR[1] as u32) << 8 | (COLOR[2] as u32) << 16));
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: hinst.into(),
                lpszClassName: w!("PovCaptureTestWindow"),
                hbrBackground: brush,
                ..Default::default()
            };
            // Fails harmlessly when another test registered it already.
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("PovCaptureTestWindow"),
                w!("pov capture test"),
                WS_OVERLAPPEDWINDOW,
                100,
                100,
                cw,
                ch,
                None,
                None,
                Some(hinst.into()),
                None,
            )
            .unwrap();
            let mut r = RECT { left: 0, top: 0, right: cw, bottom: ch };
            AdjustWindowRectExForDpi(&mut r, WS_OVERLAPPEDWINDOW, false, WINDOW_EX_STYLE(0), GetDpiForWindow(hwnd))
                .unwrap();
            SetWindowPos(hwnd, None, 100, 100, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE)
                .unwrap();
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            tx.send(hwnd.0 as isize).unwrap();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
        let hwnd = rx.recv().unwrap();
        // Let the window paint and DWM compose it.
        thread::sleep(Duration::from_millis(300));
        TestWindow { hwnd, thread: Some(thread) }
    }

    fn poe_window(&self) -> PoeWindow {
        let hwnd = HWND(self.hwnd as *mut c_void);
        let mut r = RECT::default();
        let mut p = POINT { x: 0, y: 0 };
        unsafe {
            GetClientRect(hwnd, &mut r).unwrap();
            assert!(ClientToScreen(hwnd, &mut p).as_bool());
        }
        PoeWindow {
            hwnd: self.hwnd,
            pid: std::process::id(),
            exe: PathBuf::from("test.exe"),
            client: Rect { x: p.x, y: p.y, w: r.right - r.left, h: r.bottom - r.top },
            foreground: false,
            minimized: false,
        }
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(Some(HWND(self.hwnd as *mut c_void)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn capture_supported() -> bool {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let ok = GraphicsCaptureSession::IsSupported().unwrap_or(false);
    if !ok {
        println!("Windows.Graphics.Capture is not supported here, skipping");
    }
    ok
}

fn grab_until_some(src: &mut dyn FrameSource, win: &PoeWindow) -> Rgb {
    for _ in 0..20 {
        let t = Instant::now();
        let got = src.grab(win).unwrap();
        println!("grab took {:?}", t.elapsed());
        if let Some(img) = got {
            return img;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("no frame after 20 grabs");
}

fn assert_close(p: [u8; 3], want: [u8; 3]) {
    for c in 0..3 {
        assert!((p[c] as i32 - want[c] as i32).abs() <= 3, "pixel {p:?} is not close to {want:?}");
    }
}

fn assert_solid(img: &Rgb) {
    assert_eq!((img.w, img.h), (300, 200));
    assert_close(img.px[100 * img.w + 150], COLOR);
    // Edge midpoints too: a wrong crop would show the title bar or border. (Not the bottom
    // corners: Windows 11 rounds the window corners, those pixels are transparent black.)
    let (w, h) = (img.w, img.h);
    for (x, y) in [(w / 2, 0), (w / 2, h - 1), (0, h / 2), (w - 1, h / 2), (0, 0), (w - 1, 0)] {
        assert_close(img.px[y * w + x], COLOR);
    }
}

#[test]
fn captures_a_solid_window() {
    if !capture_supported() {
        return;
    }
    let tw = TestWindow::spawn(300, 200);
    let win = tw.poe_window();
    assert_eq!((win.client.w, win.client.h), (300, 200));
    let mut cap = WindowCapture::new().unwrap();
    let img = grab_until_some(&mut cap, &win);
    assert_solid(&img);
    // No new frame (static window): the last frame is returned again.
    let t = Instant::now();
    let again = cap.grab(&win).unwrap().expect("last frame again");
    println!("repeat grab took {:?}", t.elapsed());
    assert_solid(&again);
}

#[test]
fn pause_then_grab_restarts() {
    if !capture_supported() {
        return;
    }
    let tw = TestWindow::spawn(300, 200);
    let win = tw.poe_window();
    let mut cap = WindowCapture::new().unwrap();
    assert_solid(&grab_until_some(&mut cap, &win));
    cap.pause();
    assert_solid(&grab_until_some(&mut cap, &win));
}

/// CPU time (user + kernel) of the current thread.
fn thread_cpu() -> Duration {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};
    let (mut a, mut b, mut k, mut u) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe { GetThreadTimes(GetCurrentThread(), &mut a, &mut b, &mut k, &mut u).unwrap() };
    let t = |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) * 100;
    Duration::from_nanos(t(k) + t(u))
}

/// Informational: time per grab on a large window. Run with
/// `POV_CAPTURE_BIG=3440x1440 cargo test -p pov-capture --release -- --ignored --nocapture`.
#[test]
#[ignore]
fn grab_timing_large_window() {
    if !capture_supported() {
        return;
    }
    let spec = std::env::var("POV_CAPTURE_BIG").unwrap_or_else(|_| "1920x1080".into());
    let (w, h) = spec.split_once('x').unwrap();
    let (w, h): (i32, i32) = (w.parse().unwrap(), h.parse().unwrap());
    let tw = TestWindow::spawn(w, h);
    let win = tw.poe_window();
    println!("client {:?}", win.client);
    let mut cap = WindowCapture::new().unwrap();
    grab_until_some(&mut cap, &win);
    let n = 20;
    for repaint in [false, true] {
        let mut total = Duration::ZERO;
        let mut cpu = Duration::ZERO;
        for _ in 0..n {
            if repaint {
                // Force a repaint so the grab sees a new frame (copy path, not just re-read).
                unsafe {
                    let _ = InvalidateRect(Some(HWND(tw.hwnd as *mut c_void)), None, true);
                }
                thread::sleep(Duration::from_millis(40));
            }
            let (t, c) = (Instant::now(), thread_cpu());
            let img = cap.grab(&win).unwrap().unwrap();
            total += t.elapsed();
            cpu += thread_cpu() - c;
            assert_eq!((img.w as i32, img.h as i32), (win.client.w, win.client.h));
        }
        let what = if repaint { "new frame" } else { "repeat" };
        println!("{}x{} {what}: {:?} wall, {:?} thread CPU per grab", win.client.w, win.client.h, total / n, cpu / n);
    }
}

fn write_png(path: &std::path::Path, color: [u8; 3]) {
    image::RgbImage::from_pixel(4, 4, image::Rgb(color)).save(path).unwrap();
}

#[test]
fn replay_cycles_images() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.png");
    let b = dir.path().join("b.png");
    write_png(&a, [10, 20, 30]);
    write_png(&b, [40, 50, 60]);
    let mut src = ReplaySource::new(vec![a, b], Duration::from_millis(50)).unwrap();
    let win = PoeWindow {
        hwnd: 0,
        pid: 0,
        exe: PathBuf::new(),
        client: Rect { x: 0, y: 0, w: 0, h: 0 },
        foreground: false,
        minimized: false,
    };
    let first = src.grab(&win).unwrap().unwrap();
    assert_eq!((first.w, first.h), (4, 4));
    assert_eq!(first.px[0], [10, 20, 30]);
    thread::sleep(Duration::from_millis(60));
    let second = src.grab(&win).unwrap().unwrap();
    assert_eq!(second.px[0], [40, 50, 60]);
}
