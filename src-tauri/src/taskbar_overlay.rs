//! Native Win32 taskbar overlay: one always-on-top layered "pill" window per
//! monitor, positioned over that monitor's taskbar, showing 5h/7d usage.
//!
//! Deliberately NOT a Tauri WebView window - runs on its own thread with a
//! blocking Win32 `GetMessage` loop so idle CPU stays at ~0%. Everything in
//! `imp` below (windows, GDI buffers, the WinEventHook) lives on that one
//! thread; the only cross-thread traffic is the shared `(pct_5h, pct_7d,
//! stale)` tuple written by [`update`] and a `PostThreadMessageW` wakeup.

#[cfg(windows)]
mod imp {
    use crate::data::Settings;
    use ab_glyph::{point, Font, FontArc, PxScale, ScaleFont};
    use std::{
        cell::Cell,
        ffi::c_void,
        mem::size_of,
        sync::{mpsc, Arc, Mutex, OnceLock},
        time::Instant,
    };
    use tauri::{AppHandle, Emitter, Manager};
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{BOOL, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM},
            Graphics::Gdi::{
                CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EnumDisplayMonitors,
                GetDC, GetMonitorInfoW, MonitorFromWindow, ReleaseDC, SelectObject,
                AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
                DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, HMONITOR, MONITORINFO, MONITORINFOEXW,
                MONITOR_DEFAULTTONEAREST,
            },
            System::LibraryLoader::GetModuleHandleW,
            UI::{
                Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK},
                HiDpi::{
                    GetDpiForMonitor, SetProcessDpiAwarenessContext, MDT_EFFECTIVE_DPI,
                    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                },
                Shell::{SHAppBarMessage, ABM_GETTASKBARPOS, APPBARDATA},
                WindowsAndMessaging::{
                    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
                    DestroyWindow, DispatchMessageW, FindWindowExW, GetCursorPos, GetMessageW,
                    GetClassNameW, GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, KillTimer, LoadCursorW, PeekMessageW,
                    PostThreadMessageW, RegisterClassExW, SetForegroundWindow, SetTimer,
                    SetWindowLongPtrW, SetWindowPos, ShowWindow, TrackPopupMenu, TranslateMessage,
                    UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, HCURSOR, IDC_ARROW,
                    MF_SEPARATOR, MF_STRING, MONITORINFOF_PRIMARY, MSG, PM_NOREMOVE,
                    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, TPM_RETURNCMD,
                    TPM_RIGHTBUTTON, ULW_ALPHA, WINEVENT_OUTOFCONTEXT, WM_APP, WM_CONTEXTMENU,
                    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_LBUTTONUP, WM_RBUTTONUP,
                    WM_SETTINGCHANGE, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
                    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, EVENT_SYSTEM_FOREGROUND,
                },
            },
        },
    };

    // ── Tunables ────────────────────────────────────────────────────────────
    const PILL_LOGICAL_W: f32 = 92.0;
    const PILL_LOGICAL_H: f32 = 24.0;
    const DEFAULT_OFFSET_PRIMARY: i32 = 250;
    const DEFAULT_OFFSET_SECONDARY: i32 = 150;
    const REPOSITION_DEBOUNCE_MS: u32 = 500;
    const TIMER_REPOSITION: usize = 1;
    /// Explorer's own taskbar periodically re-asserts its own WS_EX_TOPMOST
    /// position, which can push our overlay windows behind it even though
    /// they're topmost too (ordering among topmost windows is just z-order).
    /// A thread-wide (hwnd-less) timer keeps bumping every overlay window
    /// back to the very top of the z-order to win that fight. Kept well
    /// above 1s since this app is meant to sit in the background for hours
    /// at a time - a rare, brief delay in winning the fight back is a much
    /// better trade than waking the thread every couple of seconds all day.
    const TOPMOST_REASSERT_MS: u32 = 15000;
    const TIMER_TOPMOST: usize = 2;
    const WM_APP_UPDATE: u32 = WM_APP + 1;
    const WM_APP_STOP: u32 = WM_APP + 2;
    const ID_HIDE_MONITOR: u32 = 1001;
    const ID_SETTINGS: u32 = 1002;
    const ID_HIDE_ALL: u32 = 1003;

    // ── Public control API (called from any thread) ─────────────────────────

    struct ControlHandle {
        thread_id: u32,
        join: std::thread::JoinHandle<()>,
        shared: Arc<Mutex<(u8, u8, bool)>>,
    }

    static CONTROL: OnceLock<Mutex<Option<ControlHandle>>> = OnceLock::new();

    fn control() -> &'static Mutex<Option<ControlHandle>> {
        CONTROL.get_or_init(|| Mutex::new(None))
    }

    pub fn start(app: AppHandle, settings: Settings) {
        stop();
        let shared = Arc::new(Mutex::new((0u8, 0u8, true)));
        let shared_thread = shared.clone();
        let (tid_tx, tid_rx) = mpsc::channel::<u32>();

        let join = match std::thread::Builder::new()
            .name("taskbar-overlay".into())
            .spawn(move || thread_main(app, settings, shared_thread, tid_tx))
        {
            Ok(j) => j,
            Err(_) => return,
        };
        let thread_id = tid_rx.recv().unwrap_or(0);
        if thread_id == 0 {
            return;
        }
        *control().lock().unwrap() = Some(ControlHandle { thread_id, join, shared });
    }

    pub fn stop() {
        let handle = control().lock().unwrap().take();
        if let Some(h) = handle {
            unsafe {
                let _ = PostThreadMessageW(h.thread_id, WM_APP_STOP, WPARAM(0), LPARAM(0));
            }
            let _ = h.join.join();
        }
    }

    pub fn update(pct_5h: u8, pct_7d: u8, stale: bool) {
        let guard = control().lock().unwrap();
        if let Some(h) = guard.as_ref() {
            *h.shared.lock().unwrap() = (pct_5h, pct_7d, stale);
            unsafe {
                let _ = PostThreadMessageW(h.thread_id, WM_APP_UPDATE, WPARAM(0), LPARAM(0));
            }
        }
    }

    pub fn apply_settings(app: &AppHandle, old: &Settings, new: &Settings) {
        let relevant_changed = old.taskbar_overlay_enabled != new.taskbar_overlay_enabled
            || old.overlay_all_monitors_fallback != new.overlay_all_monitors_fallback
            || old.overlay_primary_only != new.overlay_primary_only
            || old.overlay_offset_x_overrides != new.overlay_offset_x_overrides;
        if !relevant_changed {
            return;
        }
        if new.taskbar_overlay_enabled {
            start(app.clone(), new.clone());
        } else {
            stop();
        }
    }

    // ── Thread-local context (single-threaded past this point) ─────────────

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum TaskbarEdge {
        Top,
        Bottom,
        Left,
        Right,
    }

    struct MonitorOverlay {
        hmonitor: HMONITOR,
        monitor_rect: RECT,
        hwnd: HWND,
        edge: Option<TaskbarEdge>,
        taskbar_present: bool,
        taskbar_hidden: bool,
        manually_hidden: bool,
        fullscreen_hidden: bool,
        currently_visible: bool,
        dpi: u32,
        size: (u32, u32),
        pos: (i32, i32),
        last_values: Option<(u8, u8, bool)>,
        mem_dc: HDC,
        hbitmap: HBITMAP,
        old_obj: HGDIOBJ,
        bits_ptr: *mut u8,
        buf_size: (u32, u32),
    }

    impl MonitorOverlay {
        fn new(hmonitor: HMONITOR, monitor_rect: RECT, hwnd: HWND) -> Self {
            Self {
                hmonitor,
                monitor_rect,
                hwnd,
                edge: None,
                taskbar_present: false,
                taskbar_hidden: false,
                manually_hidden: false,
                fullscreen_hidden: false,
                currently_visible: false,
                dpi: 96,
                size: (0, 0),
                pos: (0, 0),
                last_values: None,
                mem_dc: HDC(std::ptr::null_mut()),
                hbitmap: HBITMAP(std::ptr::null_mut()),
                old_obj: HGDIOBJ(std::ptr::null_mut()),
                bits_ptr: std::ptr::null_mut(),
                buf_size: (0, 0),
            }
        }
    }

    struct OverlayThreadCtx {
        app: AppHandle,
        settings: Settings,
        monitors: Vec<MonitorOverlay>,
        hook: HWINEVENTHOOK,
        shared: Arc<Mutex<(u8, u8, bool)>>,
        hinstance: HINSTANCE,
        atom: u16,
    }

    thread_local! {
        static CTX_PTR: Cell<*mut OverlayThreadCtx> = const { Cell::new(std::ptr::null_mut()) };
    }

    fn wstr(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    // ── Rendering ────────────────────────────────────────────────────────────

    static FONT: OnceLock<Option<FontArc>> = OnceLock::new();

    fn font() -> Option<&'static FontArc> {
        FONT.get_or_init(|| {
            for path in [
                "C:\\Windows\\Fonts\\segoeuib.ttf",
                "C:\\Windows\\Fonts\\arialbd.ttf",
                "C:\\Windows\\Fonts\\segoeui.ttf",
            ] {
                if let Ok(bytes) = std::fs::read(path) {
                    if let Ok(f) = FontArc::try_from_vec(bytes) {
                        return Some(f);
                    }
                }
            }
            None
        })
        .as_ref()
    }

    // The widget's own light/dark palettes (index.css --bg/--acc2/--navy),
    // used in REVERSE of the current system theme: on a dark taskbar (the
    // common case) the pill uses the widget's light-mode colors, and on a
    // light taskbar it uses the widget's dark-mode colors. That guarantees
    // the pill always contrasts with its surroundings instead of a same-tone
    // pill blending into a same-tone taskbar.
    #[derive(Clone, Copy)]
    struct Palette {
        bg: (u8, u8, u8),
        color_5h: (u8, u8, u8),
        color_7d: (u8, u8, u8),
        divider: (u8, u8, u8),
    }
    const WIDGET_LIGHT: Palette = Palette {
        bg: (255, 255, 255),
        color_5h: (154, 103, 0),
        color_7d: (58, 90, 140),
        divider: (0, 0, 0),
    };
    const WIDGET_DARK: Palette = Palette {
        bg: (14, 17, 23),
        color_5h: (210, 153, 34),
        color_7d: (120, 170, 216),
        divider: (255, 255, 255),
    };

    /// Reads the "SystemUsesLightTheme" registry value that drives the
    /// taskbar's own light/dark appearance. Defaults to `false` (assume a
    /// dark taskbar, the common case) if it can't be read.
    fn system_uses_light_theme() -> bool {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
            .and_then(|key| key.get_value::<u32, _>("SystemUsesLightTheme"))
            .map(|v| v != 0)
            .unwrap_or(false)
    }

    fn active_palette() -> Palette {
        if system_uses_light_theme() {
            WIDGET_DARK
        } else {
            WIDGET_LIGHT
        }
    }

    fn rounded_rect_path(w: f32, h: f32, r: f32) -> tiny_skia::Path {
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        let k = r * 0.552_285;
        let mut pb = tiny_skia::PathBuilder::new();
        pb.move_to(r, 0.0);
        pb.line_to(w - r, 0.0);
        pb.cubic_to(w - r + k, 0.0, w, r - k, w, r);
        pb.line_to(w, h - r);
        pb.cubic_to(w, h - r + k, w - r + k, h, w - r, h);
        pb.line_to(r, h);
        pb.cubic_to(r - k, h, 0.0, h - r + k, 0.0, h - r);
        pb.line_to(0.0, r);
        pb.cubic_to(0.0, r - k, r - k, 0.0, r, 0.0);
        pb.close();
        pb.finish().expect("rounded rect path")
    }

    fn blend_over(buf: &mut [u8], w: u32, h: u32, x: i32, y: i32, color: (u8, u8, u8), coverage: f32, base_alpha: u8) {
        if x < 0 || y < 0 || x as u32 >= w || y as u32 >= h {
            return;
        }
        let idx = ((y as u32 * w + x as u32) * 4) as usize;
        if idx + 3 >= buf.len() {
            return;
        }
        let src_a = (coverage * (base_alpha as f32 / 255.0)).clamp(0.0, 1.0);
        if src_a <= 0.0 {
            return;
        }
        let (r, g, b) = color;
        let dst_b = buf[idx] as f32;
        let dst_g = buf[idx + 1] as f32;
        let dst_r = buf[idx + 2] as f32;
        let dst_a = buf[idx + 3] as f32;
        buf[idx] = (b as f32 * src_a + dst_b * (1.0 - src_a)).round().clamp(0.0, 255.0) as u8;
        buf[idx + 1] = (g as f32 * src_a + dst_g * (1.0 - src_a)).round().clamp(0.0, 255.0) as u8;
        buf[idx + 2] = (r as f32 * src_a + dst_r * (1.0 - src_a)).round().clamp(0.0, 255.0) as u8;
        buf[idx + 3] = (src_a * 255.0 + dst_a * (1.0 - src_a)).round().clamp(0.0, 255.0) as u8;
    }

    fn measure_text(font: &FontArc, text: &str, size_px: f32) -> f32 {
        let scaled = font.as_scaled(PxScale::from(size_px));
        text.chars().map(|c| scaled.h_advance(font.glyph_id(c))).sum()
    }

    fn draw_text(
        buf: &mut [u8],
        w: u32,
        h: u32,
        font: &FontArc,
        text: &str,
        size_px: f32,
        start_x: f32,
        baseline_y: f32,
        color: (u8, u8, u8),
        alpha: u8,
    ) -> f32 {
        let scale = PxScale::from(size_px);
        let scaled_font = font.as_scaled(scale);
        let mut cursor_x = start_x;
        for ch in text.chars() {
            let glyph_id = font.glyph_id(ch);
            let glyph = glyph_id.with_scale_and_position(scale, point(cursor_x, baseline_y));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    let px = bounds.min.x as i32 + gx as i32;
                    let py = bounds.min.y as i32 + gy as i32;
                    blend_over(buf, w, h, px, py, color, coverage, alpha);
                });
            }
            cursor_x += scaled_font.h_advance(glyph_id);
        }
        cursor_x
    }

    /// Renders one pill frame into a premultiplied top-down BGRA buffer ready
    /// for `UpdateLayeredWindow`.
    fn render_pill(pct_5h: u8, pct_7d: u8, stale: bool, w: u32, h: u32) -> Vec<u8> {
        let w = w.max(1);
        let h = h.max(1);
        let wf = w as f32;
        let hf = h as f32;
        let bg_alpha = if stale { 0.45 } else { 0.85 };
        let palette = active_palette();
        let (r, g, b) = palette.bg;

        let mut pixmap = tiny_skia::Pixmap::new(w, h).expect("pixmap alloc");
        let path = rounded_rect_path(wf, hf, hf / 2.0);
        let mut paint = tiny_skia::Paint::default();
        paint.anti_alias = true;
        paint.set_color(tiny_skia::Color::from_rgba8(r, g, b, (bg_alpha * 255.0) as u8));
        pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);

        let divider_w = (wf * 0.012).max(1.0);
        let divider_x = wf * 0.5 - divider_w / 2.0;
        if let Some(div_rect) = tiny_skia::Rect::from_xywh(divider_x, hf * 0.18, divider_w, hf * 0.64) {
            let mut pb = tiny_skia::PathBuilder::new();
            pb.push_rect(div_rect);
            if let Some(div_path) = pb.finish() {
                let (dr, dg, db) = palette.divider;
                let mut dp = tiny_skia::Paint::default();
                dp.anti_alias = true;
                dp.set_color(tiny_skia::Color::from_rgba8(dr, dg, db, (bg_alpha * 90.0) as u8));
                pixmap.fill_path(&div_path, &dp, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
            }
        }

        let mut buf = vec![0u8; (w * h * 4) as usize];
        for (i, px) in pixmap.pixels().iter().enumerate() {
            let o = i * 4;
            buf[o] = px.blue();
            buf[o + 1] = px.green();
            buf[o + 2] = px.red();
            buf[o + 3] = px.alpha();
        }

        if let Some(font) = font() {
            // Both sides share identical sizing/baseline so the pill reads as
            // two symmetric halves. Each number group is horizontally
            // centered within its own half (left half up to the divider,
            // right half after it), rather than anchored to either edge.
            let num_size = hf * 0.74;
            let pct_size = num_size * 0.52;
            let baseline = hf * 0.80;
            let gap = wf * 0.012;
            let text_alpha = if stale { 140 } else { 255 };

            let main_text = if stale { "--".to_string() } else { pct_5h.to_string() };
            let sub_text = if stale { "--".to_string() } else { pct_7d.to_string() };

            let left_half = divider_x;
            let right_half_start = divider_x + divider_w;
            let right_half = wf - right_half_start;

            let main_w = measure_text(font, &main_text, num_size) + gap + measure_text(font, "%", pct_size);
            let main_start = (left_half - main_w) / 2.0;
            let after_main = draw_text(&mut buf, w, h, font, &main_text, num_size, main_start, baseline, palette.color_5h, text_alpha);
            draw_text(&mut buf, w, h, font, "%", pct_size, after_main + gap, baseline, palette.color_5h, text_alpha);

            let sub_w = measure_text(font, &sub_text, num_size) + gap + measure_text(font, "%", pct_size);
            let sub_start = right_half_start + (right_half - sub_w) / 2.0;
            let after_sub = draw_text(&mut buf, w, h, font, &sub_text, num_size, sub_start, baseline, palette.color_7d, text_alpha);
            draw_text(&mut buf, w, h, font, "%", pct_size, after_sub + gap, baseline, palette.color_7d, text_alpha);
        }

        buf
    }

    // ── Taskbar / monitor geometry ───────────────────────────────────────────

    fn is_thin(rect: &RECT, edge: TaskbarEdge) -> bool {
        match edge {
            TaskbarEdge::Top | TaskbarEdge::Bottom => (rect.bottom - rect.top) < 10,
            TaskbarEdge::Left | TaskbarEdge::Right => (rect.right - rect.left) < 10,
        }
    }

    fn infer_edge(rect: &RECT, mon: &RECT) -> TaskbarEdge {
        let w = rect.right - rect.left;
        let h = rect.bottom - rect.top;
        let mw = mon.right - mon.left;
        let mh = mon.bottom - mon.top;
        if w >= mw - 4 || h < mh - 4 && w >= h {
            if (rect.top - mon.top).abs() < (mon.bottom - rect.bottom).abs() {
                TaskbarEdge::Top
            } else {
                TaskbarEdge::Bottom
            }
        } else {
            if (rect.left - mon.left).abs() < (mon.right - rect.right).abs() {
                TaskbarEdge::Left
            } else {
                TaskbarEdge::Right
            }
        }
    }

    struct TaskbarInfo {
        rect: RECT,
        edge: TaskbarEdge,
        hidden: bool,
    }

    fn primary_taskbar_info() -> Option<TaskbarInfo> {
        unsafe {
            let mut abd: APPBARDATA = std::mem::zeroed();
            abd.cbSize = size_of::<APPBARDATA>() as u32;
            let ret = SHAppBarMessage(ABM_GETTASKBARPOS, &mut abd);
            if ret == 0 {
                return None;
            }
            let edge = match abd.uEdge {
                0 => TaskbarEdge::Left,
                1 => TaskbarEdge::Top,
                2 => TaskbarEdge::Right,
                _ => TaskbarEdge::Bottom,
            };
            let hidden = is_thin(&abd.rc, edge);
            Some(TaskbarInfo { rect: abd.rc, edge, hidden })
        }
    }

    fn secondary_taskbar_info(hmonitor: HMONITOR, monitor_rect: RECT) -> Option<TaskbarInfo> {
        unsafe {
            let class = wstr("Shell_SecondaryTrayWnd");
            let mut hwnd = HWND(std::ptr::null_mut());
            loop {
                hwnd = FindWindowExW(None, hwnd, PCWSTR(class.as_ptr()), PCWSTR::null()).unwrap_or(HWND(std::ptr::null_mut()));
                if hwnd.0.is_null() {
                    return None;
                }
                let mut rect = RECT::default();
                if GetWindowRect(hwnd, &mut rect).is_ok() {
                    let m = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
                    if m == hmonitor {
                        let edge = infer_edge(&rect, &monitor_rect);
                        let hidden = is_thin(&rect, edge);
                        return Some(TaskbarInfo { rect, edge, hidden });
                    }
                }
            }
        }
    }

    fn taskbar_for_monitor(hmonitor: HMONITOR, monitor_rect: RECT, is_primary: bool) -> Option<TaskbarInfo> {
        if is_primary {
            primary_taskbar_info().or_else(|| secondary_taskbar_info(hmonitor, monitor_rect))
        } else {
            secondary_taskbar_info(hmonitor, monitor_rect)
        }
    }

    fn monitor_dpi(hmonitor: HMONITOR) -> u32 {
        let mut dpix = 96u32;
        let mut dpiy = 96u32;
        unsafe {
            let _ = GetDpiForMonitor(hmonitor, MDT_EFFECTIVE_DPI, &mut dpix, &mut dpiy);
        }
        dpix.max(1)
    }

    fn compute_position(tb: &TaskbarInfo, offset_logical: i32, dpi: u32, pw: i32, ph: i32) -> (i32, i32) {
        let offset_px = (offset_logical as f32 * dpi as f32 / 96.0) as i32;
        match tb.edge {
            TaskbarEdge::Bottom | TaskbarEdge::Top => {
                let x = tb.rect.right - offset_px - pw;
                let y = tb.rect.top + ((tb.rect.bottom - tb.rect.top) - ph) / 2;
                (x, y)
            }
            TaskbarEdge::Left | TaskbarEdge::Right => {
                let x = tb.rect.left + ((tb.rect.right - tb.rect.left) - pw) / 2;
                let y = tb.rect.bottom - offset_px - ph;
                (x, y)
            }
        }
    }

    fn fallback_position(work_rect: RECT, pw: i32, ph: i32) -> (i32, i32) {
        (work_rect.right - pw - 12, work_rect.bottom - ph - 12)
    }

    // ── Win32 window plumbing ────────────────────────────────────────────────

    fn register_class(hinstance: HINSTANCE) -> u16 {
        let class_name = wstr("UsageTokenOverlayClass");
        let mut wc: WNDCLASSEXW = unsafe { std::mem::zeroed() };
        wc.cbSize = size_of::<WNDCLASSEXW>() as u32;
        wc.style = CS_HREDRAW | CS_VREDRAW;
        wc.lpfnWndProc = Some(wndproc);
        wc.hInstance = hinstance;
        wc.lpszClassName = PCWSTR(class_name.as_ptr());
        wc.hCursor = unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or(HCURSOR(std::ptr::null_mut())) };
        std::mem::forget(class_name); // must outlive the class registration (leaked once per process, negligible)
        unsafe { RegisterClassExW(&wc) }
    }

    fn create_overlay_window(hinstance: HINSTANCE, atom: u16) -> Option<HWND> {
        let title = wstr("UsageToken Overlay");
        unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                PCWSTR(atom as usize as *const u16),
                PCWSTR(title.as_ptr()),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                hinstance,
                None,
            )
            .ok()
        }
    }

    fn ensure_buffer(m: &mut MonitorOverlay, w: u32, h: u32) {
        if m.buf_size == (w, h) && !m.mem_dc.0.is_null() {
            return;
        }
        destroy_buffer(m);
        unsafe {
            let screen_dc = GetDC(None);
            let mem_dc = CreateCompatibleDC(screen_dc);
            ReleaseDC(None, screen_dc);

            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = w as i32;
            bmi.bmiHeader.biHeight = -(h as i32);
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB.0;

            let mut bits: *mut c_void = std::ptr::null_mut();
            if let Ok(hbitmap) = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                let old = SelectObject(mem_dc, hbitmap);
                m.mem_dc = mem_dc;
                m.hbitmap = hbitmap;
                m.old_obj = old;
                m.bits_ptr = bits as *mut u8;
                m.buf_size = (w, h);
            } else {
                let _ = DeleteDC(mem_dc);
            }
        }
    }

    fn destroy_buffer(m: &mut MonitorOverlay) {
        unsafe {
            if !m.mem_dc.0.is_null() {
                if !m.old_obj.0.is_null() {
                    SelectObject(m.mem_dc, m.old_obj);
                }
                if !m.hbitmap.0.is_null() {
                    let _ = DeleteObject(m.hbitmap);
                }
                let _ = DeleteDC(m.mem_dc);
            }
        }
        m.mem_dc = HDC(std::ptr::null_mut());
        m.hbitmap = HBITMAP(std::ptr::null_mut());
        m.old_obj = HGDIOBJ(std::ptr::null_mut());
        m.bits_ptr = std::ptr::null_mut();
        m.buf_size = (0, 0);
    }

    fn destroy_overlay(m: &mut MonitorOverlay) {
        destroy_buffer(m);
        unsafe {
            let _ = DestroyWindow(m.hwnd);
        }
    }

    fn blit(m: &MonitorOverlay, buf: &[u8], w: u32, h: u32) {
        if m.bits_ptr.is_null() {
            return;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(buf.as_ptr(), m.bits_ptr, buf.len().min((w * h * 4) as usize));
            let size = SIZE { cx: w as i32, cy: h as i32 };
            let src_pt = POINT { x: 0, y: 0 };
            let dst_pt = POINT { x: m.pos.0, y: m.pos.1 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let screen_dc = GetDC(None);
            let _ = UpdateLayeredWindow(
                m.hwnd,
                screen_dc,
                Some(&dst_pt as *const _),
                Some(&size as *const _),
                m.mem_dc,
                Some(&src_pt as *const _),
                COLORREF(0),
                Some(&blend as *const _),
                ULW_ALPHA,
            );
            ReleaseDC(None, screen_dc);
        }
    }

    /// Re-asserts HWND_TOPMOST on every overlay window without moving or
    /// resizing it. Explorer's taskbar re-promotes itself to the top of the
    /// topmost band on its own schedule; without this, our overlay can end
    /// up stuck behind it (still "visible", just invisibly so) until the
    /// next display-change event happens to fire.
    fn reassert_topmost(ctx: &mut OverlayThreadCtx) {
        for m in ctx.monitors.iter() {
            if m.currently_visible {
                unsafe {
                    let _ = SetWindowPos(m.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                }
            }
        }
    }

    fn apply_visibility(m: &mut MonitorOverlay, app: &AppHandle) {
        let should_show = m.taskbar_present && !m.manually_hidden && !m.fullscreen_hidden && !m.taskbar_hidden;
        if should_show != m.currently_visible {
            unsafe {
                let _ = ShowWindow(m.hwnd, if should_show { SW_SHOWNOACTIVATE } else { SW_HIDE });
            }
            m.currently_visible = should_show;
            soak_log(
                app,
                &format!(
                    "visibility monitor={:?} show={should_show} present={} manual={} fullscreen={} tb_hidden={}",
                    m.hmonitor.0, m.taskbar_present, m.manually_hidden, m.fullscreen_hidden, m.taskbar_hidden
                ),
            );
        }
    }

    // ── Monitor enumeration / layout ─────────────────────────────────────────

    unsafe extern "system" fn monitor_enum_proc(hmonitor: HMONITOR, _hdc: HDC, _rect: *mut RECT, lparam: LPARAM) -> BOOL {
        let list = &mut *(lparam.0 as *mut Vec<HMONITOR>);
        list.push(hmonitor);
        BOOL(1)
    }

    fn rebuild_monitors(ctx: &mut OverlayThreadCtx, ctx_ptr: isize) {
        let mut hmonitors: Vec<HMONITOR> = Vec::new();
        unsafe {
            let _ = EnumDisplayMonitors(None, None, Some(monitor_enum_proc), LPARAM(&mut hmonitors as *mut _ as isize));
        }

        // Drop overlays for monitors that disappeared (hotplug / sleep-wake).
        let mut i = 0;
        while i < ctx.monitors.len() {
            if !hmonitors.contains(&ctx.monitors[i].hmonitor) {
                let mut m = ctx.monitors.remove(i);
                destroy_overlay(&mut m);
                soak_log(&ctx.app, &format!("monitor-removed monitor={:?}", m.hmonitor.0));
            } else {
                i += 1;
            }
        }

        for hm in hmonitors {
            let mut info: MONITORINFOEXW = unsafe { std::mem::zeroed() };
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            let ok = unsafe { GetMonitorInfoW(hm, &mut info.monitorInfo as *mut MONITORINFO) };
            if !ok.as_bool() {
                continue;
            }
            let is_primary = (info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY) != 0;
            let monitor_rect = info.monitorInfo.rcMonitor;
            let device_name = String::from_utf16_lossy(&info.szDevice)
                .trim_end_matches('\u{0}')
                .to_string();

            if !is_primary && ctx.settings.overlay_primary_only {
                if let Some(pos) = ctx.monitors.iter().position(|m| m.hmonitor == hm) {
                    let mut m = ctx.monitors.remove(pos);
                    destroy_overlay(&mut m);
                }
                continue;
            }

            let tb = taskbar_for_monitor(hm, monitor_rect, is_primary);

            if tb.is_none() && !ctx.settings.overlay_all_monitors_fallback {
                if let Some(pos) = ctx.monitors.iter().position(|m| m.hmonitor == hm) {
                    let mut m = ctx.monitors.remove(pos);
                    destroy_overlay(&mut m);
                }
                continue;
            }

            let dpi = monitor_dpi(hm);
            let pw = (PILL_LOGICAL_W * dpi as f32 / 96.0).round() as i32;
            let ph = (PILL_LOGICAL_H * dpi as f32 / 96.0).round() as i32;

            let offset = ctx
                .settings
                .overlay_offset_x_overrides
                .get(&device_name)
                .copied()
                .unwrap_or(if is_primary { DEFAULT_OFFSET_PRIMARY } else { DEFAULT_OFFSET_SECONDARY });

            let (x, y) = match &tb {
                Some(t) => compute_position(t, offset, dpi, pw, ph),
                None => fallback_position(info.monitorInfo.rcWork, pw, ph),
            };

            let idx = if let Some(pos) = ctx.monitors.iter().position(|m| m.hmonitor == hm) {
                pos
            } else {
                let Some(hwnd) = create_overlay_window(ctx.hinstance, ctx.atom) else { continue };
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, ctx_ptr);
                }
                ctx.monitors.push(MonitorOverlay::new(hm, monitor_rect, hwnd));
                ctx.monitors.len() - 1
            };

            let m = &mut ctx.monitors[idx];
            m.monitor_rect = monitor_rect;
            m.taskbar_present = tb.is_some() || ctx.settings.overlay_all_monitors_fallback;
            m.edge = tb.as_ref().map(|t| t.edge);
            m.taskbar_hidden = tb.as_ref().map(|t| t.hidden).unwrap_or(false);
            m.dpi = dpi;

            let size_changed = m.size != (pw as u32, ph as u32);
            m.size = (pw as u32, ph as u32);
            m.pos = (x, y);

            unsafe {
                let _ = SetWindowPos(m.hwnd, HWND_TOPMOST, x, y, pw, ph, SWP_NOACTIVATE);
            }
            ensure_buffer(m, pw as u32, ph as u32);
            apply_visibility(m, &ctx.app);

            if size_changed {
                // DPI/size changed - force a fresh render even if the values
                // themselves didn't change, since the old buffer no longer fits.
                m.last_values = None;
            }

            soak_log(
                &ctx.app,
                &format!(
                    "reposition monitor={:?} x={x} y={y} w={pw} h={ph} edge={:?} hidden={}",
                    m.hmonitor.0, m.edge, m.taskbar_hidden
                ),
            );
        }

        // Any monitor whose values are now unknown (fresh window / resize)
        // gets an immediate render using the last known usage numbers.
        let (pct_5h, pct_7d, stale) = *ctx.shared.lock().unwrap();
        for m in ctx.monitors.iter_mut() {
            if m.last_values.is_none() {
                render_and_blit(m, pct_5h, pct_7d, stale);
            }
        }
    }

    fn render_and_blit(m: &mut MonitorOverlay, pct_5h: u8, pct_7d: u8, stale: bool) {
        let (w, h) = m.size;
        if w == 0 || h == 0 {
            return;
        }
        ensure_buffer(m, w, h);
        let start = Instant::now();
        let buf = render_pill(pct_5h, pct_7d, stale, w, h);
        blit(m, &buf, w, h);
        m.last_values = Some((pct_5h, pct_7d, stale));
        let _dur = start.elapsed();
    }

    fn do_update(ctx: &mut OverlayThreadCtx) {
        let (pct_5h, pct_7d, stale) = *ctx.shared.lock().unwrap();
        for m in ctx.monitors.iter_mut() {
            if m.last_values == Some((pct_5h, pct_7d, stale)) {
                continue;
            }
            let start = Instant::now();
            render_and_blit(m, pct_5h, pct_7d, stale);
            let dur_us = start.elapsed().as_micros();
            soak_log(
                &ctx.app,
                &format!("render monitor={:?} pct_5h={pct_5h} pct_7d={pct_7d} stale={stale} dur_us={dur_us}", m.hmonitor.0),
            );
        }
    }

    fn soak_log(app: &AppHandle, line: &str) {
        if !crate::soak::soak_enabled() {
            return;
        }
        let Ok(dir) = app.path().app_data_dir() else { return };
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("overlay-soak.log");
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            use std::io::Write;
            let _ = writeln!(f, "ts={ts} {line}");
        }
    }

    // ── Popup / menu interaction ─────────────────────────────────────────────

    fn toggle_popup(app: &AppHandle) {
        if let Some(w) = app.get_webview_window("popup") {
            if w.is_visible().unwrap_or(false) {
                let _ = w.hide();
            } else {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
    }

    fn show_context_menu(ctx: &mut OverlayThreadCtx, hwnd: HWND) {
        unsafe {
            let Ok(hmenu) = CreatePopupMenu() else { return };
            let hide_mon = wstr("Hide overlay on this monitor");
            let settings_label = wstr("Overlay settings");
            let hide_all = wstr("Hide all");
            let _ = AppendMenuW(hmenu, MF_STRING, ID_HIDE_MONITOR as usize, PCWSTR(hide_mon.as_ptr()));
            let _ = AppendMenuW(hmenu, MF_STRING, ID_SETTINGS as usize, PCWSTR(settings_label.as_ptr()));
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(hmenu, MF_STRING, ID_HIDE_ALL as usize, PCWSTR(hide_all.as_ptr()));

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(hwnd);
            let cmd = TrackPopupMenu(hmenu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
            let _ = DestroyMenu(hmenu);

            match cmd.0 as u32 {
                ID_HIDE_MONITOR => {
                    if let Some(m) = ctx.monitors.iter_mut().find(|m| m.hwnd == hwnd) {
                        m.manually_hidden = true;
                        apply_visibility(m, &ctx.app);
                    }
                }
                ID_SETTINGS => {
                    toggle_popup(&ctx.app);
                    let _ = ctx.app.emit("open-settings", ());
                }
                ID_HIDE_ALL => {
                    for m in ctx.monitors.iter_mut() {
                        m.manually_hidden = true;
                        apply_visibility(m, &ctx.app);
                    }
                }
                _ => {}
            }
        }
    }

    fn rects_equal(a: &RECT, b: &RECT) -> bool {
        (a.left - b.left).abs() <= 2 && (a.top - b.top).abs() <= 2 && (a.right - b.right).abs() <= 2 && (a.bottom - b.bottom).abs() <= 2
    }

    /// The desktop background window ("Progman", or a "WorkerW" hosting the
    /// wallpaper) always exactly spans the monitor rect, same as a true
    /// fullscreen app. It becomes foreground constantly during ordinary
    /// window switching (minimizing, closing, alt-tab), so without this
    /// exclusion the fullscreen check below fires on it and hides the
    /// overlay any time nothing else happens to have focus.
    fn class_name_is(buf: &[u16], name: &str) -> bool {
        buf.len() == name.len() && buf.iter().zip(name.bytes()).all(|(&c, b)| c == b as u16)
    }

    // This runs on every EVENT_SYSTEM_FOREGROUND system-wide (any window
    // activation on the whole desktop, not just ours), which can fire
    // dozens of times a minute during ordinary use - compare the raw UTF-16
    // buffer directly instead of allocating a String per call.
    fn is_desktop_window(hwnd: HWND) -> bool {
        let mut buf = [0u16; 16];
        let len = unsafe { GetClassNameW(hwnd, &mut buf) };
        if len <= 0 {
            return false;
        }
        let class = &buf[..len as usize];
        class_name_is(class, "Progman") || class_name_is(class, "WorkerW")
    }

    unsafe extern "system" fn win_event_proc(
        _hook: HWINEVENTHOOK,
        event: u32,
        hwnd: HWND,
        _id_object: i32,
        _id_child: i32,
        _thread: u32,
        _time: u32,
    ) {
        if event != EVENT_SYSTEM_FOREGROUND || hwnd.0.is_null() || is_desktop_window(hwnd) {
            return;
        }
        let ptr = CTX_PTR.with(|c| c.get());
        if ptr.is_null() {
            return;
        }
        let ctx = &mut *ptr;

        let hmonitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut wrect = RECT::default();
        let got_rect = GetWindowRect(hwnd, &mut wrect).is_ok();

        for m in ctx.monitors.iter_mut() {
            let is_fullscreen_here = m.hmonitor == hmonitor && got_rect && rects_equal(&wrect, &m.monitor_rect);
            if m.fullscreen_hidden != is_fullscreen_here {
                m.fullscreen_hidden = is_fullscreen_here;
                apply_visibility(m, &ctx.app);
            }
        }
    }

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let ctx_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OverlayThreadCtx;
        match msg {
            WM_LBUTTONUP => {
                if !ctx_ptr.is_null() {
                    toggle_popup(&(*ctx_ptr).app);
                }
                LRESULT(0)
            }
            WM_RBUTTONUP | WM_CONTEXTMENU => {
                if !ctx_ptr.is_null() {
                    show_context_menu(&mut *ctx_ptr, hwnd);
                }
                LRESULT(0)
            }
            WM_DISPLAYCHANGE | WM_SETTINGCHANGE | WM_DPICHANGED => {
                let _ = SetTimer(hwnd, TIMER_REPOSITION, REPOSITION_DEBOUNCE_MS, None);
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == TIMER_REPOSITION {
                    let _ = KillTimer(hwnd, TIMER_REPOSITION);
                    if !ctx_ptr.is_null() {
                        rebuild_monitors(&mut *ctx_ptr, ctx_ptr as isize);
                    }
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    // ── Thread entry point ───────────────────────────────────────────────────

    fn cleanup(mut ctx: OverlayThreadCtx) {
        unsafe {
            let _ = UnhookWinEvent(ctx.hook);
        }
        for m in ctx.monitors.iter_mut() {
            destroy_overlay(m);
        }
        ctx.monitors.clear();
        CTX_PTR.with(|c| c.set(std::ptr::null_mut()));
    }

    fn thread_main(app: AppHandle, settings: Settings, shared: Arc<Mutex<(u8, u8, bool)>>, tid_tx: mpsc::Sender<u32>) {
        unsafe {
            // Best-effort: Tauri/WebView2 apps are usually already per-monitor
            // DPI aware via the manifest; ignore failure if so.
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

            // Force this thread's message queue into existence before we hand
            // its id back to the caller, so PostThreadMessageW can't race it.
            let mut msg = MSG::default();
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        }
        let thread_id = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
        let _ = tid_tx.send(thread_id);

        let hinstance: HINSTANCE = unsafe { GetModuleHandleW(None).map(|h| HINSTANCE(h.0)).unwrap_or_default() };
        let atom = register_class(hinstance);
        if atom == 0 {
            return;
        }

        let mut ctx = OverlayThreadCtx {
            app,
            settings,
            monitors: Vec::new(),
            hook: HWINEVENTHOOK(std::ptr::null_mut()),
            shared,
            hinstance,
            atom,
        };
        let ctx_ptr: *mut OverlayThreadCtx = &mut ctx;
        CTX_PTR.with(|c| c.set(ctx_ptr));

        ctx.hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };

        rebuild_monitors(&mut ctx, ctx_ptr as isize);
        // SetTimer ignores the id we pass when hWnd is NULL and hands back a
        // system-assigned one instead - that's what WM_TIMER.wParam carries,
        // so we have to remember it to recognize our own timer below.
        let topmost_timer_id = unsafe { SetTimer(None, TIMER_TOPMOST, TOPMOST_REASSERT_MS, None) };

        loop {
            let mut msg = MSG::default();
            let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
            if ret.0 <= 0 {
                break;
            }
            if msg.hwnd.0.is_null() {
                if msg.message == WM_APP_UPDATE {
                    do_update(&mut ctx);
                } else if msg.message == WM_APP_STOP {
                    break;
                } else if msg.message == WM_TIMER && msg.wParam.0 == topmost_timer_id {
                    reassert_topmost(&mut ctx);
                }
            } else {
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }

        cleanup(ctx);
    }
}

#[cfg(windows)]
pub use imp::{apply_settings, start, stop, update};

#[cfg(not(windows))]
pub fn start(_app: tauri::AppHandle, _settings: crate::data::Settings) {}
#[cfg(not(windows))]
pub fn stop() {}
#[cfg(not(windows))]
pub fn update(_pct_5h: u8, _pct_7d: u8, _stale: bool) {}
#[cfg(not(windows))]
pub fn apply_settings(_app: &tauri::AppHandle, _old: &crate::data::Settings, _new: &crate::data::Settings) {}
