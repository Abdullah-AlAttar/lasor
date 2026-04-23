#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager,
    hotkey::{Code, HotKey, Modifiers},
};
use std::collections::VecDeque;
use std::time::Instant;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn main() -> eframe::Result<()> {
    let laser_on = Arc::new(AtomicBool::new(false));
    let laser_on_hotkey = laser_on.clone();

    // GlobalHotKeyManager must live on the main thread (runs its own message pump on Windows)
    let manager = GlobalHotKeyManager::new().expect("failed to create hotkey manager");
    // Ctrl+Shift+L toggles laser pointer on/off
    let hotkey = HotKey::new(Some(Modifiers::CONTROL), Code::Space);
    manager.register(hotkey).expect("failed to register hotkey");

    let hotkey_id = hotkey.id();

    // Spawn thread to process hotkey events
    std::thread::spawn(move || {
        let receiver = GlobalHotKeyEvent::receiver();
        loop {
            if let Ok(event) = receiver.recv() {
                if event.id == hotkey_id {
                    let current = laser_on_hotkey.load(Ordering::Relaxed);
                    laser_on_hotkey.store(!current, Ordering::Relaxed);
                }
            }
        }
    });

    // Compute the bounding rect spanning ALL monitors (virtual desktop).
    #[cfg(windows)]
    let (virt_pos, virt_size) = {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
            SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
        };
        let x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) } as f32;
        let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) } as f32;
        let w = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) } as f32;
        let h = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) } as f32;
        ([x, y], [w, h])
    };
    #[cfg(not(windows))]
    let (virt_pos, virt_size) = ([0.0_f32, 0.0_f32], [3840.0_f32, 2160.0_f32]);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_position(virt_pos)
            .with_inner_size(virt_size)
            .with_mouse_passthrough(true),
        ..Default::default()
    };

    eframe::run_native(
        "lasor",
        native_options,
        Box::new(move |_cc| {
            Ok(Box::new(LasorApp {
                laser_on: laser_on.clone(),
                pointer_pos: None,
                trail: VecDeque::new(),
                _manager: manager,
            }))
        }),
    )
}

// Trail entry: position + time recorded
struct TrailPoint {
    pos: egui::Pos2,
    time: Instant,
}

const TRAIL_DURATION_SECS: f32 = 0.8;
const TRAIL_MAX_RADIUS: f32 = 12.0;

struct LasorApp {
    laser_on: Arc<AtomicBool>,
    pointer_pos: Option<egui::Pos2>,
    trail: VecDeque<TrailPoint>,
    // Keep manager alive so hotkeys remain registered
    _manager: GlobalHotKeyManager,
}

impl eframe::App for LasorApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Fully transparent background
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let on = self.laser_on.load(Ordering::Relaxed);

        // Always passthrough — we never want to consume clicks
        ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(true));

        // Nothing to draw; sleep the repaint rate way down to save CPU
        if !on {
            self.trail.clear();
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |_ui| {});
            return;
        }

        // In passthrough mode egui never receives mouse events; poll OS cursor pos directly.
        #[cfg(windows)]
        let current_pos = {
            use windows_sys::Win32::Foundation::POINT;
            use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
            let mut pt = POINT { x: 0, y: 0 };
            unsafe { GetCursorPos(&mut pt) };
            let ppp = ctx.pixels_per_point();
            egui::Pos2::new(pt.x as f32 / ppp, pt.y as f32 / ppp)
        };
        #[cfg(not(windows))]
        let current_pos = ctx.input(|i| i.pointer.latest_pos()).unwrap_or(egui::Pos2::ZERO);

        self.pointer_pos = Some(current_pos);

        // Push to trail if moved enough
        let should_push = self.trail.back().map_or(true, |last| {
            last.pos.distance(current_pos) > 2.0
        });
        if should_push {
            self.trail.push_back(TrailPoint { pos: current_pos, time: Instant::now() });
        }

        // Drop expired trail points
        let cutoff = TRAIL_DURATION_SECS;
        self.trail.retain(|p| p.time.elapsed().as_secs_f32() < cutoff);

        // Always repaint to stay responsive
        ctx.request_repaint();

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let painter = ui.painter();
                let trail_len = self.trail.len();

                // Draw trail (oldest first, fading + shrinking)
                for (i, tp) in self.trail.iter().enumerate() {
                    let age = tp.time.elapsed().as_secs_f32();
                    let t = 1.0 - (age / TRAIL_DURATION_SECS); // 1.0=fresh, 0.0=expired
                    // Also fade by position in queue for smoother look
                    let pos_t = (i + 1) as f32 / trail_len.max(1) as f32;
                    let blend = (t * pos_t).powf(0.5);
                    let alpha = (blend * 180.0) as u8;
                    let radius = 3.0 + blend * (TRAIL_MAX_RADIUS - 3.0);
                    painter.circle_filled(
                        tp.pos,
                        radius,
                        egui::Color32::from_rgba_unmultiplied(220, 30, 30, alpha),
                    );
                }

                // Draw head circle
                if let Some(pos) = self.pointer_pos {
                    painter.circle_filled(pos, 10.0, egui::Color32::from_rgba_unmultiplied(220, 30, 30, 220));
                    painter.circle_stroke(pos, 10.0, egui::Stroke::new(2.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 180)));
                }
            });
    }
}

