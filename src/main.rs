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

    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut native_options = eframe::NativeOptions {
        viewport: {
            let vp = egui::ViewportBuilder::default()
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                // On Linux, mouse passthrough is managed via X11 input shape
                // (platform::update_input_shape called each frame).  Setting
                // with_mouse_passthrough here would conflict: winit re-applies
                // the passthrough state after every update() call and would
                // override our per-frame shape_rectangles calls.
                // On Windows, we use with_mouse_passthrough + send_viewport_cmd.
                .with_mouse_passthrough(cfg!(windows));
            // Non-Windows: use winit fullscreen which handles multi-monitor correctly.
            #[cfg(not(windows))]
            let vp = vp.with_fullscreen(true);
            vp
        },
        ..Default::default()
    };

    // On Linux:
    // 1. Use the glow (OpenGL/glutin) renderer instead of wgpu.  wgpu's GL
    //    surface path fails on many X11 setups with "Invalid surface"; the
    //    glow renderer uses glutin directly and is far more reliable on Linux.
    // 2. Force X11 when DISPLAY is available so we don't accidentally try
    //    Wayland inside a nix-shell that lacks libwayland-client.so.
    #[cfg(target_os = "linux")]
    {
        native_options.renderer = eframe::Renderer::Glow;
        if std::env::var_os("DISPLAY").is_some() {
            use winit::platform::x11::EventLoopBuilderExtX11;
            native_options.event_loop_builder = Some(Box::new(|builder| {
                builder.with_x11();
            }));
        }
    }

    eframe::run_native(
        "lasor",
        native_options,
        Box::new(move |_cc| Ok(Box::new(LasorApp::new(cfg, virt_x, virt_y, virt_w, virt_h)))),
    )
}
