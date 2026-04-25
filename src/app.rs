use std::collections::VecDeque;
use std::time::Instant;

use eframe::egui;

use crate::config::Config;
use crate::mode::Mode;
use crate::platform;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Distance from the primary-monitor bottom-right corner used for the initial
/// toolbar placement (logical pixels).
/// Padding from the screen edge when auto-positioning the toolbar (logical px).
const TOOLBAR_EDGE_MARGIN: f32 = 10.0;

/// Preset colour swatches shown in the toolbar while Draw mode is active.
const DRAW_PALETTE: &[([u8; 3], &str)] = &[
    ([255, 220, 0], "Yellow"),
    ([255, 80, 80], "Red"),
    ([255, 160, 0], "Orange"),
    ([80, 220, 80], "Green"),
    ([0, 200, 255], "Cyan"),
    ([80, 120, 255], "Blue"),
    ([220, 80, 220], "Magenta"),
    ([255, 255, 255], "White"),
];

// ---------------------------------------------------------------------------
// Trail point
// ---------------------------------------------------------------------------

struct TrailPoint {
    pos: egui::Pos2,
    time: Instant,
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
    pointer_pos: Option<egui::Pos2>,
    /// Cursor position from the previous frame, used to interpolate
    /// intermediate trail points between frames so the trail is gap-free.
    last_frame_pos: Option<egui::Pos2>,
    trail: VecDeque<TrailPoint>,

    // ── Draw / annotation state ────────────────────────────────────────────
    /// Completed strokes; each entry is (points, rgb_color).
    strokes: Vec<(Vec<egui::Pos2>, [u8; 3])>,
    /// The stroke currently being drawn (mouse held down).
    current_stroke: Vec<egui::Pos2>,

    // ── Toolbar ────────────────────────────────────────────────────────────
    /// Free position of the toolbar (top-left corner).
    /// `None` until the first frame when screen dimensions are known.
    toolbar_pos: Option<egui::Pos2>,
    /// Screen-space rect of the toolbar, updated every frame.
    /// Used to decide when to disable mouse passthrough.
    toolbar_rect: egui::Rect,
    /// Last passthrough value sent to the OS; only re-sent when it changes.
    last_passthrough: bool,
    /// Currently selected drawing colour (RGB); overrides `cfg.draw_color` at runtime.
    draw_color_rgb: [u8; 3],
    /// Whether the colour palette is expanded in the toolbar.
    show_color_picker: bool,
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

    // -----------------------------------------------------------------------
    // Per-mode update logic
    // -----------------------------------------------------------------------

    fn update_laser(&mut self, cursor: egui::Pos2) {
        self.pointer_pos = Some(cursor);

        // Interpolate intermediate trail points between last frame's cursor
        // position and the current one, so the trail is fully gap-free even
        // when the mouse moves many pixels between frames.
        let start = self.last_frame_pos.unwrap_or(cursor);
        let dist = start.distance(cursor);
        let now = Instant::now();

        // If the cursor jumped an unreasonably large distance (e.g. after
        // switching back from Idle, or a display teleport), skip interpolation
        // for that frame to avoid a spurious long streak.
        const MAX_INTERP_DIST: f32 = 300.0;
        // Spacing between interpolated dots in logical pixels. Smaller values
        // produce more dots and a silkier trail at the cost of more geometry.
        const STEP: f32 = 1.5;

        if dist > 0.0 && dist <= MAX_INTERP_DIST {
            let steps = ((dist / STEP).ceil() as usize).max(1);
            for i in 1..=steps {
                let t = i as f32 / steps as f32;
                let p = start.lerp(cursor, t);
                self.trail.push_back(TrailPoint { pos: p, time: now });
            }
        } else {
            // Barely moved, or teleport – just record the current position.
            if self
                .trail
                .back()
                .is_none_or(|tp| tp.pos.distance(cursor) > 0.5)
            {
                self.trail.push_back(TrailPoint {
                    pos: cursor,
                    time: now,
                });
            }
        }

        self.last_frame_pos = Some(cursor);

        let cutoff = self.cfg.trail_duration_secs;
        self.trail
            .retain(|p| p.time.elapsed().as_secs_f32() < cutoff);
    }

