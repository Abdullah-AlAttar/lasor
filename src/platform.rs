use eframe::egui;

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
