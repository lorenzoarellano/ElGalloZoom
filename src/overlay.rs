// ============================================================================
// WINDOWS ADAPTER (Win32 Layered Click-Through Window + UpdateLayeredWindow)
// ============================================================================
#[cfg(target_os = "windows")]
mod imp {
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr::null_mut;
    use windows::core::w;
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, StretchDIBits, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, SRCCOPY,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetSystemMetrics,
        PeekMessageW, RegisterClassW, ShowWindow, TranslateMessage, UpdateLayeredWindow, MSG,
        PM_REMOVE, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    };

    const BUF_W: i32 = 320;
    const BUF_H: i32 = 180;

    pub struct SpotlightOverlay {
        hwnd: HWND,
        is_visible: bool,
        pixels: Vec<u32>,
    }

    impl SpotlightOverlay {
        pub fn new() -> Self {
            unsafe {
                let h_instance: HINSTANCE = GetModuleHandleW(None).unwrap_or_default().into();
                let class_name = w!("EGZSpotlightOverlayClass");

                let wc = WNDCLASSW {
                    lpfnWndProc: Some(overlay_wnd_proc),
                    hInstance: h_instance,
                    lpszClassName: class_name,
                    ..Default::default()
                };
                let _ = RegisterClassW(&wc);

                let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
                let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
                let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1920);
                let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1080);

                let ex_style = WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE;

                let hwnd = CreateWindowExW(
                    ex_style,
                    class_name,
                    w!("EGZ Spotlight"),
                    WS_POPUP,
                    vx,
                    vy,
                    vw,
                    vh,
                    None,
                    None,
                    h_instance,
                    None,
                )
                .unwrap_or_default();

                Self {
                    hwnd,
                    is_visible: false,
                    pixels: vec![0u32; (BUF_W * BUF_H) as usize],
                }
            }
        }

        pub fn update(&mut self, cursor_x: i32, cursor_y: i32, max_opacity: f32, radius_px: f32) {
            if self.hwnd.0.is_null() {
                return;
            }

            unsafe {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, self.hwnd, 0, 0, PM_REMOVE).into() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
                let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
                let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
                let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);

                if !self.is_visible {
                    let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                    self.is_visible = true;
                }

                let scale_x = vw as f32 / BUF_W as f32;
                let scale_y = vh as f32 / BUF_H as f32;
                let rel_cx = (cursor_x - vx) as f32;
                let rel_cy = (cursor_y - vy) as f32;
                let inner_r = radius_px.max(40.0);
                let feather_r = inner_r * 1.85;
                let max_alpha = (max_opacity.clamp(0.0, 0.85) * 255.0) as f32;

                for by in 0..BUF_H {
                    let py = (by as f32 + 0.5) * scale_y;
                    let dy = py - rel_cy;
                    for bx in 0..BUF_W {
                        let px = (bx as f32 + 0.5) * scale_x;
                        let dx = px - rel_cx;
                        let dist = (dx * dx + dy * dy).sqrt();

                        let t = if dist <= inner_r {
                            0.0
                        } else if dist >= feather_r {
                            1.0
                        } else {
                            let u = (dist - inner_r) / (feather_r - inner_r);
                            u * u * (3.0 - 2.0 * u)
                        };

                        let alpha = (t * max_alpha).round() as u32;
                        self.pixels[(by * BUF_W + bx) as usize] = alpha << 24;
                    }
                }

                let screen_dc = GetDC(None);
                let mem_dc = CreateCompatibleDC(screen_dc);

                let bmi_screen = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: vw,
                        biHeight: -vh,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };

                let mut bits_ptr: *mut c_void = null_mut();
                if let Ok(dib) = CreateDIBSection(
                    mem_dc,
                    &bmi_screen,
                    DIB_RGB_COLORS,
                    &mut bits_ptr,
                    None,
                    0,
                ) {
                    let old_obj = SelectObject(mem_dc, dib);

                    let bmi_src = BITMAPINFO {
                        bmiHeader: BITMAPINFOHEADER {
                            biSize: size_of::<BITMAPINFOHEADER>() as u32,
                            biWidth: BUF_W,
                            biHeight: -BUF_H,
                            biPlanes: 1,
                            biBitCount: 32,
                            biCompression: BI_RGB.0,
                            ..Default::default()
                        },
                        ..Default::default()
                    };

                    let _ = StretchDIBits(
                        mem_dc,
                        0,
                        0,
                        vw,
                        vh,
                        0,
                        0,
                        BUF_W,
                        BUF_H,
                        Some(self.pixels.as_ptr() as *const c_void),
                        &bmi_src,
                        DIB_RGB_COLORS,
                        SRCCOPY,
                    );

                    let pt_dst = POINT { x: vx, y: vy };
                    let size_dst = SIZE { cx: vw, cy: vh };
                    let pt_src = POINT { x: 0, y: 0 };
                    let blend = BLENDFUNCTION {
                        BlendOp: AC_SRC_OVER as u8,
                        BlendFlags: 0,
                        SourceConstantAlpha: 255,
                        AlphaFormat: AC_SRC_ALPHA as u8,
                    };

                    let _ = UpdateLayeredWindow(
                        self.hwnd,
                        screen_dc,
                        Some(&pt_dst),
                        Some(&size_dst),
                        mem_dc,
                        Some(&pt_src),
                        windows::Win32::Foundation::COLORREF(0),
                        Some(&blend),
                        ULW_ALPHA,
                    );

                    SelectObject(mem_dc, old_obj);
                    let _ = DeleteObject(dib);
                }

                let _ = DeleteDC(mem_dc);
                let _ = ReleaseDC(None, screen_dc);
            }
        }

        pub fn hide(&mut self) {
            if self.is_visible && !self.hwnd.0.is_null() {
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
                self.is_visible = false;
            }
        }
    }

    impl Drop for SpotlightOverlay {
        fn drop(&mut self) {
            if !self.hwnd.0.is_null() {
                unsafe {
                    let _ = DestroyWindow(self.hwnd);
                }
            }
        }
    }

    unsafe extern "system" fn overlay_wnd_proc(
        hwnd: HWND,
        msg: u32,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        DefWindowProcW(hwnd, msg, w_param, l_param)
    }
}

