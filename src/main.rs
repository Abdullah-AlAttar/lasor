#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![warn(clippy::all)]

mod app;
mod config;
mod mode;
mod platform;

use app::LasorApp;
use config::load_config;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let cfg = load_config();

    // On Windows: compute the virtual desktop bounding rect in raw physical pixels.
    // SetWindowPos (called on the first frame inside LasorApp) spans all monitors exactly,
    // bypassing eframe/winit DPI conversion which is unreliable across mixed-DPI setups.
    // On other platforms: winit fullscreen handles multi-monitor natively.
    #[cfg(windows)]
    let (virt_x, virt_y, virt_w, virt_h) = {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
            SM_YVIRTUALSCREEN,
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
        Box::new(move |_cc| Ok(Box::new(LasorApp::new(cfg, virt_x, virt_y, virt_w, virt_h)))),
    )
}
