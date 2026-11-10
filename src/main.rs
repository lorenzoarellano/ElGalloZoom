#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod config;
mod hook;
mod magnifier;
mod overlay;
mod tray;
mod ui;

use crate::config::{AppConfig, SharedState};
use crate::hook::spawn_hook_thread;
use crate::magnifier::{cleanup_magnification, spawn_zoom_thread};
use crate::tray::load_rgba_icon;
use crate::ui::EgzApp;
use eframe::egui;
use std::sync::{Arc, Mutex};

fn install_safety_hooks() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        cleanup_magnification();
        default_hook(panic_info);
    }));
}

fn enable_per_monitor_dpi() {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::UI::HiDpi::{
            SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

fn activate_existing_instance() -> bool {
    #[cfg(target_os = "windows")]
    unsafe {
        use crate::ui::apply_toolwindow_style;
        use windows::core::w;
        use windows::Win32::Graphics::Gdi::{
            RedrawWindow, RDW_INTERNALPAINT, RDW_INVALIDATE, RDW_UPDATENOW,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
        };

        if let Ok(hwnd) = FindWindowW(None, w!("ELGalloZoom (EGZ) - Settings")) {
            if !hwnd.0.is_null() {
                apply_toolwindow_style(hwnd);
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
                let _ = RedrawWindow(
                    hwnd,
                    None,
                    None,
                    RDW_INVALIDATE | RDW_UPDATENOW | RDW_INTERNALPAINT,
                );
                return true;
            }
        }
    }
    false
}

fn main() -> eframe::Result<()> {
    enable_per_monitor_dpi();

    if activate_existing_instance() {
        return Ok(());
    }

    install_safety_hooks();

    let cfg = AppConfig::load();
    let shared_state = Arc::new(Mutex::new(SharedState::new(cfg)));

    // Spawn dedicated Low-Level Hook thread (Win32 WH_MOUSE_LL or macOS CGEventTap)
    spawn_hook_thread(Arc::clone(&shared_state));

    // Spawn high-frequency (~120 FPS / 8ms) Magnification & Spotlight interpolation thread
    spawn_zoom_thread(Arc::clone(&shared_state));

    let (rgba, w, h) = load_rgba_icon();
    let icon_data = egui::IconData {
        rgba,
        width: w,
        height: h,
    };

    let viewport = egui::ViewportBuilder::default()
        .with_title("ELGalloZoom (EGZ) - Settings")
        .with_inner_size([460.0, 380.0])
        .with_min_inner_size([420.0, 350.0])
        .with_resizable(false)
        .with_maximize_button(false)
        .with_minimize_button(false)
        .with_taskbar(false)
        .with_icon(Arc::new(icon_data));

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let state_for_ui = Arc::clone(&shared_state);
    let result = eframe::run_native(
        "ELGalloZoom (EGZ) - Settings",
        native_options,
        Box::new(move |cc| Ok(Box::new(EgzApp::new(cc, state_for_ui)))),
    );

    cleanup_magnification();
    result
}
