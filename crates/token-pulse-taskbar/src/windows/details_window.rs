//! Thread-owned, nonactivating read-only details. All content comes from the host projection.
use super::{
    render::{NativeFont, Palette, rgb},
    topology::{ScreenRect, wide},
};
use crate::{
    TaskbarView, WireError,
    details::{DetailContent, content},
    display::{Density, MeasuredPlan, PlacedSpan, Span, Tone},
};
use std::{
    cell::{Cell, RefCell},
    mem, ptr,
    rc::Rc,
};
use token_pulse_core::settings::AppTheme;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, COLOR_WINDOW, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, EndPaint,
        FillRect, FrameRect, GetDC, GetMonitorInfoW, GetSysColor, InvalidateRect,
        MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow, PAINTSTRUCT, ReleaseDC,
        SetWindowRgn, UpdateWindow,
    },
    System::{
        LibraryLoader::GetModuleHandleW,
        Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
        SystemInformation::GetTickCount64,
    },
    UI::{
        HiDpi::GetDpiForWindow,
        WindowsAndMessaging::{
            CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA,
            GetClientRect, GetCursorPos, GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST,
            IsWindowVisible, KillTimer, MA_NOACTIVATE, RegisterClassExW, SW_HIDE, SWP_NOACTIVATE,
            SWP_NOOWNERZORDER, SWP_SHOWWINDOW, SetTimer, SetWindowLongPtrW, SetWindowPos,
            SetWindowTextW, ShowWindow, UnregisterClassW, WM_CANCELMODE, WM_CLOSE, WM_DPICHANGED,
            WM_ERASEBKGND, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_MOUSEWHEEL, WM_NCCREATE,
            WM_NCDESTROY, WM_PAINT, WM_PRINTCLIENT, WM_THEMECHANGED, WM_TIMER, WNDCLASSEXW,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        },
    },
};
pub const HOVER_MS: u32 = 300;
const WATCH: usize = 1;
fn pixels(dip: i32, dpi: u32) -> i32 {
    dip * dpi as i32 / 96
}
fn screen(rect: RECT) -> ScreenRect {
    ScreenRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}
