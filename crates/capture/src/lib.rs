//! Frame sources for the live scanner: Windows.Graphics.Capture window capture and an
//! image replay source for tests and offline runs.
//!
//! The game window is captured (not the monitor) so our own overlay windows never show up
//! in the frames. Capture is purely passive: no input is ever sent to the game.
use std::ffi::c_void;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Context;
use pov_game::PoeWindow;
use vision::Rgb;
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HMODULE, HWND, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
    D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
    ID3D11Texture2D,
};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::core::{Interface, factory};

/// A source of game frames (client area of the game window, RGB).
pub trait FrameSource: Send {
    /// The newest frame, or the previous one again when nothing new arrived.
    /// `Ok(None)` while no frame has been received yet.
    fn grab(&mut self, window: &PoeWindow) -> anyhow::Result<Option<Rgb>>;
    /// Stop capturing until the next `grab`.
    fn pause(&mut self);
}

const PIXEL_FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;
const BUFFERS: i32 = 2;

/// An active capture of one window.
struct Session {
    hwnd: isize,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    /// Size the pool buffers were created with.
    size: SizeInt32,
}

impl Session {
    fn close(self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

/// CPU-readable copy of the last captured client area.
struct Staging {
    tex: ID3D11Texture2D,
    w: u32,
    h: u32,
}

/// Captures a window with Windows.Graphics.Capture and returns its client area.
pub struct WindowCapture {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt_device: IDirect3DDevice,
    session: Option<Session>,
    /// Reused while the client size is unchanged; holds the last frame once `has_frame`.
    staging: Option<Staging>,
    has_frame: bool,
}

// SAFETY: the D3D11 device is created without D3D11_CREATE_DEVICE_SINGLETHREADED (free-threaded),
// the WinRT capture objects are agile and the frame pool is free-threaded. The immediate context
// is only ever used through `&mut self`, i.e. by one thread at a time.
unsafe impl Send for WindowCapture {}

impl WindowCapture {
    pub fn new() -> anyhow::Result<WindowCapture> {
        // Background threads have no apartment yet; an already initialized one is fine too.
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        anyhow::ensure!(
            GraphicsCaptureSession::IsSupported().unwrap_or(false),
            "Windows.Graphics.Capture is not supported on this system"
        );
        let mut device = None;
        let mut context = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .context("D3D11CreateDevice")?;
        }
        let device: ID3D11Device = device.context("no D3D11 device")?;
        let context = context.context("no D3D11 device context")?;
        let dxgi: IDXGIDevice = device.cast()?;
        let winrt_device: IDirect3DDevice = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? }.cast()?;
        Ok(WindowCapture { device, context, winrt_device, session: None, staging: None, has_frame: false })
    }

    fn start(&self, hwnd: isize) -> anyhow::Result<Session> {
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem =
            unsafe { interop.CreateForWindow(HWND(hwnd as *mut c_void)) }.context("CreateForWindow")?;
        let size = item.Size()?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&self.winrt_device, PIXEL_FORMAT, BUFFERS, size)?;
        let session = pool.CreateCaptureSession(&item)?;
        let _ = session.SetIsCursorCaptureEnabled(false);
        let _ = session.SetIsBorderRequired(false);
        session.StartCapture()?;
        Ok(Session { hwnd, pool, session, size })
    }

    fn stop(&mut self) {
        if let Some(s) = self.session.take() {
            s.close();
        }
        self.has_frame = false;
    }

    /// Copies the client-area part of the newest frame (if any) into the staging texture.
    fn take_newest_frame(&mut self, window: &PoeWindow) -> anyhow::Result<()> {
        let Some(s) = &mut self.session else {
            return Ok(());
        };
        let mut newest = None;
        while let Ok(frame) = s.pool.TryGetNextFrame() {
            if let Some(old) = newest.replace(frame) {
                let _ = old.Close();
            }
        }
        let Some(frame) = newest else { return Ok(()) };

        let content = frame.ContentSize()?;
        if content != s.size {
            // Window resized: new buffers from the next frame on. This frame is still usable.
            s.pool.Recreate(&self.winrt_device, PIXEL_FORMAT, BUFFERS, content)?;
            s.size = content;
        }
        let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
        let tex: ID3D11Texture2D = unsafe { access.GetInterface()? };
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { tex.GetDesc(&mut desc) };

        // The frame shows the window's extended frame bounds; the client area sits inside.
        let mut bounds = RECT::default();
        unsafe {
            DwmGetWindowAttribute(
                HWND(window.hwnd as *mut c_void),
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut bounds as *mut RECT as *mut c_void,
                size_of::<RECT>() as u32,
            )
            .context("DwmGetWindowAttribute")?;
        }
        let avail_w = (content.Width.max(0) as u32).min(desc.Width);
        let avail_h = (content.Height.max(0) as u32).min(desc.Height);
        let Some(crop) = client_crop(&bounds, window, avail_w, avail_h) else {
            let _ = frame.Close();
            return Ok(());
        };

        let (w, h) = (crop.right - crop.left, crop.bottom - crop.top);
        if !self.staging.as_ref().is_some_and(|st| st.w == w && st.h == h) {
            self.staging = Some(self.create_staging(w, h)?);
        }
        let st = self.staging.as_ref().expect("staging");
        unsafe { self.context.CopySubresourceRegion(&st.tex, 0, 0, 0, 0, &tex, 0, Some(&crop)) };
        let _ = frame.Close();
        self.has_frame = true;
        Ok(())
    }

    fn create_staging(&self, w: u32, h: u32) -> anyhow::Result<Staging> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut tex = None;
        unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut tex)) }.context("CreateTexture2D")?;
        Ok(Staging { tex: tex.context("no staging texture")?, w, h })
    }

    /// Converts the staging texture (the last frame) to RGB.
    fn read_staging(&self) -> anyhow::Result<Rgb> {
        let st = self.staging.as_ref().expect("staging");
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe { self.context.Map(&st.tex, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }.context("Map")?;
        let (w, h, pitch) = (st.w as usize, st.h as usize, mapped.RowPitch as usize);
        let len = if h == 0 { 0 } else { pitch * (h - 1) + w * 4 };
        // SAFETY: a mapped w x h BGRA texture spans `RowPitch` bytes per row.
        let bytes = unsafe { std::slice::from_raw_parts(mapped.pData as *const u8, len) };
        let rgb = Rgb::from_bgra(w, h, pitch, bytes);
        unsafe { self.context.Unmap(&st.tex, 0) };
        rgb
    }
}

