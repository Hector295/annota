//! Capture through GDI: one `BitBlt` of the whole virtual desktop.
//!
//! Windows lets any desktop app read the screen, so there is no permission
//! step. The process is per-monitor DPI aware (see `main.rs`), so all
//! coordinates here are physical pixels, even with mixed monitor scales.

use anyhow::{Context, bail};
use gtk::{gdk, glib, prelude::*};
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap,
    CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, EnumDisplayMonitors, GetDC,
    GetDIBits, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, ROP_CODE, ReleaseDC, SRCCOPY,
    SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};
use windows::core::BOOL;

use super::{CaptureError, CapturedImage, MonitorRect, ScreenshotBackend};

pub struct GdiBackend;

impl ScreenshotBackend for GdiBackend {
    async fn capture(&self) -> Result<CapturedImage, CaptureError> {
        let raw = grab_desktop()?;
        let stride = raw.width as usize * 4;
        let bytes = glib::Bytes::from_owned(raw.pixels);
        // GDI leaves the fourth byte undefined, so treat it as padding.
        let texture = gdk::MemoryTexture::new(
            raw.width,
            raw.height,
            gdk::MemoryFormat::B8g8r8x8,
            &bytes,
            stride,
        );
        Ok(CapturedImage {
            texture: texture.upcast(),
            monitors: raw.monitors,
        })
    }
}

struct RawCapture {
    width: i32,
    height: i32,
    /// BGRx, top-down, tightly packed.
    pixels: Vec<u8>,
    monitors: Vec<MonitorRect>,
}

fn grab_desktop() -> anyhow::Result<RawCapture> {
    // SAFETY: plain GDI calls on handles created and released here.
    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        if width <= 0 || height <= 0 {
            bail!("no desktop to capture");
        }

        let screen = GetDC(None);
        if screen.is_invalid() {
            bail!("GetDC failed");
        }
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let previous = SelectObject(memory, bitmap.into());

        // CAPTUREBLT includes layered (translucent) windows.
        let copied = BitBlt(
            memory,
            0,
            0,
            width,
            height,
            Some(screen),
            x,
            y,
            ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0),
        );

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                // Negative height: rows top-down, like GDK expects.
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        let lines = if copied.is_ok() {
            GetDIBits(
                memory,
                bitmap,
                0,
                height as u32,
                Some(pixels.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };

        SelectObject(memory, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);

        copied.context("BitBlt failed")?;
        if lines != height {
            bail!("GetDIBits copied {lines} of {height} lines");
        }

        let monitors = monitor_rects()
            .into_iter()
            .map(|r| MonitorRect {
                x: r.left - x,
                y: r.top - y,
                width: r.right - r.left,
                height: r.bottom - r.top,
            })
            .collect();
        Ok(RawCapture {
            width,
            height,
            pixels,
            monitors,
        })
    }
}

/// Physical rectangles of all monitors, in virtual-desktop coordinates.
fn monitor_rects() -> Vec<RECT> {
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        // SAFETY: `data` is the `Vec` passed below, alive for the call.
        let rects = unsafe { &mut *(data.0 as *mut Vec<RECT>) };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            rects.push(info.rcMonitor);
        }
        BOOL::from(true)
    }

    let mut rects: Vec<RECT> = Vec::new();
    // SAFETY: the callback only runs during this call.
    let _ = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut rects as *mut Vec<RECT> as isize),
        )
    };
    rects
}
