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
    #[cfg(target_os = "linux")]
    {
        use x11rb::protocol::xproto::ConnectionExt;

        let queried = LINUX_X11.with(|cell| {
            let mut guard = cell.borrow_mut();
            if guard.is_none() {
                *guard = linux_x11_connect();
            }
            let mut clear = false;
            let result = if let Some(state) = guard.as_ref() {
                match state.conn.query_pointer(state.root) {
                    Ok(cookie) => match cookie.reply() {
                        Ok(reply) => {
                            let ppp = ctx.pixels_per_point();
                            Some(egui::pos2(
                                reply.root_x as f32 / ppp,
                                reply.root_y as f32 / ppp,
                            ))
                        }
                        Err(_) => {
                            clear = true;
                            None
                        }
                    },
                    Err(_) => {
                        clear = true;
                        None
                    }
                }
            } else {
                None
            };
            if clear {
                *guard = None;
            }
            result
        });

        if let Some(pos) = queried {
            return (pos, 1.0);
        }
    }

    // Fallback: egui's tracked pointer (works on Wayland or when X11 is
    // unavailable; only valid while the window is receiving events).
    (
        ctx.input(|i| i.pointer.latest_pos())
            .unwrap_or(egui::Pos2::ZERO),
        1.0_f32,
    )
}

// ---------------------------------------------------------------------------
// Linux X11 – shared connection state
// ---------------------------------------------------------------------------
//
// Both `cursor_info` (XQueryPointer) and `update_input_shape` (XShapeInput)
// need an X11 connection.  We keep it in a thread-local struct so it is opened
// once and reused, with a lazy-reconnect on error.
//
// IMPORTANT: `update_input_shape` MUST be the mechanism used on Linux to
// control which areas of the overlay receive mouse input.  The egui
// `ViewportCommand::MousePassthrough` path is processed *after* `update()`
// returns, so a winit call would silently override any XShape rectangle we set
// inside the frame.  Driving XShape directly from inside `update()` avoids
// that race entirely.

#[cfg(target_os = "linux")]
struct LinuxX11State {
    conn: x11rb::rust_connection::RustConnection,
    screen_num: usize,
    root: u32,
    /// Cached XID of our overlay window; found lazily by WM_NAME search.
    lasor_wid: Option<u32>,
}

#[cfg(target_os = "linux")]
std::thread_local! {
    static LINUX_X11: std::cell::RefCell<Option<LinuxX11State>> =
        std::cell::RefCell::new(linux_x11_connect());
}

#[cfg(target_os = "linux")]
fn linux_x11_connect() -> Option<LinuxX11State> {
    use x11rb::connection::Connection;
    x11rb::connect(None).ok().map(|(conn, screen_num)| {
        let root = conn.setup().roots[screen_num].root;
        LinuxX11State { conn, screen_num, root, lasor_wid: None }
    })
}

/// Walk the X11 window tree to find the first window whose `WM_NAME` equals
/// `name`.  Depth-limited to avoid stack overflow on exotic tree shapes.
#[cfg(target_os = "linux")]
fn find_window_by_name(
    conn: &x11rb::rust_connection::RustConnection,
    window: u32,
    name: &[u8],
    depth: u32,
) -> Option<u32> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

    if depth > 12 {
        return None;
    }

    // Check WM_NAME on this window.
    if let Ok(cookie) =
        conn.get_property(false, window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 32)
    {
        if let Ok(reply) = cookie.reply() {
            if reply.value == name {
                return Some(window);
            }
        }
    }

    // Walk children.
    if let Ok(cookie) = conn.query_tree(window) {
        if let Ok(tree) = cookie.reply() {
            let children = tree.children; // move out before iterating
            for child in children {
                if let Some(w) = find_window_by_name(conn, child, name, depth + 1) {
                    return Some(w);
                }
            }
        }
    }

    None
}

/// Set the X11 input shape for the lasor overlay window.
///
/// In Idle / Laser mode only the toolbar rect receives mouse events; the rest
/// of the overlay is fully click-through at the X11 level.  In Draw mode the
/// entire window is interactive so the user can paint strokes.
///
/// This must be called on Linux **instead of** (not in addition to)
/// `ctx.send_viewport_cmd(MousePassthrough(…))`.  The viewport-command path
/// is flushed after `update()` returns and would override an in-frame
/// `shape_rectangles` call on the same window.
#[cfg(target_os = "linux")]
pub fn update_input_shape(toolbar_rect: egui::Rect, draw_mode: bool, ppp: f32) {
    use x11rb::connection::Connection;
    use x11rb::protocol::shape::{ConnectionExt as ShapeExt, SK, SO};
    use x11rb::protocol::xproto::{ClipOrdering, Rectangle};

    // Skip until the toolbar has been measured at least once.
    if !toolbar_rect.min.is_finite() {
        return;
    }

    LINUX_X11.with(|cell| {
        let mut guard = cell.borrow_mut();

        if guard.is_none() {
            *guard = linux_x11_connect();
        }

        let state = match guard.as_mut() {
            Some(s) => s,
            None => return,
        };

        // Lazily find the lasor window by WM_NAME ("lasor").
        // Separated into two statements so the immutable borrow of
        // `state.conn` and `state.root` ends before the mutable write to
        // `state.lasor_wid`.
        if state.lasor_wid.is_none() {
            let wid = find_window_by_name(&state.conn, state.root, b"lasor", 0);
            state.lasor_wid = wid;
        }

        let wid = match state.lasor_wid {
            Some(w) => w,
            None => return, // window not found yet; retry next frame
        };

        if draw_mode {
            // Full-window input: a single large rectangle covers any display.
            let rects = [Rectangle { x: 0, y: 0, width: 32767, height: 32767 }];
            let _ = state
                .conn
                .shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, wid, 0, 0, &rects);
        } else {
            // Toolbar-only input: expand slightly so the hit-test matches the
            // visual expand used in `should_passthrough`.
            let r = toolbar_rect.expand(4.0);
            let rects = [Rectangle {
                x: (r.min.x * ppp).max(0.0) as i16,
                y: (r.min.y * ppp).max(0.0) as i16,
                width: (r.width() * ppp).max(1.0) as u16,
                height: (r.height() * ppp).max(1.0) as u16,
            }];
            let _ = state
                .conn
                .shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, wid, 0, 0, &rects);
        }

        let _ = state.conn.flush();
    });
}
