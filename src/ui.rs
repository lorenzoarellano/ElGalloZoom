use crate::config::{SharedState, TriggerMode};
use crate::magnifier::cleanup_magnification;
use crate::tray::{build_tray, load_rgba_icon, TrayControls};
use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicIsize, Ordering};
#[cfg(target_os = "windows")]
use windows::core::w;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    RedrawWindow, RDW_INTERNALPAINT, RDW_INVALIDATE, RDW_UPDATENOW,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, FindWindowW, GetWindowLongW, IsWindowVisible, SetForegroundWindow,
    SetWindowLongW, ShowWindow, GWL_EXSTYLE, SW_HIDE, SW_RESTORE, SW_SHOW, WS_EX_APPWINDOW,
    WS_EX_TOOLWINDOW,
};

#[cfg(target_os = "windows")]
static SETTINGS_HWND: AtomicIsize = AtomicIsize::new(0);

#[cfg(target_os = "windows")]
pub fn resolve_settings_hwnd() -> Option<HWND> {
    let cached = SETTINGS_HWND.load(Ordering::SeqCst);
    if cached != 0 {
        return Some(HWND(cached as *mut _));
    }
    unsafe {
        if let Ok(hwnd) = FindWindowW(None, w!("ELGalloZoom (EGZ) - Settings")) {
            if !hwnd.0.is_null() {
                SETTINGS_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
                apply_toolwindow_style(hwnd);
                return Some(hwnd);
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
pub unsafe fn apply_toolwindow_style(hwnd: HWND) {
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
    let new_style = (ex_style & !(WS_EX_APPWINDOW.0 as i32)) | (WS_EX_TOOLWINDOW.0 as i32);
    if ex_style != new_style {
        SetWindowLongW(hwnd, GWL_EXSTYLE, new_style);
    }
}

#[cfg(target_os = "macos")]
pub fn configure_macos_accessory_policy() {
    use objc::runtime::{Class, Object, YES};
    use objc::{msg_send, sel, sel_impl};
    unsafe {
        if let Some(app_cls) = Class::get("NSApplication") {
            let ns_app: *mut Object = msg_send![app_cls, sharedApplication];
            if !ns_app.is_null() {
                // NSApplicationActivationPolicyAccessory = 1 (Menu Bar only, no Dock icon)
                let _: bool = msg_send![ns_app, setActivationPolicy: 1isize];
                let _: () = msg_send![ns_app, activateIgnoringOtherApps: YES];
            }
        }
    }
}

pub fn show_settings_window(state: &Arc<Mutex<SharedState>>, ctx: &egui::Context) {
    if let Ok(mut st) = state.lock() {
        st.show_settings = true;
    }

    #[cfg(target_os = "windows")]
    if let Some(hwnd) = resolve_settings_hwnd() {
        unsafe {
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
        }
    }

    #[cfg(target_os = "macos")]
    {
        use objc::runtime::{Class, Object, YES};
        use objc::{msg_send, sel, sel_impl};
        unsafe {
            if let Some(app_cls) = Class::get("NSApplication") {
                let ns_app: *mut Object = msg_send![app_cls, sharedApplication];
                if !ns_app.is_null() {
                    let _: () = msg_send![ns_app, activateIgnoringOtherApps: YES];
                }
            }
        }
    }

    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}

pub fn hide_settings_window(state: &Arc<Mutex<SharedState>>, ctx: &egui::Context) {
    if let Ok(mut st) = state.lock() {
        st.show_settings = false;
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));

    #[cfg(target_os = "windows")]
    if let Some(hwnd) = resolve_settings_hwnd() {
        unsafe {
            apply_toolwindow_style(hwnd);
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

pub fn store_tray_hwnd_and_remove() {
    #[cfg(target_os = "windows")]
    unsafe {
        if let Ok(tray_hwnd) = FindWindowW(w!("tray_icon_app"), None) {
            if !tray_hwnd.0.is_null() {
                let _ = DestroyWindow(tray_hwnd);
            }
        }
    }
}

pub struct EgzApp {
    state: Arc<Mutex<SharedState>>,
    tray: Option<TrayControls>,
    logo_texture: Option<egui::TextureHandle>,
    startup_frame: u8,
}

impl EgzApp {
    pub fn new(cc: &eframe::CreationContext<'_>, state: Arc<Mutex<SharedState>>) -> Self {
        setup_visuals(&cc.egui_ctx);

        #[cfg(target_os = "windows")]
        let _ = resolve_settings_hwnd();

        #[cfg(target_os = "macos")]
        configure_macos_accessory_policy();

        let (rgba, w, h) = load_rgba_icon();
        let color_image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
        let logo_texture = Some(cc.egui_ctx.load_texture(
            "egz_logo",
            color_image,
            egui::TextureOptions::LINEAR,
        ));

        let tray = build_tray(Arc::clone(&state), cc.egui_ctx.clone());
        if tray.is_none() {
            if let Ok(mut st) = state.lock() {
                st.show_settings = true;
            }
        }

        Self {
            state,
            tray,
            logo_texture,
            startup_frame: 0,
        }
    }
}

fn setup_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_rounding = egui::Rounding::same(10.0);
    visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(24, 22, 30);
    visuals.panel_fill = egui::Color32::from_rgb(24, 22, 30);
    ctx.set_visuals(visuals);
}

impl eframe::App for EgzApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "windows")]
        if let Some(hwnd) = resolve_settings_hwnd() {
            unsafe {
                apply_toolwindow_style(hwnd);
            }
        }

        // Frame 0 & 1: let eframe complete initial post_rendering, then hide into System Tray / Menu Bar
        if self.startup_frame == 0 {
            self.startup_frame = 1;
            let show_now = self
                .state
                .lock()
                .map(|s| s.show_settings)
                .unwrap_or(false);
            if !show_now && self.tray.is_some() {
                hide_settings_window(&self.state, ctx);
            }
            ctx.request_repaint();
        } else if self.startup_frame == 1 {
            self.startup_frame = 2;
            let show_now = self
                .state
                .lock()
                .map(|s| s.show_settings)
                .unwrap_or(false);
            if !show_now && self.tray.is_some() {
                hide_settings_window(&self.state, ctx);
            }
        } else {
            #[cfg(target_os = "windows")]
            {
                let win32_visible = resolve_settings_hwnd()
                    .map(|hwnd| unsafe { IsWindowVisible(hwnd).as_bool() })
                    .unwrap_or(false);

                if let Ok(mut st) = self.state.lock() {
                    if win32_visible && !st.show_settings {
                        st.show_settings = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    } else if st.show_settings && !win32_visible {
                        if let Some(hwnd) = resolve_settings_hwnd() {
                            unsafe {
                                apply_toolwindow_style(hwnd);
                                let _ = ShowWindow(hwnd, SW_SHOW);
                                let _ = ShowWindow(hwnd, SW_RESTORE);
                                let _ = SetForegroundWindow(hwnd);
                            }
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    }
                }
            }
        }

        // Intercept "X" close button to hide to System Tray / Menu Bar instead of terminating process
        if ctx.input(|i| i.viewport().close_requested()) {
            let should_quit = self
                .state
                .lock()
                .map(|s| s.should_quit)
                .unwrap_or(false);
            if !should_quit && self.tray.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                hide_settings_window(&self.state, ctx);
            } else {
                store_tray_hwnd_and_remove();
                cleanup_magnification();
            }
        }

        let is_visible = self
            .state
            .lock()
            .map(|s| s.show_settings)
            .unwrap_or(true);

        if is_visible {
            ctx.request_repaint_after(Duration::from_millis(50));
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(tex) = &self.logo_texture {
                    ui.add(
                        egui::Image::new(tex)
                            .fit_to_exact_size(egui::vec2(44.0, 44.0))
                            .rounding(8.0),
                    );
                }
                ui.vertical(|ui| {
                    ui.heading(
                        egui::RichText::new("ELGalloZoom (EGZ)")
                            .strong()
                            .color(egui::Color32::from_rgb(245, 145, 65)),
                    );
                    ui.label(
                        egui::RichText::new("Smooth Desktop Magnifier & Spotlight (Windows & macOS)")
                            .small()
                            .weak(),
                    );
                });
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            let mut config_changed = false;
            let mut request_hide = false;

            if let Ok(mut st) = self.state.lock() {
                if let Some(tray) = &self.tray {
                    if tray.toggle_item.is_checked() != st.zoom_enabled {
                        tray.toggle_item.set_checked(st.zoom_enabled);
                    }
                }

                ui.horizontal(|ui| {
                    let status_color = if st.zoom_enabled {
                        egui::Color32::from_rgb(80, 210, 120)
                    } else {
                        egui::Color32::from_rgb(220, 90, 90)
                    };
                    if ui
                        .checkbox(&mut st.zoom_enabled, "")
                        .on_hover_text("Activar / Pausar el motor de zoom")
                        .changed()
                    {
                        if !st.zoom_enabled {
                            st.target_zoom = 1.0;
                            st.f24_toggled = false;
                        }
                        if let Some(tray) = &self.tray {
                            tray.toggle_item.set_checked(st.zoom_enabled);
                        }
                    }
                    ui.colored_label(
                        status_color,
                        if st.zoom_enabled {
                            "● Motor de Zoom Activo"
                        } else {
                            "● Motor de Zoom Pausado"
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("Zoom actual: {:.2}x", st.current_zoom))
                                .monospace(),
                        );
                    });
                });

                ui.add_space(8.0);

                egui::Grid::new("egz_settings_grid")
                    .num_columns(2)
                    .spacing([16.0, 10.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("Paso de Zoom (Step):");
                        if ui
                            .add(
                                egui::Slider::new(&mut st.config.zoom_step, 0.2..=2.0)
                                    .suffix("x")
                                    .step_by(0.05),
                            )
                            .changed()
                        {
                            config_changed = true;
                        }
                        ui.end_row();

                        ui.label("Zoom Máximo:");
                        if ui
                            .add(
                                egui::Slider::new(&mut st.config.max_zoom, 1.5..=10.0)
                                    .suffix("x")
                                    .step_by(0.1),
                            )
                            .changed()
                        {
                            config_changed = true;
                        }
                        ui.end_row();

                        ui.label("Suavidad (Smoothness):");
                        if ui
                            .add(
                                egui::Slider::new(&mut st.config.smoothness, 0.05..=0.40)
                                    .step_by(0.01),
                            )
                            .on_hover_text("0.05 = Cinematográfico / 0.40 = Instantáneo")
                            .changed()
                        {
                            config_changed = true;
                        }
                        ui.end_row();

                        ui.label("Atajo / Disparador:");
                        egui::ComboBox::from_id_salt("trigger_mode_combo")
                            .selected_text(st.config.trigger_mode.label())
                            .width(240.0)
                            .show_ui(ui, |ui| {
                                let modes = [
                                    TriggerMode::CtrlAltWheel,
                                    TriggerMode::AltWheel,
                                    TriggerMode::Mouse4Wheel,
                                    TriggerMode::Mouse5Wheel,
                                    TriggerMode::F24ToggleAndWheel,
                                ];
                                for mode in modes {
                                    if ui
                                        .selectable_value(
                                            &mut st.config.trigger_mode,
                                            mode,
                                            mode.label(),
                                        )
                                        .changed()
                                    {
                                        config_changed = true;
                                    }
                                }
                            });
                        ui.end_row();
                    });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);

                if ui
                    .checkbox(
                        &mut st.config.spotlight_enabled,
                        "Activar Sombreado / Viñeta (Spotlight Overlay)",
                    )
                    .changed()
                {
                    config_changed = true;
                }

                ui.add_enabled_ui(st.config.spotlight_enabled, |ui| {
                    egui::Grid::new("egz_spotlight_grid")
                        .num_columns(2)
                        .spacing([16.0, 10.0])
                        .show(ui, |ui| {
                            ui.label("Opacidad del Sombreado:");
                            let mut pct = (st.config.spotlight_opacity * 100.0).round();
                            if ui
                                .add(egui::Slider::new(&mut pct, 0.0..=85.0).suffix("%"))
                                .changed()
                            {
                                st.config.spotlight_opacity = (pct / 100.0).clamp(0.0, 0.85);
                                config_changed = true;
                            }
                            ui.end_row();

                            ui.label("Radio del Foco (px):");
                            if ui
                                .add(
                                    egui::Slider::new(&mut st.config.spotlight_radius, 80.0..=600.0)
                                        .suffix(" px"),
                                )
                                .changed()
                            {
                                config_changed = true;
                            }
                            ui.end_row();
                        });
                });

                if config_changed {
                    st.config.save();
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    if ui.button("Restablecer Zoom (1.0x)").clicked() {
                        st.target_zoom = 1.0;
                        st.f24_toggled = false;
                    }

                    if self.tray.is_some() && ui.button("Ocultar en Bandeja").clicked() {
                        request_hide = true;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Salir de EGZ")
                                        .color(egui::Color32::from_rgb(255, 110, 110)),
                                ),
                            )
                            .clicked()
                        {
                            st.should_quit = true;
                            store_tray_hwnd_and_remove();
                            cleanup_magnification();
                            std::process::exit(0);
                        }
                    });
                });
            }

            if request_hide {
                hide_settings_window(&self.state, ctx);
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        store_tray_hwnd_and_remove();
        cleanup_magnification();
    }
}
