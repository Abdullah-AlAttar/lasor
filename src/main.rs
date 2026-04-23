#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager,
    hotkey::{Code, HotKey, Modifiers},
    HotKeyState,
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
                // Only toggle on key-press, not key-release.
                if event.id == hotkey_id && event.state == HotKeyState::Pressed {
                    let current = laser_on_hotkey.load(Ordering::Relaxed);
                    laser_on_hotkey.store(!current, Ordering::Relaxed);
                }
            }
        }
    });

    // On Windows: compute virtual desktop bounding rect in raw physical pixels.
    // SetWindowPos will be called on first frame to span all monitors exactly.
    // On other platforms: use winit fullscreen (handles multi-monitor natively).
    #[cfg(windows)]
    let (virt_x, virt_y, virt_w, virt_h) = {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics,
            SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
            SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
        };
        let x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let w = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let h = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
        (x, y, w, h)
    };
    #[cfg(not(windows))]
    let (virt_x, virt_y, virt_w, virt_h) = (0i32, 0i32, 0i32, 0i32);

    let native_options = eframe::NativeOptions {
        viewport: {
            let vp = egui::ViewportBuilder::default()
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                .with_mouse_passthrough(true);
            // Non-Windows: use winit fullscreen which handles multi-monitor correctly.
            #[cfg(not(windows))]
            let vp = vp.with_fullscreen(true);
            vp
        },
        ..Default::default()
    };

    eframe::run_native(
        "lasor",
        native_options,
        Box::new(move |_cc| {
            Ok(Box::new(LasorApp {
                laser_on: laser_on.clone(),
                virt_x,
                virt_y,
                virt_w,
                virt_h,
                positioned: false,
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

const TRAIL_DURATION_SECS: f32 = 1.8;
const TRAIL_MAX_RADIUS: f32 = 12.0;

struct LasorApp {
    laser_on: Arc<AtomicBool>,
    /// Physical coords of the virtual screen bounding box (all monitors combined).
    virt_x: i32,
    virt_y: i32,
    virt_w: i32,
    virt_h: i32,
    /// Whether we've already called SetWindowPos to span all monitors.
    positioned: bool,
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
        // On first frame: use SetWindowPos with raw physical pixel coords to span all monitors.
        // This bypasses eframe/winit DPI conversion which is unreliable across mixed-DPI monitors.
        if !self.positioned {
            self.positioned = true;
            #[cfg(windows)]
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    FindWindowW, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW,
                };
                let title: Vec<u16> = "lasor\0".encode_utf16().collect();
                let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
                if hwnd != std::ptr::null_mut() {
                    SetWindowPos(hwnd, HWND_TOPMOST, self.virt_x, self.virt_y, self.virt_w, self.virt_h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
                }
            }
        }

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
        // GetCursorPos returns absolute physical coords; subtract the virtual screen origin
        // and divide by ppp to get coords relative to our window's top-left.
        // Also fetch the DPI scale of the monitor under the cursor so the dot stays
        // the same visual size regardless of per-monitor scaling settings.
        #[cfg(windows)]
        let (current_pos, monitor_scale) = {
            use windows_sys::Win32::Foundation::POINT;
            use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
            use windows_sys::Win32::Graphics::Gdi::MonitorFromPoint;
            use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
            let mut pt = POINT { x: 0, y: 0 };
            unsafe { GetCursorPos(&mut pt) };
            let ppp = ctx.pixels_per_point();
            let pos = egui::Pos2::new((pt.x - self.virt_x) as f32 / ppp, (pt.y - self.virt_y) as f32 / ppp);
            // Per-monitor DPI: keeps physical dot size consistent across monitors.
            let scale = unsafe {
                let monitor = MonitorFromPoint(pt, 2); // MONITOR_DEFAULTTONEAREST
                let mut dx = 96u32;
                let mut dy = 96u32;
                GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
                (dx as f32 / 96.0) / ppp
            };
            (pos, scale)
        };
        #[cfg(not(windows))]
        let (current_pos, monitor_scale) = (
            ctx.input(|i| i.pointer.latest_pos()).unwrap_or(egui::Pos2::ZERO),
            1.0_f32,
        );

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
                    let t = 1.0 - (age / TRAIL_DURATION_SECS);
                    let pos_t = (i + 1) as f32 / trail_len.max(1) as f32;
                    let blend = (t * pos_t).powf(0.5);
                    let alpha = (blend * 180.0) as u8;
                    let radius = (3.0 + blend * (TRAIL_MAX_RADIUS - 3.0)) * monitor_scale;
                    painter.circle_filled(
                        tp.pos,
                        radius,
                        egui::Color32::from_rgba_unmultiplied(220, 30, 30, alpha),
                    );
                }

                // Draw head circle
                if let Some(pos) = self.pointer_pos {
                    let r = 10.0 * monitor_scale;
                    painter.circle_filled(pos, r, egui::Color32::from_rgba_unmultiplied(220, 30, 30, 220));
                    painter.circle_stroke(pos, r, egui::Stroke::new(2.0 * monitor_scale, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 180)));
                }
            });
    }
}

