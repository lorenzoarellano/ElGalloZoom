use crate::config::{SharedState, TriggerMode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

static HOOK_STATE: OnceLock<Arc<Mutex<SharedState>>> = OnceLock::new();
static XBUTTON1_HELD: AtomicBool = AtomicBool::new(false);
static XBUTTON2_HELD: AtomicBool = AtomicBool::new(false);
static F24_HELD: AtomicBool = AtomicBool::new(false);

pub fn spawn_hook_thread(state: Arc<Mutex<SharedState>>) {
    let _ = HOOK_STATE.set(state);

    #[cfg(target_os = "windows")]
    windows_hook::spawn();

    #[cfg(target_os = "macos")]
    macos_hook::spawn();
}

// ============================================================================
// WINDOWS ADAPTER (Win32 Low-Level Hooks: WH_MOUSE_LL + WH_KEYBOARD_LL)
// ============================================================================
#[cfg(target_os = "windows")]
mod windows_hook {
    use super::*;
    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_F24, VK_MENU,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
        UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL,
        WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_MOUSEWHEEL, WM_SYSKEYDOWN, WM_SYSKEYUP,
        WM_XBUTTONDOWN, WM_XBUTTONUP,
    };

    const XBUTTON1_FLAG: u16 = 0x0001;
    const XBUTTON2_FLAG: u16 = 0x0002;
    const WHEEL_DELTA: f32 = 120.0;

    pub fn spawn() {
        thread::spawn(|| unsafe {
            let h_instance: HINSTANCE = GetModuleHandleW(None).unwrap_or_default().into();

            let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_ll_proc), h_instance, 0);
            let kbd_hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_ll_proc), h_instance, 0);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            if let Ok(h) = mouse_hook {
                let _ = UnhookWindowsHookEx(h);
            }
            if let Ok(h) = kbd_hook {
                let _ = UnhookWindowsHookEx(h);
            }
        });
    }

    unsafe extern "system" fn mouse_ll_proc(
        n_code: i32,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        if n_code >= 0 {
            let msg = w_param.0 as u32;
            let mouse_struct = &*(l_param.0 as *const MSLLHOOKSTRUCT);

            match msg {
                WM_XBUTTONDOWN | WM_XBUTTONUP => {
                    let is_down = msg == WM_XBUTTONDOWN;
                    let hiword = ((mouse_struct.mouseData >> 16) & 0xFFFF) as u16;
                    if (hiword & XBUTTON1_FLAG) != 0 {
                        XBUTTON1_HELD.store(is_down, Ordering::SeqCst);
                        if let Some(state_lock) = HOOK_STATE.get() {
                            if let Ok(st) = state_lock.lock() {
                                if st.zoom_enabled
                                    && st.config.trigger_mode == TriggerMode::Mouse4Wheel
                                {
                                    return LRESULT(1);
                                }
                            }
                        }
                    }
                    if (hiword & XBUTTON2_FLAG) != 0 {
                        XBUTTON2_HELD.store(is_down, Ordering::SeqCst);
                        if let Some(state_lock) = HOOK_STATE.get() {
                            if let Ok(st) = state_lock.lock() {
                                if st.zoom_enabled
                                    && st.config.trigger_mode == TriggerMode::Mouse5Wheel
                                {
                                    return LRESULT(1);
                                }
                            }
                        }
                    }
                }
                WM_MOUSEWHEEL => {
                    if let Some(state_lock) = HOOK_STATE.get() {
                        if let Ok(mut st) = state_lock.lock() {
                            if st.zoom_enabled
                                && is_trigger_active(st.config.trigger_mode, st.f24_toggled)
                            {
                                let delta = ((mouse_struct.mouseData >> 16) as i16) as f32;
                                let notches = delta / WHEEL_DELTA;
                                let step = st.config.zoom_step;
                                let max_zoom = st.config.max_zoom;

                                let next_zoom =
                                    (st.target_zoom + notches * step).clamp(1.0, max_zoom);
                                st.target_zoom = next_zoom;

                                // Consume the wheel event so underlying window does not scroll
                                return LRESULT(1);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        CallNextHookEx(HHOOK::default(), n_code, w_param, l_param)
    }

    unsafe extern "system" fn keyboard_ll_proc(
        n_code: i32,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        if n_code >= 0 {
            let msg = w_param.0 as u32;
            let kbd_struct = &*(l_param.0 as *const KBDLLHOOKSTRUCT);
            let vk = kbd_struct.vkCode as u16;

            if vk == VK_F24.0 {
                let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
                let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

                if is_down && !F24_HELD.swap(true, Ordering::SeqCst) {
                    if let Some(state_lock) = HOOK_STATE.get() {
                        if let Ok(mut st) = state_lock.lock() {
                            if st.zoom_enabled
                                && st.config.trigger_mode == TriggerMode::F24ToggleAndWheel
                            {
                                st.f24_toggled = !st.f24_toggled;
                                if st.f24_toggled && st.target_zoom < 1.05 {
                                    st.target_zoom = (1.0 + st.config.zoom_step * 2.0)
                                        .min(st.config.max_zoom);
                                } else if !st.f24_toggled {
                                    st.target_zoom = 1.0;
                                }
                                return LRESULT(1);
                            }
                        }
                    }
                } else if is_up {
                    F24_HELD.store(false, Ordering::SeqCst);
                }
            }
        }

        CallNextHookEx(HHOOK::default(), n_code, w_param, l_param)
    }

    unsafe fn is_trigger_active(mode: TriggerMode, f24_toggled: bool) -> bool {
        let ctrl_down = (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
        let alt_down = (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0;

        match mode {
            TriggerMode::CtrlAltWheel => ctrl_down && alt_down,
            TriggerMode::AltWheel => alt_down && !ctrl_down,
            TriggerMode::Mouse4Wheel => XBUTTON1_HELD.load(Ordering::SeqCst),
            TriggerMode::Mouse5Wheel => XBUTTON2_HELD.load(Ordering::SeqCst),
            TriggerMode::F24ToggleAndWheel => F24_HELD.load(Ordering::SeqCst) || f24_toggled,
        }
    }
}

// ============================================================================
// MACOS ADAPTER (CoreGraphics CGEventTapCreate + CFRunLoop)
// ============================================================================
#[cfg(target_os = "macos")]
mod macos_hook {
    use super::*;
    use std::ffi::c_void;
    use std::ptr::null_mut;

    type CFMachPortRef = *mut c_void;
    type CFRunLoopSourceRef = *mut c_void;
    type CFRunLoopRef = *mut c_void;
    type CFStringRef = *const c_void;
    type CGEventTapProxy = *mut c_void;
    type CGEventRef = *mut c_void;
    type CGEventMask = u64;
    type CGEventType = u32;
    type CGEventFlags = u64;
    type CGEventField = u32;

    const K_CG_HID_EVENT_TAP: u32 = 0;
    const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
    const K_CG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;

    const K_CG_EVENT_KEY_DOWN: CGEventType = 10;
    const K_CG_EVENT_KEY_UP: CGEventType = 11;
    const K_CG_EVENT_SCROLL_WHEEL: CGEventType = 22;
    const K_CG_EVENT_OTHER_MOUSE_DOWN: CGEventType = 25;
    const K_CG_EVENT_OTHER_MOUSE_UP: CGEventType = 26;
    const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: CGEventType = 0xFFFF_FFFE;

    const K_CG_EVENT_FLAG_MASK_CONTROL: CGEventFlags = 0x0004_0000;
    const K_CG_EVENT_FLAG_MASK_ALTERNATE: CGEventFlags = 0x0008_0000; // Option / Alt
    const K_CG_EVENT_FLAG_MASK_COMMAND: CGEventFlags = 0x0010_0000; // Command

    const K_CG_MOUSE_EVENT_BUTTON_NUMBER: CGEventField = 3;
    const K_CG_KEYBOARD_EVENT_KEYCODE: CGEventField = 9;
    const K_CG_SCROLL_WHEEL_EVENT_DELTA_AXIS_1: CGEventField = 11;
    const K_CG_SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_1: CGEventField = 96;

    // macOS keycode for F19/F20 (commonly mapped on Logi Options+ on macOS)
    const MACOS_KEYCODE_F19: i64 = 0x50;
    const MACOS_KEYCODE_F20: i64 = 0x5A;

    type CGEventTapCallBack = unsafe extern "C" fn(
        proxy: CGEventTapProxy,
        type_: CGEventType,
        event: CGEventRef,
        user_info: *mut c_void,
    ) -> CGEventRef;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: CGEventMask,
            callback: CGEventTapCallBack,
            user_info: *mut c_void,
        ) -> CFMachPortRef;
        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CGEventGetFlags(event: CGEventRef) -> CGEventFlags;
        fn CGEventGetIntegerValueField(event: CGEventRef, field: CGEventField) -> i64;
        fn CGEventGetDoubleValueField(event: CGEventRef, field: CGEventField) -> f64;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFRunLoopCommonModes: CFStringRef;
        fn CFMachPortCreateRunLoopSource(
            allocator: *const c_void,
            port: CFMachPortRef,
            order: isize,
        ) -> CFRunLoopSourceRef;
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
        fn CFRunLoopRun();
    }

    pub fn spawn() {
        thread::spawn(|| unsafe {
            let mask: CGEventMask = (1 << K_CG_EVENT_SCROLL_WHEEL)
                | (1 << K_CG_EVENT_OTHER_MOUSE_DOWN)
                | (1 << K_CG_EVENT_OTHER_MOUSE_UP)
                | (1 << K_CG_EVENT_KEY_DOWN)
                | (1 << K_CG_EVENT_KEY_UP);

            let tap = CGEventTapCreate(
                K_CG_HID_EVENT_TAP,
                K_CG_HEAD_INSERT_EVENT_TAP,
                K_CG_EVENT_TAP_OPTION_DEFAULT,
                mask,
                event_tap_callback,
                null_mut(),
            );

            if tap.is_null() {
                // Accessibility permissions not yet granted in System Settings -> Privacy & Security -> Accessibility
                return;
            }

            let source = CFMachPortCreateRunLoopSource(null_mut(), tap, 0);
            if !source.is_null() {
                let rl = CFRunLoopGetCurrent();
                CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);
                CFRunLoopRun();
            }
        });
    }

    unsafe extern "C" fn event_tap_callback(
        _proxy: CGEventTapProxy,
        type_: CGEventType,
        event: CGEventRef,
        _user_info: *mut c_void,
    ) -> CGEventRef {
        if type_ == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT {
            return event;
        }

        match type_ {
            K_CG_EVENT_OTHER_MOUSE_DOWN | K_CG_EVENT_OTHER_MOUSE_UP => {
                let is_down = type_ == K_CG_EVENT_OTHER_MOUSE_DOWN;
                let btn = CGEventGetIntegerValueField(event, K_CG_MOUSE_EVENT_BUTTON_NUMBER);
                if btn == 3 {
                    // Mouse 4 (Back)
                    XBUTTON1_HELD.store(is_down, Ordering::SeqCst);
                    if let Some(state_lock) = HOOK_STATE.get() {
                        if let Ok(st) = state_lock.lock() {
                            if st.zoom_enabled && st.config.trigger_mode == TriggerMode::Mouse4Wheel
                            {
                                return null_mut();
                            }
                        }
                    }
                } else if btn == 4 {
                    // Mouse 5 (Forward)
                    XBUTTON2_HELD.store(is_down, Ordering::SeqCst);
                    if let Some(state_lock) = HOOK_STATE.get() {
                        if let Ok(st) = state_lock.lock() {
                            if st.zoom_enabled && st.config.trigger_mode == TriggerMode::Mouse5Wheel
                            {
                                return null_mut();
                            }
                        }
                    }
                }
            }
            K_CG_EVENT_KEY_DOWN | K_CG_EVENT_KEY_UP => {
                let keycode = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE);
                if keycode == MACOS_KEYCODE_F19 || keycode == MACOS_KEYCODE_F20 {
                    let is_down = type_ == K_CG_EVENT_KEY_DOWN;
                    if is_down && !F24_HELD.swap(true, Ordering::SeqCst) {
                        if let Some(state_lock) = HOOK_STATE.get() {
                            if let Ok(mut st) = state_lock.lock() {
                                if st.zoom_enabled
                                    && st.config.trigger_mode == TriggerMode::F24ToggleAndWheel
                                {
                                    st.f24_toggled = !st.f24_toggled;
                                    if st.f24_toggled && st.target_zoom < 1.05 {
                                        st.target_zoom = (1.0 + st.config.zoom_step * 2.0)
                                            .min(st.config.max_zoom);
                                    } else if !st.f24_toggled {
                                        st.target_zoom = 1.0;
                                    }
                                    return null_mut();
                                }
                            }
                        }
                    } else if !is_down {
                        F24_HELD.store(false, Ordering::SeqCst);
                    }
                }
            }
            K_CG_EVENT_SCROLL_WHEEL => {
                if let Some(state_lock) = HOOK_STATE.get() {
                    if let Ok(mut st) = state_lock.lock() {
                        let flags = CGEventGetFlags(event);
                        if st.zoom_enabled
                            && is_macos_trigger_active(st.config.trigger_mode, flags, st.f24_toggled)
                        {
                            let line_delta = CGEventGetIntegerValueField(
                                event,
                                K_CG_SCROLL_WHEEL_EVENT_DELTA_AXIS_1,
                            ) as f32;
                            let pt_delta = CGEventGetDoubleValueField(
                                event,
                                K_CG_SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_1,
                            ) as f32;

                            let notches = if pt_delta.abs() > 0.01 {
                                (pt_delta / 15.0).clamp(-3.0, 3.0)
                            } else {
                                line_delta.clamp(-3.0, 3.0)
                            };

                            if notches.abs() > 0.001 {
                                let step = st.config.zoom_step;
                                let max_zoom = st.config.max_zoom;
                                st.target_zoom =
                                    (st.target_zoom + notches * step).clamp(1.0, max_zoom);
                            }

                            // Consume scroll event on macOS so underlying window does not scroll
                            return null_mut();
                        }
                    }
                }
            }
            _ => {}
        }

        event
    }

    fn is_macos_trigger_active(
        mode: TriggerMode,
        flags: CGEventFlags,
        f24_toggled: bool,
    ) -> bool {
        let ctrl_or_cmd =
            (flags & (K_CG_EVENT_FLAG_MASK_CONTROL | K_CG_EVENT_FLAG_MASK_COMMAND)) != 0;
        let opt_down = (flags & K_CG_EVENT_FLAG_MASK_ALTERNATE) != 0;

        match mode {
            TriggerMode::CtrlAltWheel => ctrl_or_cmd && opt_down,
            TriggerMode::AltWheel => opt_down && !ctrl_or_cmd,
            TriggerMode::Mouse4Wheel => XBUTTON1_HELD.load(Ordering::SeqCst),
            TriggerMode::Mouse5Wheel => XBUTTON2_HELD.load(Ordering::SeqCst),
            TriggerMode::F24ToggleAndWheel => F24_HELD.load(Ordering::SeqCst) || f24_toggled,
        }
    }
}
