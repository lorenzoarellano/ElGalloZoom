use crate::config::SharedState;
use crate::magnifier::cleanup_magnification;
use crate::ui::{show_settings_window, store_tray_hwnd_and_remove};
use eframe::egui;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

const EMBEDDED_ICO: &[u8] = include_bytes!("../assets/icon.ico");
const EMBEDDED_PNG: &[u8] = include_bytes!("../assets/logo.png");

pub struct TrayControls {
    pub _tray_icon: TrayIcon,
    pub toggle_item: CheckMenuItem,
}

pub fn load_rgba_icon() -> (Vec<u8>, u32, u32) {
    // 1. Try loading from runtime disk `assets/icon.ico`
    if Path::new("assets/icon.ico").exists() {
        if let Ok(img) = image::open("assets/icon.ico") {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            return (rgba.into_raw(), w, h);
        }
    }

    // 2. Try loading from embedded bytes
    if let Ok(img) = image::load_from_memory(EMBEDDED_ICO) {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        return (rgba.into_raw(), w, h);
    }

    if let Ok(img) = image::load_from_memory(EMBEDDED_PNG) {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        return (rgba.into_raw(), w, h);
    }

    // 3. Programmatic fallback RGBA icon in memory (64x64 warm rooster crest gradient)
    let w = 64u32;
    let h = 64u32;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - 32.0;
            let dy = y as f32 - 32.0;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= 28.0 {
                rgba.push(230); // R
                rgba.push((80 + (y * 2)).min(220) as u8); // G
                rgba.push(45); // B
                rgba.push(255); // A
            } else {
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
            }
        }
    }
    (rgba, w, h)
}

pub fn build_tray(state: Arc<Mutex<SharedState>>, egui_ctx: egui::Context) -> Option<TrayControls> {
    let (rgba, w, h) = load_rgba_icon();
    let icon = Icon::from_rgba(rgba, w, h).ok()?;

    let menu = Menu::new();
    let settings_item = MenuItem::new("Configuración...", true, None);
    let toggle_item = CheckMenuItem::new("Activar / Pausar Zoom", true, true, None);
    let separator = PredefinedMenuItem::separator();
    let quit_item = MenuItem::new("Salir", true, None);

    let settings_id = settings_item.id().clone();
    let toggle_id = toggle_item.id().clone();
    let quit_id = quit_item.id().clone();

    let _ = menu.append(&settings_item);
    let _ = menu.append(&toggle_item);
    let _ = menu.append(&separator);
    let _ = menu.append(&quit_item);

    // Register synchronous Win32 event handlers BEFORE building TrayIcon so events are processed
    // immediately inside the Win32 message loop even when the egui window is hidden (SW_HIDE).
    let state_menu = Arc::clone(&state);
    let ctx_menu = egui_ctx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == settings_id {
            show_settings_window(&state_menu, &ctx_menu);
        } else if event.id == toggle_id {
            if let Ok(mut st) = state_menu.lock() {
                st.zoom_enabled = !st.zoom_enabled;
                if !st.zoom_enabled {
                    st.target_zoom = 1.0;
                    st.f24_toggled = false;
                }
            }
            ctx_menu.request_repaint();
        } else if event.id == quit_id {
            if let Ok(mut st) = state_menu.lock() {
                st.should_quit = true;
            }
            store_tray_hwnd_and_remove();
            cleanup_magnification();
            std::process::exit(0);
        }
    }));

    let state_tray = Arc::clone(&state);
    let ctx_tray = egui_ctx;
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => {
                show_settings_window(&state_tray, &ctx_tray);
            }
            _ => {}
        }
    }));

    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("ELGalloZoom (EGZ) - Clic para Configuración")
        .with_icon(icon)
        .build()
        .ok()?;

    Some(TrayControls {
        _tray_icon: tray_icon,
        toggle_item,
    })
}