fn contains(rect: ScreenRect, point: POINT) -> bool {
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}
fn work_area(source: HWND) -> Result<ScreenRect, WireError> {
    let monitor = unsafe { MonitorFromWindow(source, MONITOR_DEFAULTTONEAREST) };
    let mut info: MONITORINFO = unsafe { mem::zeroed() };
    info.cbSize = mem::size_of::<MONITORINFO>() as u32;
    if monitor.is_null() || unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return Err(WireError::InvalidState);
    }
    let work = screen(info.rcWork);
    if !work.valid() {
        return Err(WireError::InvalidState);
    }
    Ok(work)
}
fn window_rect(window: HWND) -> Result<ScreenRect, WireError> {
    let mut value: RECT = unsafe { mem::zeroed() };
    if unsafe { GetWindowRect(window, &mut value) } == 0 {
        return Err(WireError::Closed);
    }
    let value = screen(value);
    if !value.valid() {
        return Err(WireError::InvalidState);
    }
    Ok(value)
}
pub(crate) fn placement(
    anchor: ScreenRect,
    work: ScreenRect,
    width: i32,
    requested_height: i32,
    dpi: u32,
) -> Result<ScreenRect, WireError> {
    if !anchor.valid() || !work.valid() || !(96..=768).contains(&dpi) {
        return Err(WireError::InvalidFrame);
    }
    let margin = pixels(4, dpi);
    let gap = pixels(8, dpi);
    let above = (anchor.top - work.top - gap - margin).max(0);
    let below = (work.bottom - anchor.bottom - gap - margin).max(0);
    let height = requested_height.min(pixels(600, dpi)).min(above.max(below));
    if width <= 0 || width > work.width() - margin * 2 || height < pixels(64, dpi) {
        return Err(WireError::InvalidState);
    }
    let left = (anchor.right - width).clamp(work.left + margin, work.right - margin - width);
    let top = if above >= height {
        anchor.top - gap - height
    } else {
        anchor.bottom + gap
    };
    if top < work.top + margin || top + height > work.bottom - margin {
        return Err(WireError::InvalidState);
    }
    Ok(ScreenRect {
        left,
        top,
        right: left + width,
        bottom: top + height,
    })
}
fn palette(theme: AppTheme) -> Result<Palette, WireError> {
    let background = match theme {
        AppTheme::Dark => rgb(22, 25, 31),
        AppTheme::Light => rgb(250, 252, 255),
        AppTheme::System => {
            // Same read-only per-user theme source used by the pinned Tauri/tao Windows backend.
            let mut value = 0u32;
            let mut size = 4u32;
            let status = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize").as_ptr(),
                    wide("AppsUseLightTheme").as_ptr(),
                    RRF_RT_REG_DWORD,
                    ptr::null_mut(),
                    (&mut value as *mut u32).cast(),
                    &mut size,
                )
            };
            if status == 0 && size == 4 {
                if value == 0 {
                    rgb(22, 25, 31)
                } else {
                    rgb(250, 252, 255)
                }
            } else {
                unsafe { GetSysColor(COLOR_WINDOW) }
            }
        }
    };
    Palette::system(background)
}
fn wrap(
    text: &str,
    width: i32,
    measure: &impl Fn(&str) -> Result<i32, WireError>,
) -> Result<Vec<String>, WireError> {
    let mut rows = vec![];
    let mut current = String::new();
    for ch in text.chars() {
        let mut candidate = current.clone();
        candidate.push(ch);
        if measure(&candidate)? > width {
            if current.is_empty() {
                return Err(WireError::InvalidState);
            }
            rows.push(current);
            current = ch.to_string();
            if measure(&current)? > width {
                return Err(WireError::InvalidState);
            }
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        rows.push(current);
    }
    Ok(rows)
}
fn priority(label: &str) -> u8 {
    match label {
        "统计范围" => 0,
        "可信 Token" => 1,
        "已计价部分估算" => 2,
        "价格覆盖" | "未计价 Token" | "计价状态" | "隐私模式" => 3,
        "输入总数" | "缓存输入（包含在输入中）" | "输出总数" | "分项口径" => {
            4
        }
        "账户额度（独立范围）" | "账户额度" | "额度桶" => 5,
        "重置时间" | "额度窗口" => 6,
        value if value.starts_with("窗口 ") => 6,
        _ => 7,
    }
}
#[derive(Clone)]
struct Bar {
    top: i32,
    left: i32,
    width: i32,
    percent: f64,
}
pub(crate) struct Layout {
    plan: MeasuredPlan,
    bars: Vec<Bar>,
}
impl Layout {
    fn build(
        content: &DetailContent,
        dpi: u32,
        width: i32,
        font: &NativeFont,
    ) -> Result<Self, WireError> {
        let pad = pixels(12, dpi);
        let gap = pixels(8, dpi);
        let label_width = pixels(104, dpi);
        let value_x = pad + label_width + gap;
        let value_width = width - value_x - pad - pixels(8, dpi);
        if value_width < pixels(48, dpi) || content.rows.len() > 160 {
            return Err(WireError::InvalidFrame);
        }
        let measure = |text: &str| font.width(text);
        let mut spans = vec![];
        let mut bars = vec![];
        let mut y = pad;
        for line in wrap(&content.title, width - pad * 2, &measure)? {
            let measured = font.width(&line)?;
            spans.push(PlacedSpan {
                span: Span {
                    text: line,
                    tone: Tone::Normal,
                },
                x: pad,
                y,
                width: measured,
            });
            y += font.height();
        }
        y += gap * 2;
        let mut rows: Vec<_> = content.rows.iter().collect();
        rows.sort_by_key(|row| priority(&row.label));
        for row in rows {
            let labels = wrap(&row.label, label_width, &measure)?;
            let values = wrap(&row.value, value_width, &measure)?;
            let lines = labels.len().max(values.len()).max(1);
            for (offset, text, x, tone) in labels
                .into_iter()
                .enumerate()
                .map(|(offset, line)| (offset, line, pad, Tone::Muted))
                .chain(
                    values
                        .into_iter()
                        .enumerate()
                        .map(|(offset, line)| (offset, line, value_x, row.tone)),
                )
            {
                let measured = font.width(&text)?;
                spans.push(PlacedSpan {
                    span: Span { text, tone },
                    x,
                    y: y + offset as i32 * font.height(),
                    width: measured,
                });
            }
            y += lines as i32 * font.height();
            if let Some(percent) = row.remaining_percent {
                bars.push(Bar {
                    top: y + pixels(3, dpi),
                    left: value_x,
                    width: value_width,
                    percent,
                });
                y += pixels(9, dpi);
            }
            y += gap;
            if y > 100_000 {
                return Err(WireError::TooLarge);
            }
        }
        y += pad;
        Ok(Self {
            plan: MeasuredPlan {
                width,
                height: y,
                density: Density::Full,
                spans,
            },
            bars,
        })
    }
}
struct Frame {
    font: NativeFont,
    layout: Layout,
    palette: Palette,
    dpi: u32,
    text: String,
}
impl Frame {
    fn build(view: &TaskbarView, dpi: u32, width: i32, now: i64) -> Result<Self, WireError> {
        let model = content(view, now)?;
        let theme = view.details.as_ref().ok_or(WireError::InvalidState)?.theme;
        let font = NativeFont::new(dpi)?;
        let layout = Layout::build(&model, dpi, width, &font)?;
        Ok(Self {
            font,
            layout,
            palette: palette(theme)?,
            dpi,
            text: format!(
                "{}；超出面板时可用滚轮或入口的方向键、PageUp / PageDown、Home / End 阅读；Escape 关闭",
                model.accessible_text()
            ),
        })
    }
    unsafe fn paint(
        &self,
        dc: windows_sys::Win32::Graphics::Gdi::HDC,
        width: i32,
        height: i32,
        scroll: i32,
    ) -> Result<(), WireError> {
        let mut plan = self.layout.plan.clone();
        for span in &mut plan.spans {
            span.y -= scroll;
        }
        unsafe {
            self.font
                .paint_mode(dc, Some(&plan), width, height, self.palette, false)?;
        }
        let fill = |area: RECT, color| -> Result<(), WireError> {
            let brush = unsafe { CreateSolidBrush(color) };
            if brush.is_null() {
                return Err(WireError::InvalidState);
            }
            let result = unsafe { FillRect(dc, &area, brush) };
            unsafe {
                DeleteObject(brush);
            }
            if result == 0 {
                Err(WireError::InvalidState)
            } else {
                Ok(())
            }
        };
        for bar in &self.layout.bars {
            let top = bar.top - scroll;
            if top < 0 || top + pixels(4, self.dpi) > height {
                continue;
            }
            let area = RECT {
                left: bar.left,
                top,
                right: bar.left + bar.width,
                bottom: top + pixels(4, self.dpi),
            };
            fill(area, self.palette.muted)?;
            let amount = ((bar.percent / 100.0) * f64::from(bar.width)).round() as i32;
            if amount > 0 {
                fill(
                    RECT {
                        right: area.left + amount,
                        ..area
                    },
                    if bar.percent <= 20.0 {
                        self.palette.warning
                    } else {
                        self.palette.accent
                    },
                )?;
            }
        }
        if self.layout.plan.height > height {
            let thumb = (i64::from(height) * i64::from(height) / i64::from(self.layout.plan.height))
                .max(i64::from(pixels(24, self.dpi)))
                .min(i64::from(height)) as i32;
            let top = (i64::from(scroll) * i64::from(height - thumb)
                / i64::from(self.layout.plan.height - height)) as i32;
            fill(
                RECT {
                    left: width - pixels(5, self.dpi),
                    right: width - pixels(2, self.dpi),
                    top,
                    bottom: top + thumb,
                },
                self.palette.muted,
            )?;
        }
        let border = unsafe { CreateSolidBrush(self.palette.muted) };
        if border.is_null() {
            return Err(WireError::InvalidState);
        }
        let result = unsafe {
            FrameRect(
                dc,
                &RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                },
                border,
            )
        };
        unsafe {
            DeleteObject(border);
        }
        if result == 0 {
            return Err(WireError::InvalidState);
        }
        Ok(())
    }
}
struct PopupState {
    frame: RefCell<Option<Rc<Frame>>>,
    window: Cell<HWND>,
    source: Cell<HWND>,
    anchor: Cell<ScreenRect>,
    alive: Cell<bool>,
    painting: Cell<bool>,
    failed: Cell<bool>,
    scroll: Cell<i32>,
    wheel: Cell<i32>,
    keyboard: Cell<bool>,
    outside_since: Cell<Option<u64>>,
}
impl PopupState {
    fn visible(&self) -> bool {
        self.alive.get() && unsafe { IsWindowVisible(self.window.get()) } != 0
    }
    fn hide(&self) {
        self.keyboard.set(false);
        self.outside_since.set(None);
        if self.alive.get() {
            unsafe {
                KillTimer(self.window.get(), WATCH);
                ShowWindow(self.window.get(), SW_HIDE);
            }
        }
    }
    fn redraw(&self) {
        if self.alive.get() {
            unsafe {
                InvalidateRect(self.window.get(), ptr::null(), 0);
                UpdateWindow(self.window.get());
            }
        }
    }
    fn scroll_by(&self, amount: i32, absolute: bool) {
        let Some(frame) = self.frame.borrow().clone() else {
            return;
        };
        let mut client: RECT = unsafe { mem::zeroed() };
        if unsafe { GetClientRect(self.window.get(), &mut client) } == 0 {
            return;
        }
        let max = (frame.layout.plan.height - client.bottom).max(0);
        let next = if absolute {
            amount
        } else {
            self.scroll.get().saturating_add(amount)
        }
        .clamp(0, max);
        if next != self.scroll.replace(next) {
            self.redraw();
        }
    }
    fn watch(&self) {
        if !self.visible() {
            return;
        }
        if unsafe { IsWindowVisible(self.source.get()) } == 0
            || window_rect(self.source.get()).ok() != Some(self.anchor.get())
        {
            self.hide();
            return;
        }
        if self.keyboard.get() {
            return;
        }
        let mut point: POINT = unsafe { mem::zeroed() };
        let Ok(popup) = window_rect(self.window.get()) else {
            self.hide();
            return;
        };
        let anchor = self.anchor.get();
        let bridge = ScreenRect {
            left: popup.left.max(anchor.left),
            right: popup.right.min(anchor.right),
            top: popup.bottom.min(anchor.bottom),
            bottom: popup.top.max(anchor.top),
        };
        if unsafe { GetCursorPos(&mut point) } != 0
            && (contains(anchor, point) || contains(popup, point) || contains(bridge, point))
        {
            self.outside_since.set(None);
            return;
        }
        let now = unsafe { GetTickCount64() };
        if self
            .outside_since
            .get()
            .is_some_and(|at| now.saturating_sub(at) >= 80)
        {
            self.hide();
        } else if self.outside_since.get().is_none() {
            self.outside_since.set(Some(now));
        }
    }
}
unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        unsafe {
            (*(create.lpCreateParams as *const PopupState))
                .window
                .set(window);
        }
    }
    let raw = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const PopupState;
    if !raw.is_null() {
        unsafe {
            Rc::increment_strong_count(raw);
        }
        let state = unsafe { Rc::from_raw(raw) };
        match message {
            WM_MOUSEACTIVATE => return MA_NOACTIVATE as _,
            WM_ERASEBKGND => return 1,
            WM_CANCELMODE | WM_CLOSE | WM_DPICHANGED | WM_THEMECHANGED => {
                state.hide();
                return 0;
            }
            WM_TIMER if wparam == WATCH => {
                state.watch();
                return 0;
            }
            WM_MOUSEWHEEL => {
                let delta = (wparam >> 16) as u16 as i16 as i32 + state.wheel.get();
                state.wheel.set(delta % 120);
                let step = state
                    .frame
                    .borrow()
                    .as_ref()
                    .map(|frame| frame.font.height() * 3)
                    .unwrap_or(0);
                state.scroll_by(-(delta / 120) * step, false);
                return 0;
            }
            WM_LBUTTONDOWN => {
                let x = lparam as u16 as i16 as i32;
                let y = (lparam >> 16) as u16 as i16 as i32;
                let mut client: RECT = unsafe { mem::zeroed() };
                let frame = state.frame.borrow().clone();
                if unsafe { GetClientRect(window, &mut client) } != 0 {
                    if let Some(frame) = frame {
                        if x >= client.right - pixels(12, frame.dpi) && client.bottom > 0 {
                            let target = (i64::from(y.clamp(0, client.bottom))
                                * i64::from((frame.layout.plan.height - client.bottom).max(0))
                                / i64::from(client.bottom))
                                as i32;
                            state.scroll_by(target, true);
                        }
                    }
                }
                return 0;
            }
            WM_PAINT | WM_PRINTCLIENT => {
                if state.painting.replace(true) {
                    return 0;
                }
                let frame = state.frame.borrow().clone();
                let mut paint: PAINTSTRUCT = unsafe { mem::zeroed() };
                let dc = if message == WM_PAINT {
                    unsafe { BeginPaint(window, &mut paint) }
                } else {
                    wparam as _
                };
                let mut client: RECT = unsafe { mem::zeroed() };
                let result = if dc.is_null() || unsafe { GetClientRect(window, &mut client) } == 0 {
                    Err(WireError::InvalidState)
                } else if let Some(frame) = frame {
                    unsafe { frame.paint(dc, client.right, client.bottom, state.scroll.get()) }
                } else {
                    let brush = unsafe { CreateSolidBrush(rgb(22, 25, 31)) };
                    let ok = !brush.is_null() && unsafe { FillRect(dc, &client, brush) } != 0;
                    if !brush.is_null() {
                        unsafe {
                            DeleteObject(brush);
                        }
                    }
                    if ok {
                        Ok(())
                    } else {
                        Err(WireError::InvalidState)
                    }
                };
                if result.is_err() {
                    state.failed.set(true);
                }
                if message == WM_PAINT {
                    unsafe {
                        EndPaint(window, &paint);
                    }
                }
                state.painting.set(false);
                return 0;
            }
            WM_NCDESTROY => {
                state.alive.set(false);
                state.frame.borrow_mut().take();
                unsafe {
                    SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                }
            }
            _ => {}
        }
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
pub(crate) struct NativeDetails {
    pub(crate) window: HWND,
    class: Vec<u16>,
    state: Rc<PopupState>,
}
impl NativeDetails {
    pub(crate) fn create(owner: HWND) -> Result<Self, WireError> {
        let class = wide(&format!(
            "TokenPulse.Taskbar.Details.{}",
            uuid::Uuid::new_v4().simple()
        ));
        let module = unsafe { GetModuleHandleW(ptr::null()) };
        let state = Rc::new(PopupState {
            frame: RefCell::new(None),
            window: Cell::new(ptr::null_mut()),
            source: Cell::new(ptr::null_mut()),
            anchor: Cell::new(ScreenRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            }),
            alive: Cell::new(true),
            painting: Cell::new(false),
            failed: Cell::new(false),
            scroll: Cell::new(0),
            wheel: Cell::new(0),
            keyboard: Cell::new(false),
            outside_since: Cell::new(None),
        });
        let wc = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(procedure),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            ..unsafe { mem::zeroed() }
        };
        if unsafe { RegisterClassExW(&wc) } == 0 {
            return Err(WireError::InvalidState);
        }
        let window = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                class.as_ptr(),
                wide("TokenPulse 详情已关闭").as_ptr(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                owner,
                ptr::null_mut(),
                module,
                Rc::as_ptr(&state).cast(),
            )
        };
        if window.is_null() {
            unsafe {
                UnregisterClassW(class.as_ptr(), module);
            }
            return Err(WireError::InvalidState);
        }
        Ok(Self {
            window,
            class,
            state,
        })
    }
    pub(crate) fn prepare(
        &self,
        source: HWND,
        view: &TaskbarView,
        dpi: u32,
        now: i64,
    ) -> Result<(), WireError> {
        let result = self.prepare_frame(source, view, dpi, now);
        if result.is_err() {
            self.clear().ok();
        }
        result
    }
    fn prepare_frame(
        &self,
        source: HWND,
        view: &TaskbarView,
        dpi: u32,
        now: i64,
    ) -> Result<(), WireError> {
        if !(96..=768).contains(&dpi) {
            return Err(WireError::InvalidFrame);
        }
        self.state.source.set(source);
        if view.details.is_none() {
            return self.clear();
        }
        let width = pixels(340, dpi).min(work_area(source)?.width() - pixels(8, dpi));
        let frame = Rc::new(Frame::build(view, dpi, width, now)?);
        let visible = self.state.visible();
        if unsafe { SetWindowTextW(self.window, wide(&frame.text).as_ptr()) } == 0 {
            return Err(WireError::InvalidState);
        }
        *self.state.frame.borrow_mut() = Some(frame);
        if visible {
            self.position(false)?;
            self.state.scroll_by(0, false);
            self.state.redraw();
        }
        Ok(())
    }
    fn position(&self, show: bool) -> Result<(), WireError> {
        let frame = self
            .state
            .frame
            .borrow()
            .clone()
            .ok_or(WireError::InvalidState)?;
        let anchor = window_rect(self.state.source.get())?;
        if unsafe { GetDpiForWindow(self.state.source.get()) } != frame.dpi {
            return Err(WireError::InvalidState);
        }
        let area = placement(
            anchor,
            work_area(self.state.source.get())?,
            frame.layout.plan.width,
            frame.layout.plan.height,
            frame.dpi,
        )?;
        self.state.anchor.set(anchor);
        let region = unsafe {
            CreateRoundRectRgn(
                0,
                0,
                area.width() + 1,
                area.height() + 1,
                pixels(8, frame.dpi),
                pixels(8, frame.dpi),
            )
        };
        if region.is_null() {
            return Err(WireError::InvalidState);
        }
        if unsafe { SetWindowRgn(self.window, region, 0) } == 0 {
            unsafe {
                DeleteObject(region);
            }
            return Err(WireError::InvalidState);
        }
        if unsafe {
            SetWindowPos(
                self.window,
                HWND_TOPMOST,
                area.left,
                area.top,
                area.width(),
                area.height(),
                SWP_NOACTIVATE | SWP_NOOWNERZORDER | if show { SWP_SHOWWINDOW } else { 0 },
            )
        } == 0
        {
            return Err(WireError::InvalidState);
        }
        if unsafe { GetDpiForWindow(self.window) } != frame.dpi {
            self.state.hide();
            return Err(WireError::InvalidState);
        }
        Ok(())
    }
    pub(crate) fn show(&self, keyboard: bool) -> Result<(), WireError> {
        if self.state.frame.borrow().is_none() {
            return Ok(());
        }
        if !self.state.alive.get() || unsafe { IsWindowVisible(self.state.source.get()) } == 0 {
            return Err(WireError::Closed);
        }
        self.state.keyboard.set(keyboard);
        self.state.outside_since.set(None);
        if let Err(error) = self.position(true) {
            self.state.hide();
            return Err(error);
        }
        self.state.redraw();
        if unsafe { SetTimer(self.window, WATCH, 50, None) } == 0 {
            self.state.hide();
            return Err(WireError::InvalidState);
        }
        Ok(())
    }
    pub(crate) fn hide(&self) {
        self.state.hide();
    }
    pub(crate) fn leave(&self) {
        self.state.keyboard.set(false);
        self.state.watch();
    }
    pub(crate) fn visible(&self) -> bool {
        self.state.visible()
    }
    pub(crate) fn failed(&self) -> bool {
        self.state.failed.get()
    }
    pub(crate) fn scroll_key(&self, key: u16) -> bool {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            VK_DOWN, VK_END, VK_HOME, VK_NEXT, VK_PRIOR, VK_UP,
        };
        if !self.visible() {
            return false;
        }
        let frame = self.state.frame.borrow().clone();
        let Some(frame) = frame else {
            return false;
        };
        let mut client: RECT = unsafe { mem::zeroed() };
        if unsafe { GetClientRect(self.window, &mut client) } == 0 {
            return false;
        }
        let (amount, absolute) = match key {
            VK_DOWN => (frame.font.height() * 3, false),
            VK_UP => (-frame.font.height() * 3, false),
            VK_NEXT => (client.bottom - frame.font.height(), false),
            VK_PRIOR => (-client.bottom + frame.font.height(), false),
            VK_HOME => (0, true),
            VK_END => (i32::MAX, true),
            _ => return false,
        };
        self.state.scroll_by(amount, absolute);
        true
    }
    pub(crate) fn wheel(&self, wparam: WPARAM, lparam: LPARAM) {
        unsafe {
            procedure(self.window, WM_MOUSEWHEEL, wparam, lparam);
        }
    }
    pub(crate) fn clear(&self) -> Result<(), WireError> {
        self.state.hide();
        self.state.frame.borrow_mut().take();
        self.state.scroll.set(0);
        self.state.wheel.set(0);
        if !self.state.alive.get() {
            return Ok(());
        }
        if unsafe { SetWindowTextW(self.window, wide("TokenPulse 详情已关闭").as_ptr()) } == 0
        {
            return Err(WireError::InvalidState);
        }
        // A clear cannot acknowledge while an older immutable paint frame is in use.
        if self.state.painting.get() {
            self.state.failed.set(true);
            return Err(WireError::InvalidState);
        }
        let mut client: RECT = unsafe { mem::zeroed() };
        if unsafe { GetClientRect(self.window, &mut client) } != 0
            && (client.right == 0 || client.bottom == 0)
        {
            return Ok(());
        }
        self.state.redraw();
        let dc = unsafe { GetDC(self.window) };
        if dc.is_null() {
            return Err(WireError::InvalidState);
        }
        unsafe {
            procedure(self.window, WM_PRINTCLIENT, dc as usize, 0);
            ReleaseDC(self.window, dc);
        }
        if unsafe { windows_sys::Win32::Graphics::Gdi::GdiFlush() } == 0 || self.failed() {
            return Err(WireError::InvalidState);
        }
        Ok(())
    }
}
impl Drop for NativeDetails {
    fn drop(&mut self) {
        self.clear().ok();
        if self.state.alive.get() {
            unsafe {
                DestroyWindow(self.window);
            }
        }
        unsafe {
            UnregisterClassW(self.class.as_ptr(), GetModuleHandleW(ptr::null()));
        }
    }
}

