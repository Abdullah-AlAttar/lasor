use eframe::egui;

use super::LasorApp;
use crate::mode::Mode;
use crate::platform;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Padding from the screen edge when auto-positioning the toolbar (logical px).
const TOOLBAR_EDGE_MARGIN: f32 = 10.0;

/// Preset colour swatches shown in the palette popup while Draw mode is active.
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
// Toolbar impl
// ---------------------------------------------------------------------------

impl LasorApp {
    /// Render the draggable toolbar area and update `toolbar_rect`.
    pub(super) fn show_toolbar(&mut self, ctx: &egui::Context) {
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
                egui::pos2(br.x - 350.0, br.y - 100.0)
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
                        ui.add(egui::Separator::default().vertical().spacing(4.0));
                        self.toolbar_action_buttons(ui);
                    });
                });
            });

        self.clamp_toolbar_pos(ctx, inner_resp.response.rect);
        self.toolbar_rect = inner_resp.response.rect;
    }

    /// Floating vertical colour palette that pops up above the toolbar.
    /// Collapses automatically when a colour is picked.
    pub(super) fn show_color_palette_popup(&mut self, ctx: &egui::Context) {
        if !(self.mode == Mode::Draw && self.show_color_picker) {
            return;
        }

        // Anchor the popup's bottom-left corner just above the toggle button.
        let anchor = egui::pos2(self.color_toggle_rect.min.x, self.toolbar_rect.min.y - 6.0);
        let alpha = self.cfg.draw_alpha;

        egui::Area::new(egui::Id::new("color_palette"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::LEFT_BOTTOM)
            .fixed_pos(anchor)
            .interactable(true)
            .show(ctx, |ui| {
                toolbar_frame().show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 4.0);
                    ui.vertical(|ui| {
                        for &([r, g, b], label) in DRAW_PALETTE {
                            let is_selected = self.draw_color_rgb == [r, g, b];
                            let (rect, resp) = ui
                                .allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());

                            if resp.clicked() {
                                self.draw_color_rgb = [r, g, b];
                                self.show_color_picker = false;
                            }

                            let painter = ui.painter();
                            if is_selected {
                                painter.circle_filled(rect.center(), 9.5, egui::Color32::WHITE);
                            } else if resp.hovered() {
                                painter.circle_filled(
                                    rect.center(),
                                    9.5,
                                    egui::Color32::from_rgba_unmultiplied(200, 200, 200, 120),
                                );
                            }
                            let swatch = egui::Color32::from_rgba_unmultiplied(r, g, b, alpha);
                            painter.circle_filled(
                                rect.center(),
                                if is_selected { 7.0 } else { 8.0 },
                                swatch,
                            );

                            resp.on_hover_text(label);
                        }
                    });
                });
            });
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

    /// Laser and Draw mode toggle buttons, plus the colour indicator when in Draw mode.
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

        // ── Colour toggle (only while Draw is active) ──────────────────────
        if self.mode == Mode::Draw {
            self.toolbar_color_toggle(ui);
        }
    }

    /// Small coloured-circle button that toggles the palette popup open/closed.
    fn toolbar_color_toggle(&mut self, ui: &mut egui::Ui) {
        let [r, g, b] = self.draw_color_rgb;
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
        self.color_toggle_rect = rect;

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
// Free helpers — no `self` needed
// ---------------------------------------------------------------------------

/// Dark, rounded panel frame used for the toolbar and palette popup backgrounds.
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