// ============================================================================
// MACOS ADAPTER (Main-Queue Dispatched Click-Through NSWindow Overlay)
// ============================================================================
#[cfg(target_os = "macos")]
mod imp {
    use objc::runtime::{Class, Object, BOOL, NO, YES};
    use objc::{msg_send, sel, sel_impl};
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSPoint {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSSize {
        width: f64,
        height: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSRect {
        origin: NSPoint,
        size: NSSize,
    }

    extern "C" {
        static _dispatch_main_q: c_void;
        fn dispatch_async_f(
            queue: *const c_void,
            context: *mut c_void,
            work: unsafe extern "C" fn(*mut c_void),
        );
    }

    static NS_OVERLAY_WIN: AtomicUsize = AtomicUsize::new(0);

    pub struct SpotlightOverlay {
        is_visible: bool,
    }

    struct OverlayUpdateCmd {
        opacity: f64,
        visible: bool,
    }

    unsafe extern "C" fn apply_on_main_thread(ctx: *mut c_void) {
        let cmd = Box::from_raw(ctx as *mut OverlayUpdateCmd);
        let mut win_ptr = NS_OVERLAY_WIN.load(Ordering::SeqCst) as *mut Object;

        if win_ptr.is_null() && cmd.visible {
            if let (Some(screen_cls), Some(win_cls)) =
                (Class::get("NSScreen"), Class::get("NSWindow"))
            {
                let main_screen: *mut Object = msg_send![screen_cls, mainScreen];
                if !main_screen.is_null() {
                    let frame: NSRect = msg_send![main_screen, frame];
                    let alloc: *mut Object = msg_send![win_cls, alloc];
                    // styleMask 0 = NSWindowStyleMaskBorderless, backing 2 = NSBackingStoreBuffered
                    let win: *mut Object = msg_send![
                        alloc,
                        initWithContentRect: frame
                        styleMask: 0usize
                        backing: 2usize
                        defer: NO
                    ];
                    if !win.is_null() {
                        let _: () = msg_send![win, setOpaque: NO];
                        let _: () = msg_send![win, setIgnoresMouseEvents: YES];
                        // NSStatusWindowLevel = 25
                        let _: () = msg_send![win, setLevel: 25isize];
                        // CanJoinAllSpaces (1 << 0) | Stationary (1 << 4) | IgnoresCycle (1 << 6)
                        let behavior: usize = (1 << 0) | (1 << 4) | (1 << 6);
                        let _: () = msg_send![win, setCollectionBehavior: behavior];

                        if let Some(color_cls) = Class::get("NSColor") {
                            let black: *mut Object = msg_send![
                                color_cls,
                                colorWithCalibratedWhite: 0.0f64
                                alpha: 1.0f64
                            ];
                            let _: () = msg_send![win, setBackgroundColor: black];
                        }

                        NS_OVERLAY_WIN.store(win as usize, Ordering::SeqCst);
                        win_ptr = win;
                    }
                }
            }
        }

        if !win_ptr.is_null() {
            if cmd.visible {
                let _: () = msg_send![win_ptr, setAlphaValue: cmd.opacity * 0.45];
                let _: () = msg_send![win_ptr, orderFrontRegardless];
            } else {
                let _: () = msg_send![win_ptr, orderOut: std::ptr::null_mut::<Object>()];
            }
        }
    }

    impl SpotlightOverlay {
        pub fn new() -> Self {
            Self { is_visible: false }
        }

        pub fn update(
            &mut self,
            _cursor_x: i32,
            _cursor_y: i32,
            max_opacity: f32,
            _radius_px: f32,
        ) {
            self.is_visible = true;
            let cmd = Box::new(OverlayUpdateCmd {
                opacity: max_opacity.clamp(0.0, 0.85) as f64,
                visible: true,
            });
            unsafe {
                dispatch_async_f(
                    &_dispatch_main_q as *const c_void,
                    Box::into_raw(cmd) as *mut c_void,
                    apply_on_main_thread,
                );
            }
        }

        pub fn hide(&mut self) {
            if self.is_visible {
                self.is_visible = false;
                let cmd = Box::new(OverlayUpdateCmd {
                    opacity: 0.0,
                    visible: false,
                });
                unsafe {
                    dispatch_async_f(
                        &_dispatch_main_q as *const c_void,
                        Box::into_raw(cmd) as *mut c_void,
                        apply_on_main_thread,
                    );
                }
            }
        }
    }
}

pub use imp::SpotlightOverlay;
