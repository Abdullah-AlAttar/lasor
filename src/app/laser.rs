use std::time::Instant;

use eframe::egui;

use super::{LasorApp, TrailPoint};

impl LasorApp {
    pub(super) fn update_laser(&mut self, cursor: egui::Pos2) {
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

    /// Draws the fading trail and the dot head; only called in `Laser` mode.
    pub(super) fn paint_laser(&self, painter: &egui::Painter, monitor_scale: f32) {
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
        // points. This fills any remaining sub-pixel gaps between the circles
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
}
