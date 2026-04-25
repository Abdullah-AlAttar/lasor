use eframe::egui;

use super::LasorApp;

impl LasorApp {
    pub(super) fn update_draw(&mut self, ctx: &egui::Context) {
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

    /// Paint all completed and in-progress annotation strokes.
    /// Each completed stroke carries its own colour captured at draw time.
    pub(super) fn paint_strokes(&self, painter: &egui::Painter, monitor_scale: f32) {
        for (stroke, color_rgb) in &self.strokes {
            let [dr, dg, db] = *color_rgb;
            let color = egui::Color32::from_rgba_unmultiplied(dr, dg, db, self.cfg.draw_alpha);
            let pen = egui::Stroke::new(self.cfg.draw_width * monitor_scale, color);
            Self::paint_stroke(painter, stroke, pen);
        }

        if !self.current_stroke.is_empty() {
            let [dr, dg, db] = self.draw_color_rgb;
            let color = egui::Color32::from_rgba_unmultiplied(dr, dg, db, self.cfg.draw_alpha);
            let pen = egui::Stroke::new(self.cfg.draw_width * monitor_scale, color);
            Self::paint_stroke(painter, &self.current_stroke, pen);
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
}
