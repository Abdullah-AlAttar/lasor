use eframe::egui;

// ---------------------------------------------------------------------------
// Primary monitor helpers
// ---------------------------------------------------------------------------

/// Returns the bottom-right corner of the **primary** monitor in egui logical
/// coordinates (relative to the virtual-screen top-left).
///
/// On Windows the primary monitor is always anchored at physical (0, 0), so
/// its size is given directly by `SM_CXSCREEN` / `SM_CYSCREEN`.  We then
/// subtract the virtual-screen origin (`virt_x`, `virt_y`) and divide by
/// `pixels_per_point` to arrive at egui-space coordinates.
#[cfg(windows)]
pub fn primary_monitor_bottom_right(ctx: &egui::Context, virt_x: i32, virt_y: i32) -> egui::Pos2 {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN, SPI_GETWORKAREA, SystemParametersInfoW,
    };

    // Prefer the work area (excludes taskbar / desktop toolbars) so the
    // toolbar is never placed behind the system taskbar.
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            &mut rc as *mut RECT as *mut core::ffi::c_void,
            0,
        )
    };
    let (r, b) = if ok != 0 {
        (rc.right, rc.bottom)
    } else {
        // Fallback: full screen dimensions if SPI_GETWORKAREA fails.
        let w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        (w, h)
    };

    let ppp = ctx.pixels_per_point();
    egui::pos2((r - virt_x) as f32 / ppp, (b - virt_y) as f32 / ppp)
}

/// Fallback for non-Windows: use the bottom-right of whatever screen rect
/// egui reports (typically the primary monitor on single-monitor setups).
#[cfg(not(windows))]
pub fn primary_monitor_bottom_right(ctx: &egui::Context, _virt_x: i32, _virt_y: i32) -> egui::Pos2 {
    ctx.screen_rect().max
}

/// Returns `(cursor_pos, monitor_scale)` where:
/// - `cursor_pos` is the OS cursor position in egui logical coordinates
///   relative to the virtual-screen top-left corner.
/// - `monitor_scale` is the DPI scale factor for the monitor under the cursor
///   divided by `pixels_per_point`, so that physical dot sizes stay constant
///   across mixed-DPI setups.
#[cfg(windows)]
pub fn cursor_info(ctx: &egui::Context, virt_x: i32, virt_y: i32) -> (egui::Pos2, f32) {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::MonitorFromPoint;
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut pt = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut pt) };

    let ppp = ctx.pixels_per_point();
    let pos = egui::pos2((pt.x - virt_x) as f32 / ppp, (pt.y - virt_y) as f32 / ppp);

    let scale = unsafe {
        let monitor = MonitorFromPoint(pt, 2); // MONITOR_DEFAULTTONEAREST
        let mut dx = 96u32;
        let mut dy = 96u32;
        GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        (dx as f32 / 96.0) / ppp
    };

    (pos, scale)
}

#[cfg(not(windows))]
pub fn cursor_info(ctx: &egui::Context, _virt_x: i32, _virt_y: i32) -> (egui::Pos2, f32) {
    (
        ctx.input(|i| i.pointer.latest_pos())
            .unwrap_or(egui::Pos2::ZERO),
        1.0_f32,
    )
}