    fn update_draw(&mut self, ctx: &egui::Context) {
        let (pressed, released, down, pos_opt, undo) = ctx.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_released(),
                i.pointer.primary_down(),
                i.pointer.latest_pos(),
                i.key_pressed(egui::Key::Z) && i.modifiers.ctrl,
            )
        });

        if undo {
            if !self.current_stroke.is_empty() {
                self.current_stroke.clear();
            } else {
                self.strokes.pop();
            }
        }

        let Some(pos) = pos_opt else { return };

        // Don't start or continue strokes while the user is clicking the toolbar.
        if self.toolbar_rect.expand(4.0).contains(pos) {
            if !self.current_stroke.is_empty() {
                let stroke = std::mem::take(&mut self.current_stroke);
                self.strokes.push((stroke, self.draw_color_rgb));
            }
            return;
        }

        if pressed {
            self.current_stroke.clear();
            self.current_stroke.push(pos);
        } else if down {
            let far_enough = self
                .current_stroke
                .last()
                .is_none_or(|last| last.distance(pos) > 2.0);
            if far_enough {
                self.current_stroke.push(pos);
            }
        } else if released && !self.current_stroke.is_empty() {
            let stroke = std::mem::take(&mut self.current_stroke);
            self.strokes.push((stroke, self.draw_color_rgb));
        }
    }

    // -----------------------------------------------------------------------
    // Rendering
    // -----------------------------------------------------------------------

    fn paint_overlay(&self, painter: &egui::Painter, monitor_scale: f32) {
        // ── Annotation strokes (always visible, even in Laser / Idle mode) ─
        // Each completed stroke carries its own colour captured at draw time.
        for (stroke, color_rgb) in &self.strokes {
            let [dr, dg, db] = *color_rgb;
            let draw_color = egui::Color32::from_rgba_unmultiplied(dr, dg, db, self.cfg.draw_alpha);
            let draw_stroke = egui::Stroke::new(self.cfg.draw_width * monitor_scale, draw_color);
            Self::paint_stroke(painter, stroke, draw_stroke);
        }
        if !self.current_stroke.is_empty() {
            let [dr, dg, db] = self.draw_color_rgb;
            let draw_color = egui::Color32::from_rgba_unmultiplied(dr, dg, db, self.cfg.draw_alpha);
            let draw_stroke = egui::Stroke::new(self.cfg.draw_width * monitor_scale, draw_color);
            Self::paint_stroke(painter, &self.current_stroke, draw_stroke);
        }

        // ── Laser trail + head ─────────────────────────────────────────────
        if self.mode == Mode::Laser {
            self.paint_laser(painter, monitor_scale);
        }
    }

    /// Draws the fading trail and the dot head; only called in `Laser` mode.
    fn paint_laser(&self, painter: &egui::Painter, monitor_scale: f32) {
        let [cr, cg, cb] = self.cfg.dot_color;
        let trail_len = self.trail.len();

        // Helper closure: compute the visual blend weight for trail index `i`.
        let blend_for = |i: usize, age: f32| -> f32 {
            let t = 1.0 - (age / self.cfg.trail_duration_secs).clamp(0.0, 1.0);
            let pos_t = (i + 1) as f32 / trail_len.max(1) as f32;
            (t * pos_t).powf(0.5)
        };

        // ── Pass 1: connected ribbon ──────────────────────────────────────
        // Draw a thick line segment between every consecutive pair of trail
        // points.  This fills any remaining sub-pixel gaps between the circles
        // drawn in pass 2, producing a perfectly continuous trail ribbon.
        for (i, (tp0, tp1)) in self.trail.iter().zip(self.trail.iter().skip(1)).enumerate() {
            let blend = blend_for(i, tp0.time.elapsed().as_secs_f32())
                .max(blend_for(i + 1, tp1.time.elapsed().as_secs_f32()));
            let alpha = (blend * self.cfg.trail_alpha_max as f32) as u8;
            let width = (self.cfg.trail_min_radius
                + blend * (self.cfg.trail_max_radius - self.cfg.trail_min_radius))
                * monitor_scale
                * 2.0;
            painter.line_segment(
                [tp0.pos, tp1.pos],
                egui::Stroke::new(
                    width,
                    egui::Color32::from_rgba_unmultiplied(cr, cg, cb, alpha),
                ),
            );
        }

        // ── Pass 2: round caps at every trail point ───────────────────────
        // Circles produce round end-caps on the ribbon and smooth out the
        // angular joints between line segments.
        for (i, tp) in self.trail.iter().enumerate() {
            let blend = blend_for(i, tp.time.elapsed().as_secs_f32());
            let alpha = (blend * self.cfg.trail_alpha_max as f32) as u8;
            let radius = (self.cfg.trail_min_radius
                + blend * (self.cfg.trail_max_radius - self.cfg.trail_min_radius))
                * monitor_scale;
            painter.circle_filled(
                tp.pos,
                radius,
                egui::Color32::from_rgba_unmultiplied(cr, cg, cb, alpha),
            );
        }

        // ── Dot head ─────────────────────────────────────────────────────
        if let Some(pos) = self.pointer_pos {
            let r = self.cfg.dot_radius * monitor_scale;
            painter.circle_filled(
                pos,
                r,
                egui::Color32::from_rgba_unmultiplied(cr, cg, cb, self.cfg.dot_alpha),
            );
            painter.circle_stroke(
                pos,
                r,
                egui::Stroke::new(
                    self.cfg.dot_stroke_width * monitor_scale,
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, self.cfg.dot_stroke_alpha),
                ),
            );
        }
    }

    /// Draw a single polyline stroke; handles single-point strokes as a dot.
    fn paint_stroke(painter: &egui::Painter, points: &[egui::Pos2], stroke: egui::Stroke) {
        match points.len() {
            0 => {}
            1 => {
                painter.circle_filled(points[0], stroke.width * 0.5, stroke.color);
            }
            _ => {
                for segment in points.windows(2) {
                    painter.line_segment([segment[0], segment[1]], stroke);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Toolbar UI

    // -----------------------------------------------------------------------

    fn show_toolbar(&mut self, ctx: &egui::Context) {
        let (virt_x, virt_y) = (self.virt_x, self.virt_y);

        // Two-phase positioning so the toolbar lands exactly at the
        // primary-monitor bottom-right regardless of its actual rendered size.
        //
        //  • Frame 1: toolbar_rect is Rect::NOTHING (not yet measured).
        //             Use a rough provisional position so something renders.
        //  • Frame 2+: actual size is known from the previous frame.
        //              Compute the exact top-left that places the toolbar's
        //              bottom-right at `br - TOOLBAR_EDGE_MARGIN`, then lock
        //              toolbar_pos to Some so subsequent drags work normally.
        // `primary_monitor_bottom_right` calls SPI_GETWORKAREA on Windows.
        // Only invoke it while toolbar_pos is still unknown (first two frames);
        // after that the position is locked and `br` is never used again.
        let pos = if let Some(p) = self.toolbar_pos {
            p
        } else {
            let br = platform::primary_monitor_bottom_right(ctx, virt_x, virt_y);
            let (tw, th) = (self.toolbar_rect.width(), self.toolbar_rect.height());
            if tw > 0.0 && th > 0.0 {
                let m = TOOLBAR_EDGE_MARGIN;
                let p = egui::pos2(br.x - tw - m, br.y - th - m);
                self.toolbar_pos = Some(p);
                p
            } else {
                // First frame only: provisional position until size is measured.
                egui::pos2(br.x - 350.0, br.y - 50.0)
            }
        };

        let inner_resp = egui::Area::new(egui::Id::new("lasor_toolbar"))
            .fixed_pos(pos)
            .interactable(true)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                toolbar_frame().show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                    ui.spacing_mut().button_padding = egui::vec2(10.0, 6.0);
                    ui.horizontal(|ui| {
                        self.draw_grip_handle(ui);
                        ui.add(egui::Separator::default().vertical().spacing(4.0));
                        self.toolbar_mode_buttons(ui);
                        if self.mode == Mode::Draw && self.show_color_picker {
                            ui.add(egui::Separator::default().vertical().spacing(4.0));
                            self.toolbar_color_picker(ui);
                        }
                        ui.add(egui::Separator::default().vertical().spacing(4.0));
                        self.toolbar_action_buttons(ui);
                    });
                });
            });

        self.clamp_toolbar_pos(ctx, inner_resp.response.rect);
        self.toolbar_rect = inner_resp.response.rect;
    }

    /// Paint the 2×3 dot grip and apply any drag delta to `toolbar_pos`.
    fn draw_grip_handle(&mut self, ui: &mut egui::Ui) {
        let (grip_rect, grip_resp) =
            ui.allocate_exact_size(egui::vec2(14.0, 24.0), egui::Sense::drag());

        let grip_color = if grip_resp.hovered() || grip_resp.dragged() {
            egui::Color32::from_rgba_unmultiplied(220, 220, 220, 220)
        } else {
            egui::Color32::from_rgba_unmultiplied(120, 120, 120, 180)
        };

        let c = grip_rect.center();
        for row in 0..3i32 {
            for col in 0..2i32 {
                ui.painter().circle_filled(
                    c + egui::vec2((col as f32 - 0.5) * 5.0, (row as f32 - 1.0) * 6.0),
                    1.5,
                    grip_color,
                );
            }
        }

        if grip_resp.dragged()
            && let Some(p) = self.toolbar_pos.as_mut()
        {
            *p += grip_resp.drag_delta();
        }
        if grip_resp.hovered() || grip_resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }

    /// Laser and Draw toggle buttons.
    fn toolbar_mode_buttons(&mut self, ui: &mut egui::Ui) {
        // ── Laser ──────────────────────────────────────────────────────────
        let laser_active = self.mode == Mode::Laser;
        if ui
            .add(mode_button(
                "🔴  Laser",
                laser_active,
                egui::Color32::from_rgb(160, 35, 35),
            ))
            .clicked()
        {
            self.mode = if laser_active {
                Mode::Idle
            } else {
                Mode::Laser
            };
            if self.mode != Mode::Laser {
                self.trail.clear();
                self.pointer_pos = None;
            }
        }

        // ── Draw ───────────────────────────────────────────────────────────
        let draw_active = self.mode == Mode::Draw;
        if ui
            .add(mode_button(
                "✏  Draw",
                draw_active,
                egui::Color32::from_rgb(25, 90, 170),
            ))
            .clicked()
        {
            self.mode = if draw_active { Mode::Idle } else { Mode::Draw };
            if self.mode == Mode::Draw {
                // Entering draw: clear stale laser state.
                self.trail.clear();
                self.pointer_pos = None;
            } else {
                // Leaving draw: collapse the palette and commit any in-progress stroke.
                self.show_color_picker = false;
                if !self.current_stroke.is_empty() {
                    let s = std::mem::take(&mut self.current_stroke);
                    self.strokes.push((s, self.draw_color_rgb));
                }
            }
        }

        // ── Draw colour toggle ─────────────────────────────────────────────
        if self.mode == Mode::Draw {
            self.toolbar_color_toggle(ui);
        }
    }

    /// Small coloured-circle button that toggles the palette open/closed.
    /// Only rendered when Draw mode is active.
    fn toolbar_color_toggle(&mut self, ui: &mut egui::Ui) {
        let [r, g, b] = self.draw_color_rgb;
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());

        let painter = ui.painter();
        // Button background – brighter when the palette is open.
        let bg = if self.show_color_picker {
            egui::Color32::from_rgba_unmultiplied(90, 90, 90, 230)
        } else {
            egui::Color32::from_rgba_unmultiplied(55, 55, 55, 200)
        };
        painter.rect_filled(rect, egui::CornerRadius::same(6), bg);
        // Current colour disc.
        painter.circle_filled(
            rect.center(),
            8.0,
            egui::Color32::from_rgba_unmultiplied(r, g, b, self.cfg.draw_alpha),
        );
        // White ring when palette is open.
        if self.show_color_picker {
            painter.circle_stroke(
                rect.center(),
                9.0,
                egui::Stroke::new(1.5, egui::Color32::WHITE),
            );
        }

        if resp.clicked() {
            self.show_color_picker = !self.show_color_picker;
        }
        resp.on_hover_text("Pick colour");
    }

    /// Colour palette swatches; only shown while Draw mode is active.
    fn toolbar_color_picker(&mut self, ui: &mut egui::Ui) {
        let alpha = self.cfg.draw_alpha;
        for &([r, g, b], label) in DRAW_PALETTE {
            let is_selected = self.draw_color_rgb == [r, g, b];
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());

            if resp.clicked() {
                self.draw_color_rgb = [r, g, b];
            }

            let painter = ui.painter();
            // Selection ring / hover highlight
            if is_selected {
                painter.circle_filled(rect.center(), 9.5, egui::Color32::WHITE);
            } else if resp.hovered() {
                painter.circle_filled(
                    rect.center(),
                    9.5,
                    egui::Color32::from_rgba_unmultiplied(200, 200, 200, 120),
                );
            }
            // Colour disc
            let swatch_color = egui::Color32::from_rgba_unmultiplied(r, g, b, alpha);
            painter.circle_filled(
                rect.center(),
                if is_selected { 7.0 } else { 8.0 },
                swatch_color,
            );

            resp.on_hover_text(label);
        }
    }

    /// Clear and Close (exit) buttons.
    fn toolbar_action_buttons(&mut self, ui: &mut egui::Ui) {
        // ── Clear ──────────────────────────────────────────────────────────
        let clear_btn = egui::Button::new(
            egui::RichText::new("🗑  Clear")
                .size(13.0)
                .color(egui::Color32::from_rgb(220, 180, 180)),
        )
        .fill(egui::Color32::from_rgba_unmultiplied(55, 55, 55, 200))
        .corner_radius(egui::CornerRadius::same(6));

        if ui.add(clear_btn).clicked() {
            self.strokes.clear();
            self.current_stroke.clear();
        }

        ui.add(egui::Separator::default().vertical().spacing(4.0));

        // ── Close / exit ───────────────────────────────────────────────────
        let close_btn = egui::Button::new(
            egui::RichText::new("❌")
                .size(13.0)
                .color(egui::Color32::from_rgb(200, 140, 140)),
        )
        .fill(egui::Color32::TRANSPARENT)
        .corner_radius(egui::CornerRadius::same(6));

        if ui.add(close_btn).clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Clamp `toolbar_pos` so the widget cannot be dragged fully off-screen.
    fn clamp_toolbar_pos(&mut self, ctx: &egui::Context, tb: egui::Rect) {
        let screen = ctx.screen_rect();
        if let Some(p) = self.toolbar_pos.as_mut() {
            p.x = p.x.clamp(
                screen.left(),
                (screen.right() - tb.width()).max(screen.left()),
            );
            p.y = p.y.clamp(
                screen.top(),
                (screen.bottom() - tb.height()).max(screen.top()),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Toolbar helpers (free functions — no `self` needed)
// ---------------------------------------------------------------------------

/// Dark, rounded panel frame used for the toolbar background.
fn toolbar_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(egui::Color32::from_rgba_unmultiplied(18, 18, 18, 210))
        .corner_radius(egui::CornerRadius::same(10))
        .stroke(egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_unmultiplied(90, 90, 90, 200),
        ))
        .inner_margin(egui::Margin::symmetric(10, 7))
}

/// A mode-toggle button: highlighted with `active_color` when active,
/// neutral otherwise.
fn mode_button(label: &str, active: bool, active_color: egui::Color32) -> egui::Button<'_> {
    let fill = if active {
        active_color
    } else {
        egui::Color32::from_rgba_unmultiplied(55, 55, 55, 200)
    };
    egui::Button::new(
        egui::RichText::new(label)
            .size(13.0)
            .color(egui::Color32::WHITE),
    )
    .fill(fill)
    .corner_radius(egui::CornerRadius::same(6))
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
                self.paint_overlay(ui.painter(), monitor_scale);
            });

        self.show_toolbar(ctx);
    }
}