#[cfg(test)]
pub(super) fn fixture() -> TaskbarView {
    use token_pulse_core::{
        numeric::{DecimalInt, EpochMs},
        protocol::DateRange,
    };
    let mut view: TaskbarView =
        serde_json::from_str(include_str!("../../../../fixtures/taskbar-display.json")).unwrap();
    view.details = Some(crate::HostDetails {
        theme: AppTheme::Dark,
        range: DateRange {
            start_ms: EpochMs::new(1790899200000).unwrap(),
            end_ms: EpochMs::new(1790985600000).unwrap(),
            timezone: view.timezone.clone(),
        },
        scope: crate::HostScope::FixedSession,
        source_last_success_at_ms: Some(view.generated_at_ms),
        source_statuses: vec![crate::HostSourceStatus::Unreadable],
        pending_observations: DecimalInt::parse("9007199254740993").unwrap(),
        pending_files: DecimalInt::parse("1").unwrap(),
        breakdown_complete: false,
        input_complete: false,
        cached_complete: true,
        output_complete: false,
        pricing_calculating: false,
    });
    view
}

#[cfg(test)]
mod tests {
    use super::super::topology::DpiGuard;
    use super::*;
    use windows_sys::Win32::UI::{
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{VK_END, VK_HOME, VK_NEXT},
        WindowsAndMessaging::{
            GWL_EXSTYLE, GetForegroundWindow, GetWindowLongW, GetWindowTextW, IsWindow,
            SW_SHOWNOACTIVATE,
        },
    };
    #[test]
    fn placement_stays_in_negative_monitor_work_area_and_outside_taskbar_at_four_dpis() {
        for dpi in [96, 120, 144, 192] {
            let work = ScreenRect {
                left: -2560,
                top: -1440,
                right: 0,
                bottom: 0,
            };
            let anchor = ScreenRect {
                left: -350,
                top: 0,
                right: -10,
                bottom: 60,
            };
            let placed = placement(anchor, work, pixels(340, dpi), pixels(1000, dpi), dpi).unwrap();
            assert_eq!(placed.height(), pixels(600, dpi));
            assert!(placed.left >= work.left && placed.right <= work.right);
            assert!(placed.top >= work.top && placed.bottom <= anchor.top - pixels(8, dpi));
            let top = ScreenRect {
                top: -1500,
                bottom: -1440,
                ..anchor
            };
            let below = placement(top, work, pixels(340, dpi), pixels(1000, dpi), dpi).unwrap();
            assert_eq!(below.top, work.top + pixels(8, dpi));
            assert!(below.bottom <= work.bottom);
            assert!(placement(anchor, work, 3000, 100, dpi).is_err());
            assert!(
                placement(
                    anchor,
                    ScreenRect {
                        left: -100,
                        top: -20,
                        right: 0,
                        bottom: 0
                    },
                    50,
                    100,
                    dpi
                )
                .is_err()
            );
        }
    }
    #[test]
    fn real_system_font_preserves_every_character_and_complete_values_at_four_dpis() {
        use token_pulse_core::numeric::DecimalInt;
        let mut view = fixture();
        view.total_tokens = Some(DecimalInt::parse("9007199254740993").unwrap());
        for dpi in [96, 120, 144, 192] {
            let frame =
                Frame::build(&view, dpi, pixels(340, dpi), view.generated_at_ms.value()).unwrap();
            for span in &frame.layout.plan.spans {
                assert!(
                    span.x >= 0 && span.x + span.width <= frame.layout.plan.width - pixels(12, dpi)
                );
                assert!(span.y >= 0 && span.y + frame.font.height() <= frame.layout.plan.height);
            }
            let values: String = frame
                .layout
                .plan
                .spans
                .iter()
                .filter(|s| s.x == pixels(124, dpi))
                .map(|s| s.span.text.as_str())
                .collect();
            assert!(values.contains("9007199254740993"));
            assert!(values.contains("0.565000000000001"));
            assert!(values.contains("SYNTHETIC DEVELOPMENT FIXTURE"));
            assert!(values.contains("—（未提供）"));
            let original = "中文很长的会话名称🙂 / C:\\SYNTHETIC\\project 名称 9007199254740993";
            let lines = wrap(original, pixels(70, dpi), &|s| frame.font.width(s)).unwrap();
            assert!(lines.len() > 2);
            assert_eq!(lines.concat(), original);
            assert!(
                lines
                    .iter()
                    .all(|s| frame.font.width(s).unwrap() <= pixels(70, dpi))
            );
            assert!(wrap("中", 1, &|s| frame.font.width(s)).is_err());
        }
    }
    #[test]
    fn native_pixels_include_real_zero_bar_exclude_unknown_and_privacy_and_render_themes() {
        let mut view = fixture();
        let quota = view.quota.as_mut().unwrap();
        quota.windows[0].remaining_percent = Some(0.0);
        quota.windows[0].used_percent = Some(100.0);
        quota.windows[1].remaining_percent = None;
        quota.windows[1].used_percent = None;
        for dpi in [96, 120, 144, 192] {
            for theme in [AppTheme::Dark, AppTheme::Light] {
                view.details.as_mut().unwrap().theme = theme;
                let frame =
                    Frame::build(&view, dpi, pixels(340, dpi), view.generated_at_ms.value())
                        .unwrap();
                assert_eq!(frame.layout.bars.len(), 1);
                assert_eq!(frame.layout.bars[0].percent, 0.0);
                let height = pixels(600, dpi).min(frame.layout.plan.height);
                let bmp = frame
                    .font
                    .bitmap_with(frame.layout.plan.width, height, |dc| unsafe {
                        frame.paint(dc, frame.layout.plan.width, height, 0)
                    })
                    .unwrap();
                assert!(bmp[54..].chunks_exact(4).any(|p| p[..3]
                    != [
                        ((frame.palette.background >> 16) & 255) as u8,
                        ((frame.palette.background >> 8) & 255) as u8,
                        (frame.palette.background & 255) as u8
                    ]));
                if let Ok(path) = std::env::var("TOKENPULSE_DETAILS_VISUAL_DIR") {
                    std::fs::create_dir_all(&path).unwrap();
                    std::fs::write(
                        std::path::Path::new(&path).join(format!("details-{theme:?}-{dpi}.bmp")),
                        bmp,
                    )
                    .unwrap();
                }
            }
        }
        view.privacy = true;
        view.scope_label = None;
        view.costs.clear();
        view.quota = None;
        let frame = Frame::build(&view, 144, 510, view.generated_at_ms.value()).unwrap();
        assert!(frame.layout.bars.is_empty());
        assert!(!frame.text.contains("SYNTHETIC"));
        assert!(!frame.text.contains("0.565"));
        assert!(!frame.text.contains("剩余"));
        if let Ok(path) = std::env::var("TOKENPULSE_DETAILS_VISUAL_DIR") {
            let bmp = frame
                .font
                .bitmap_with(510, 900, |dc| unsafe { frame.paint(dc, 510, 900, 0) })
                .unwrap();
            std::fs::write(
                std::path::Path::new(&path).join("details-privacy-144.bmp"),
                bmp,
            )
            .unwrap();
        }
    }
    #[test]
    fn actual_nonactivating_popup_refresh_scroll_failure_clear_pixels_and_owner_destruction() {
        let _dpi = DpiGuard::enter().unwrap();
        let source = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                wide("STATIC").as_ptr(),
                wide("TokenPulse SYNTHETIC owned details test").as_ptr(),
                WS_POPUP,
                200,
                200,
                500,
                60,
                ptr::null_mut(),
                ptr::null_mut(),
                GetModuleHandleW(ptr::null()),
                ptr::null(),
            )
        };
        assert!(!source.is_null());
        unsafe {
            ShowWindow(source, SW_SHOWNOACTIVATE);
        }
        let details = NativeDetails::create(source).unwrap();
        let dpi = unsafe { GetDpiForWindow(source) };
        let mut view = fixture();
        details
            .prepare(source, &view, dpi, view.generated_at_ms.value())
            .unwrap();
        assert!(!details.visible());
        let before = unsafe { GetForegroundWindow() };
        details.show(true).unwrap();
        assert!(details.visible());
        assert_eq!(unsafe { GetForegroundWindow() }, before);
        // Null foreground on this desktop does not count as physical focus acceptance.
        assert_eq!(
            unsafe { GetWindowLongW(details.window, GWL_EXSTYLE) } as u32
                & (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW),
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW
        );
        assert_eq!(
            unsafe { procedure(details.window, WM_MOUSEACTIVATE, 0, 0) },
            MA_NOACTIVATE as isize
        );
        assert!(details.scroll_key(VK_END));
        assert!(details.state.scroll.get() > 0);
        assert!(details.scroll_key(VK_NEXT));
        let end = details.state.scroll.get();
        details.scroll_key(VK_END);
        assert_eq!(details.state.scroll.get(), end);
        details.scroll_key(VK_HOME);
        assert_eq!(details.state.scroll.get(), 0);
        details.wheel(((0xff88u32) << 16) as usize, 0);
        assert!(details.state.scroll.get() > 0);
        details
            .prepare(source, &view, dpi, view.generated_at_ms.value())
            .unwrap();
        assert!(details.visible());
        let mut caption = vec![0; 16000];
        let n =
            unsafe { GetWindowTextW(details.window, caption.as_mut_ptr(), caption.len() as i32) };
        assert!(
            String::from_utf16_lossy(&caption[..n as usize])
                .contains("SYNTHETIC DEVELOPMENT BUCKET")
        );
        details.clear().unwrap();
        assert!(!details.visible());
        assert!(details.state.frame.borrow().is_none());
        let n =
            unsafe { GetWindowTextW(details.window, caption.as_mut_ptr(), caption.len() as i32) };
        assert_eq!(
            String::from_utf16_lossy(&caption[..n as usize]),
            "TokenPulse 详情已关闭"
        );
        let client = window_rect(details.window).unwrap();
        let font = NativeFont::new(dpi).unwrap();
        let bmp = font
            .bitmap_with(client.width(), client.height(), |dc| {
                unsafe {
                    procedure(details.window, WM_PRINTCLIENT, dc as usize, 0);
                }
                Ok(())
            })
            .unwrap();
        assert!(bmp[54..].chunks_exact(4).all(|p| p[..3] == [31, 25, 22]));
        details
            .prepare(source, &view, dpi, view.generated_at_ms.value())
            .unwrap();
        assert!(!details.visible());
        details.show(true).unwrap();
        view.timezone = "invalid".into();
        assert!(details.prepare(source, &view, dpi, 0).is_err());
        assert!(!details.visible());
        assert!(details.state.frame.borrow().is_none());
        unsafe {
            DestroyWindow(source);
        }
        assert_eq!(unsafe { IsWindow(details.window) }, 0);
        assert!(!details.state.alive.get());
        details.clear().unwrap();
    }
}