/// The client area as a box inside a captured frame of `avail_w x avail_h` pixels whose
/// origin is the window's extended frame bounds. All inputs are physical pixels.
fn client_crop(bounds: &RECT, window: &PoeWindow, avail_w: u32, avail_h: u32) -> Option<D3D11_BOX> {
    let c = &window.client;
    let x0 = (c.x - bounds.left).clamp(0, avail_w as i32) as u32;
    let y0 = (c.y - bounds.top).clamp(0, avail_h as i32) as u32;
    let x1 = (x0 + c.w.max(0) as u32).min(avail_w);
    let y1 = (y0 + c.h.max(0) as u32).min(avail_h);
    (x1 > x0 && y1 > y0).then_some(D3D11_BOX { left: x0, top: y0, front: 0, right: x1, bottom: y1, back: 1 })
}

impl FrameSource for WindowCapture {
    fn grab(&mut self, window: &PoeWindow) -> anyhow::Result<Option<Rgb>> {
        if self.session.as_ref().is_some_and(|s| s.hwnd != window.hwnd) {
            self.stop();
        }
        if self.session.is_none() {
            self.session = Some(self.start(window.hwnd)?);
        }
        if let Err(e) = self.take_newest_frame(window) {
            // Start over on the next grab (e.g. window closed, device removed).
            self.stop();
            return Err(e);
        }
        if !self.has_frame {
            return Ok(None);
        }
        self.read_staging().map(Some)
    }

    fn pause(&mut self) {
        self.stop();
    }
}

impl Drop for WindowCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Replays still images, ignoring the window: each image is shown for `hold`, then the next,
/// cycling. The cycle starts at the first `grab`.
pub struct ReplaySource {
    images: Vec<Rgb>,
    hold: Duration,
    start: Option<Instant>,
}

impl ReplaySource {
    pub fn new(paths: Vec<PathBuf>, hold: Duration) -> anyhow::Result<ReplaySource> {
        anyhow::ensure!(!paths.is_empty(), "replay needs at least one image");
        anyhow::ensure!(!hold.is_zero(), "replay hold must be positive");
        let images = paths
            .iter()
            .map(|p| Rgb::load(p).with_context(|| format!("loading {}", p.display())))
            .collect::<anyhow::Result<_>>()?;
        Ok(ReplaySource { images, hold, start: None })
    }
}

impl FrameSource for ReplaySource {
    fn grab(&mut self, _window: &PoeWindow) -> anyhow::Result<Option<Rgb>> {
        let start = *self.start.get_or_insert_with(Instant::now);
        let i = (start.elapsed().as_nanos() / self.hold.as_nanos()) as usize % self.images.len();
        Ok(Some(self.images[i].clone()))
    }

    /// Nothing to release; the cycle keeps running on the wall clock.
    fn pause(&mut self) {}
}
