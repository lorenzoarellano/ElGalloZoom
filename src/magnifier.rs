use crate::config::SharedState;
use crate::overlay::SpotlightOverlay;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

static MAG_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn ensure_mag_initialized() {
    if !MAG_INITIALIZED.swap(true, Ordering::SeqCst) {
        platform_mag::init();
    }
}

pub fn cleanup_magnification() {
    if MAG_INITIALIZED.swap(false, Ordering::SeqCst) {
        platform_mag::cleanup(true);
    } else {
        platform_mag::cleanup(false);
    }
}

pub fn spawn_zoom_thread(state: Arc<Mutex<SharedState>>) {
    thread::spawn(move || {
        ensure_mag_initialized();
        let mut overlay = SpotlightOverlay::new();
        let mut last_applied_zoom: f32 = 1.0;

        loop {
            let (
                should_quit,
                target_zoom,
                mut current_zoom,
                smoothness,
                max_zoom,
                spotlight_enabled,
                spotlight_opacity,
                spotlight_radius,
                zoom_enabled,
            ) = {
                match state.lock() {
                    Ok(st) => (
                        st.should_quit,
                        st.target_zoom,
                        st.current_zoom,
                        st.config.smoothness,
                        st.config.max_zoom,
                        st.config.spotlight_enabled,
                        st.config.spotlight_opacity,
                        st.config.spotlight_radius,
                        st.zoom_enabled,
                    ),
                    Err(_) => break,
                }
            };

            if should_quit {
                overlay.hide();
                cleanup_magnification();
                break;
            }

            let effective_target = if zoom_enabled {
                target_zoom.clamp(1.0, max_zoom)
            } else {
                1.0
            };

            // Linear interpolation (lerp) at ~120 FPS (8ms tick)
            let diff = effective_target - current_zoom;
            if diff.abs() > 0.0008 {
                current_zoom += diff * smoothness.clamp(0.02, 0.85);
            } else {
                current_zoom = effective_target;
            }

            if let Ok(mut st) = state.lock() {
                st.current_zoom = current_zoom;
            }

            let (cx, cy) = platform_mag::get_cursor_pos();

            if current_zoom > 1.001 {
                let offset_x = cx * (1.0 - (1.0 / current_zoom));
                let offset_y = cy * (1.0 - (1.0 / current_zoom));

                platform_mag::apply_transform(current_zoom, offset_x, offset_y);
                last_applied_zoom = current_zoom;
            } else if last_applied_zoom > 1.001 {
                platform_mag::apply_transform(1.0, 0.0, 0.0);
                last_applied_zoom = 1.0;
            }

            // Update Spotlight / Vignette overlay when zoom > 1.01
            if spotlight_enabled && current_zoom > 1.01 {
                let zoom_progress = ((current_zoom - 1.0) / 0.75).clamp(0.0, 1.0);
                let effective_opacity = (spotlight_opacity * zoom_progress).clamp(0.0, 0.85);
                overlay.update(cx as i32, cy as i32, effective_opacity, spotlight_radius);
            } else {
                overlay.hide();
            }

            thread::sleep(Duration::from_millis(8));
        }
    });
}

// ============================================================================
// WINDOWS ADAPTER (Win32 Magnification API)
// ============================================================================
#[cfg(target_os = "windows")]
mod platform_mag {
    use windows::core::{s, w};
    use windows::Win32::Foundation::{BOOL, POINT, TRUE};
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::Win32::UI::Magnification::{
        MagInitialize, MagSetFullscreenTransform, MagUninitialize,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    type MagSetFullscreenUseBitmapSmoothingFn = unsafe extern "system" fn(BOOL) -> BOOL;

    unsafe fn enable_fullscreen_bitmap_smoothing() {
        if let Ok(hmod) = LoadLibraryW(w!("Magnification.dll")) {
            if let Some(proc) = GetProcAddress(hmod, s!("MagSetFullscreenUseBitmapSmoothing")) {
                let func: MagSetFullscreenUseBitmapSmoothingFn = std::mem::transmute(proc);
                let _ = func(TRUE);
            }
        }
    }

    pub fn init() {
        unsafe {
            let _ = MagInitialize();
            enable_fullscreen_bitmap_smoothing();
        }
    }

    pub fn cleanup(was_initialized: bool) {
        unsafe {
            if !was_initialized {
                let _ = MagInitialize();
            }
            let _ = MagSetFullscreenTransform(1.0, 0, 0);
            let _ = MagUninitialize();
        }
    }

    pub fn get_cursor_pos() -> (f32, f32) {
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
        }
        (pt.x as f32, pt.y as f32)
    }

    pub fn apply_transform(zoom: f32, offset_x: f32, offset_y: f32) {
        unsafe {
            let _ = MagSetFullscreenTransform(zoom, offset_x.round() as i32, offset_y.round() as i32);
        }
    }
}

// ============================================================================
// MACOS ADAPTER (CoreGraphics / SkyLight Display Transform)
// ============================================================================
#[cfg(target_os = "macos")]
mod platform_mag {
    use std::ffi::{c_char, c_void};
    use std::ptr::null;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    type CGSConnectionID = u32;
    type CGSMainConnectionIDFn = unsafe extern "C" fn() -> CGSConnectionID;
    type CGSSetZoomParametersFn =
        unsafe extern "C" fn(cid: CGSConnectionID, origin: *const CGPoint, factor: f32) -> i32;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreate(source: *const c_void) -> *mut c_void;
        fn CGEventGetLocation(event: *const c_void) -> CGPoint;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *const c_void);
    }

    unsafe fn resolve_cgs_zoom() -> Option<(CGSConnectionID, CGSSetZoomParametersFn)> {
        let cid_sym = b"CGSMainConnectionID\0".as_ptr() as *const c_char;
        let zoom_sym = b"CGSSetZoomParameters\0".as_ptr() as *const c_char;
        let sls_zoom_sym = b"SLSSetZoomParameters\0".as_ptr() as *const c_char;

        let cid_ptr = libc::dlsym(libc::RTLD_DEFAULT, cid_sym);
        if cid_ptr.is_null() {
            return None;
        }
        let get_cid: CGSMainConnectionIDFn = std::mem::transmute(cid_ptr);
        let cid = get_cid();

        let mut zoom_ptr = libc::dlsym(libc::RTLD_DEFAULT, zoom_sym);
        if zoom_ptr.is_null() {
            zoom_ptr = libc::dlsym(libc::RTLD_DEFAULT, sls_zoom_sym);
        }
        if zoom_ptr.is_null() {
            return None;
        }
        let set_zoom: CGSSetZoomParametersFn = std::mem::transmute(zoom_ptr);
        Some((cid, set_zoom))
    }

    pub fn init() {}

    pub fn cleanup(_was_initialized: bool) {
        apply_transform(1.0, 0.0, 0.0);
    }

    pub fn get_cursor_pos() -> (f32, f32) {
        unsafe {
            let ev = CGEventCreate(null());
            if ev.is_null() {
                return (0.0, 0.0);
            }
            let pt = CGEventGetLocation(ev);
            CFRelease(ev);
            (pt.x as f32, pt.y as f32)
        }
    }

    pub fn apply_transform(zoom: f32, offset_x: f32, offset_y: f32) {
        unsafe {
            if let Some((cid, set_zoom)) = resolve_cgs_zoom() {
                let origin = CGPoint {
                    x: offset_x as f64,
                    y: offset_y as f64,
                };
                let _ = set_zoom(cid, &origin, zoom.max(1.0));
            }
        }
    }
}
