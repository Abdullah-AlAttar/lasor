mod draw;
mod laser;
mod toolbar;

use std::collections::VecDeque;
use std::time::Instant;

use eframe::egui;

use crate::config::Config;
use crate::mode::Mode;
use crate::platform;

// ---------------------------------------------------------------------------
// Trail point
// ---------------------------------------------------------------------------

pub(crate) struct TrailPoint {
    pub(crate) pos: egui::Pos2,
    pub(crate) time: Instant,
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

pub struct LasorApp {
    pub cfg: Config,
    pub mode: Mode,

    // ── Window positioning (Windows multi-monitor) ─────────────────────────
    pub virt_x: i32,
    pub virt_y: i32,
    pub virt_w: i32,
    pub virt_h: i32,
    positioned: bool,

    // ── Laser state ────────────────────────────────────────────────────────
    pub(crate) pointer_pos: Option<egui::Pos2>,
    /// Cursor position from the previous frame, used to interpolate
    /// intermediate trail points between frames so the trail is gap-free.
    pub(crate) last_frame_pos: Option<egui::Pos2>,
    pub(crate) trail: VecDeque<TrailPoint>,

    // ── Draw / annotation state ────────────────────────────────────────────
    /// Completed strokes; each entry is (points, rgb_color).
    pub(crate) strokes: Vec<(Vec<egui::Pos2>, [u8; 3])>,
    /// The stroke currently being drawn (mouse held down).
    pub(crate) current_stroke: Vec<egui::Pos2>,

    // ── Toolbar ────────────────────────────────────────────────────────────
    /// Free position of the toolbar (top-left corner).
    /// `None` until the first frame when screen dimensions are known.
    pub(crate) toolbar_pos: Option<egui::Pos2>,
    /// Screen-space rect of the toolbar, updated every frame.
    /// Used to decide when to disable mouse passthrough.
    pub(crate) toolbar_rect: egui::Rect,
    /// Last passthrough value sent to the OS; only re-sent when it changes.
    last_passthrough: bool,
    /// Currently selected drawing colour (RGB); overrides `cfg.draw_color` at runtime.
    pub(crate) draw_color_rgb: [u8; 3],
    /// Whether the colour palette popup is expanded.
    pub(crate) show_color_picker: bool,
    /// Screen-space rect of the colour toggle button; used to anchor the popup.
    pub(crate) color_toggle_rect: egui::Rect,
}

impl LasorApp {
    pub fn new(cfg: Config, virt_x: i32, virt_y: i32, virt_w: i32, virt_h: i32) -> Self {
        let draw_color_rgb = cfg.draw_color;
        Self {
            cfg,
            draw_color_rgb,
            mode: Mode::Idle,
            virt_x,
            virt_y,
            virt_w,
            virt_h,
            positioned: false,
            pointer_pos: None,
            last_frame_pos: None,
            trail: VecDeque::new(),
            strokes: Vec::new(),
            current_stroke: Vec::new(),
            toolbar_pos: None,
            // Rect::NOTHING has inverted min/max so `contains` always returns
            // false – safe to use as an "uninitialised" sentinel.
            toolbar_rect: egui::Rect::NOTHING,
            // Start with `false` so the very first frame always sends the
            // passthrough command (the window is not passthrough by default).
            // On frame 1, should_passthrough() will typically return `true`
            // (cursor away from toolbar, Idle mode) and `true != false` fires
            // the send, putting the window into click-through mode immediately.
            last_passthrough: false,
            show_color_picker: false,
            color_toggle_rect: egui::Rect::NOTHING,
        }
    }

    // -----------------------------------------------------------------------
    // Passthrough decision
    // -----------------------------------------------------------------------

    /// Returns `true` if OS mouse events should pass through to the window
    /// underneath the overlay.
    fn should_passthrough(&self, cursor: egui::Pos2) -> bool {
        // Draw mode needs to capture all pointer events for painting.
        if self.mode == Mode::Draw {
            return false;
        }
        // Allow clicking the toolbar regardless of mode.
        if self.toolbar_rect.expand(4.0).contains(cursor) {
            return false;
        }
        true
    }

    // -----------------------------------------------------------------------
    // Window positioning
    // -----------------------------------------------------------------------

    /// Span the window across all monitors on the first frame.
    ///
    /// Uses `SetWindowPos` with raw physical-pixel coordinates to bypass
    /// eframe's DPI conversion, which is unreliable on mixed-DPI setups.
    fn position_window_once(&mut self) {
        if self.positioned {
            return;
        }
        self.positioned = true;

        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                FindWindowW, HWND_TOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetWindowPos,
            };
            let title: Vec<u16> = "lasor\0".encode_utf16().collect();
            let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
            if !hwnd.is_null() {
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    self.virt_x,
                    self.virt_y,
                    self.virt_w,
                    self.virt_h,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// eframe::App impl
// ---------------------------------------------------------------------------

impl eframe::App for LasorApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Fully transparent background — the OS composites our painted shapes.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.position_window_once();

        // ── Cursor position & per-monitor DPI ─────────────────────────────
        let (cursor_pos, monitor_scale) = platform::cursor_info(ctx, self.virt_x, self.virt_y);

        // ── Dynamic passthrough ────────────────────────────────────────────
        // Only issue the viewport command when the value actually changes;
        // sending it every frame forces an OS window-style round-trip each
        // repaint even when the passthrough state is stable.
        let passthrough = self.should_passthrough(cursor_pos);
        if passthrough != self.last_passthrough {
            self.last_passthrough = passthrough;
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(passthrough));
        }

        // Draw mode uses a crosshair; egui still shows pointer over buttons.
        if self.mode == Mode::Draw {
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }

        // ── Mode-specific logic ────────────────────────────────────────────
        match self.mode {
            Mode::Idle => {
                self.trail.clear();
                self.pointer_pos = None;
                // Reset so re-entering Laser mode doesn't interpolate from a
                // stale position.
                self.last_frame_pos = None;
            }
            Mode::Laser => self.update_laser(cursor_pos),
            Mode::Draw => self.update_draw(ctx),
        }

        // ── Repaint scheduling ─────────────────────────────────────────────
        match self.mode {
            Mode::Idle => {
                // 200 ms gives ~5 fps of cursor-position polling.  This is
                // more than fast enough to detect when the cursor enters the
                // toolbar area and disable passthrough.  When passthrough is
                // already OFF, egui repaints immediately on every mouse-move
                // event anyway, so toolbar responsiveness is unaffected.
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
            _ => ctx.request_repaint(),
        }

        // ── Render overlay then toolbar ────────────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let painter = ui.painter();
                self.paint_strokes(painter, monitor_scale);
                if self.mode == Mode::Laser {
                    self.paint_laser(painter, monitor_scale);
                }
            });

        self.show_toolbar(ctx);
        self.show_color_palette_popup(ctx);
    }
}
