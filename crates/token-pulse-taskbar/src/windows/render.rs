//! System-font measurement and GDI drawing. No screenshots of other applications or file I/O.
use crate::{
    TaskbarView, WireError,
    display::{DisplayLayout, DisplayPreferences, MeasureBounds, MeasuredPlan, Tone, measure},
};
use std::{mem, ptr};
#[cfg(test)]
thread_local! {
    // Observe actual successful layered submissions, including transient empty frames.
    pub(crate) static PRESENTED_FRAMES: std::cell::RefCell<Vec<bool>> = const { std::cell::RefCell::new(Vec::new()) };
}
use windows_sys::Win32::{
    Foundation::{COLORREF, RECT, SIZE},
    Graphics::Gdi::{
        ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, COLOR_WINDOW,
        COLOR_WINDOWTEXT, CreateCompatibleBitmap, CreateCompatibleDC, CreateDIBSection,
        CreateFontIndirectW, CreateSolidBrush, DEFAULT_CHARSET, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, FillRect, GdiFlush, GetClipBox, GetSysColor, GetTextExtentPoint32W,
        GetTextFaceW, GetTextMetricsW, HDC, HFONT, HGDIOBJ, IntersectClipRect, NULLREGION,
        RestoreDC, SRCCOPY, SaveDC, SelectObject, SetBkMode, SetTextColor, TEXTMETRICW,
        TRANSPARENT, TextOutW,
    },
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        HiDpi::SystemParametersInfoForDpi,
        WindowsAndMessaging::{
            NONCLIENTMETRICSW, SPI_GETHIGHCONTRAST, SPI_GETNONCLIENTMETRICS, SystemParametersInfoW,
        },
    },
};
pub fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub background: COLORREF,
    pub foreground: COLORREF,
    pub muted: COLORREF,
    pub cost: COLORREF,
    pub warning: COLORREF,
    pub accent: COLORREF,
}
impl Palette {
    pub fn for_background(background: COLORREF) -> Self {
        let light = (background & 255) * 299
            + ((background >> 8) & 255) * 587
            + ((background >> 16) & 255) * 114
            > 128_000;
        if light {
            Self {
                background,
                foreground: rgb(24, 24, 28),
                muted: rgb(80, 84, 88),
                cost: rgb(16, 115, 64),
                warning: rgb(135, 72, 0),
                accent: rgb(0, 83, 190),
            }
        } else {
            Self {
                background,
                foreground: rgb(241, 244, 248),
                muted: rgb(183, 192, 204),
                cost: rgb(85, 220, 157),
                warning: rgb(255, 201, 107),
                accent: rgb(107, 172, 255),
            }
        }
    }
    pub fn system(background: COLORREF) -> Result<Self, WireError> {
        let mut contrast: HIGHCONTRASTW = unsafe { mem::zeroed() };
        contrast.cbSize = mem::size_of_val(&contrast) as u32;
        if unsafe {
            SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                contrast.cbSize,
                (&mut contrast as *mut HIGHCONTRASTW).cast(),
                0,
            )
        } == 0
        {
            return Err(WireError::InvalidState);
        }
        if contrast.dwFlags & HCF_HIGHCONTRASTON != 0 {
            let foreground = unsafe { GetSysColor(COLOR_WINDOWTEXT) };
            Ok(Self {
                background: unsafe { GetSysColor(COLOR_WINDOW) },
                foreground,
                muted: foreground,
                cost: foreground,
                warning: foreground,
                accent: foreground,
            })
        } else {
            Ok(Self::for_background(background))
        }
    }
    fn tone(self, tone: Tone) -> COLORREF {
        match tone {
            Tone::Normal => self.foreground,
            Tone::Cost => self.cost,
            Tone::Warning => self.warning,
            Tone::Muted => self.muted,
        }
    }
}
struct Object(HGDIOBJ);
impl Drop for Object {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.0);
        }
    }
}
struct Dc(HDC);
impl Drop for Dc {
    fn drop(&mut self) {
        unsafe {
            DeleteDC(self.0);
        }
    }
}
/// Complete an opaque popup frame offscreen before copying it to the visible DC.
pub(crate) unsafe fn paint_buffered(
    dc: HDC,
    width: i32,
    height: i32,
    paint: impl FnOnce(HDC) -> Result<(), WireError>,
) -> Result<(), WireError> {
    if width <= 0 || height <= 0 {
        return Ok(());
    }
    let buffer = Dc(unsafe { CreateCompatibleDC(dc) });
    if buffer.0.is_null() {
        return Err(WireError::InvalidState);
    }
    let bitmap = Object(unsafe { CreateCompatibleBitmap(dc, width, height) });
    if bitmap.0.is_null() {
        return Err(WireError::InvalidState);
    }
    let previous = unsafe { SelectObject(buffer.0, bitmap.0) };
    if previous.is_null() || previous as isize == -1 {
        return Err(WireError::InvalidState);
    }
    let result = paint(buffer.0).and_then(|()| {
        if unsafe { BitBlt(dc, 0, 0, width, height, buffer.0, 0, 0, SRCCOPY) } == 0 {
            Err(WireError::InvalidState)
        } else {
            Ok(())
        }
    });
    unsafe {
        SelectObject(buffer.0, previous);
    }
    result
}
pub struct NativeFont {
    dc: Dc,
    font: Object,
    previous: HGDIOBJ,
    height: i32,
    dpi: u32,
}
impl NativeFont {
    pub fn new(dpi: u32) -> Result<Self, WireError> {
        Self::with_height(dpi, None)
    }
    pub(crate) fn for_details(dpi: u32) -> Result<Self, WireError> {
        Self::with_faces(
            dpi,
            &["Segoe UI Variable Text", "Segoe UI", "Microsoft YaHei UI"],
        )
    }
    pub(crate) fn for_data(dpi: u32) -> Result<Self, WireError> {
        // Same numeric typeface as --dataFont in ui/src/shared/silver-mist.css.
        Self::with_faces(dpi, &["Bahnschrift", "Segoe UI"])
    }
    fn with_faces(dpi: u32, faces: &[&str]) -> Result<Self, WireError> {
        // Match ui/src/shared/silver-mist.css. GDI can silently substitute a
        // missing face, so check the selected font before accepting a candidate.
        for &face in faces {
            let font = Self::with_typeface(dpi, None, Some(face))?;
            let mut selected = [0u16; 32];
            let length =
                unsafe { GetTextFaceW(font.dc.0, selected.len() as i32, selected.as_mut_ptr()) };
            if length == 0 {
                return Err(WireError::InvalidState);
            }
            let end = selected
                .iter()
                .position(|&ch| ch == 0)
                .unwrap_or(selected.len());
            if String::from_utf16_lossy(&selected[..end]).eq_ignore_ascii_case(face) {
                return Ok(font);
            }
        }
        Self::new(dpi)
    }
    pub(crate) fn for_taskbar(
        dpi: u32,
        prefs: DisplayPreferences,
        available_height: i32,
    ) -> Result<Self, WireError> {
        let system = Self::new(dpi)?;
        if prefs.layout != DisplayLayout::TwoRows {
            return Ok(system);
        }
        let line_height = (available_height - (6 * dpi / 96) as i32) / 2;
        if system.height <= line_height {
            return Ok(system);
        }
        // Keep the system typeface and measure every candidate. Stop at 8 pt;
        // genuinely short taskbars still use the existing readable fallback.
        let minimum = (8 * dpi / 72) as i32;
        for height in (minimum..system.height).rev() {
            let font = Self::with_height(dpi, Some(height))?;
            if font.height <= line_height {
                return Ok(font);
            }
        }
        Ok(system)
    }
    fn with_height(dpi: u32, height: Option<i32>) -> Result<Self, WireError> {
        Self::with_typeface(dpi, height, None)
    }
    fn with_typeface(dpi: u32, height: Option<i32>, face: Option<&str>) -> Result<Self, WireError> {
        if !(96..=768).contains(&dpi) {
            return Err(WireError::InvalidFrame);
        }
        let mut metrics: NONCLIENTMETRICSW = unsafe { mem::zeroed() };
        metrics.cbSize = mem::size_of_val(&metrics) as u32;
        if unsafe {
            SystemParametersInfoForDpi(
                SPI_GETNONCLIENTMETRICS,
                metrics.cbSize,
                (&mut metrics as *mut NONCLIENTMETRICSW).cast(),
                0,
                dpi,
            )
        } == 0
        {
            return Err(WireError::InvalidState);
        }
        // Grayscale coverage can be composited over the real taskbar without
        // ClearType fringes that were calculated against an opaque background.
        metrics.lfMessageFont.lfQuality = ANTIALIASED_QUALITY;
        if let Some(face) = face {
            metrics.lfMessageFont.lfFaceName.fill(0);
            for (target, ch) in metrics
                .lfMessageFont
                .lfFaceName
                .iter_mut()
                .zip(face.encode_utf16())
            {
                *target = ch;
            }
            metrics.lfMessageFont.lfWeight = 400;
            metrics.lfMessageFont.lfItalic = 0;
            metrics.lfMessageFont.lfCharSet = DEFAULT_CHARSET;
        }
        if let Some(height) = height {
            metrics.lfMessageFont.lfHeight = -height;
        }
        let font = unsafe { CreateFontIndirectW(&metrics.lfMessageFont) };
        if font.is_null() {
            return Err(WireError::InvalidState);
        }
        let font = Object(font);
        let dc = unsafe { CreateCompatibleDC(ptr::null_mut()) };
        if dc.is_null() {
            return Err(WireError::InvalidState);
        }
        let dc = Dc(dc);
        let previous = unsafe { SelectObject(dc.0, font.0) };
        if previous.is_null() || previous as isize == -1 {
            return Err(WireError::InvalidState);
        }
        let mut text: TEXTMETRICW = unsafe { mem::zeroed() };
        if unsafe { GetTextMetricsW(dc.0, &mut text) } == 0 {
            unsafe {
                SelectObject(dc.0, previous);
            }
            return Err(WireError::InvalidState);
        }
        Ok(Self {
            dc,
            font,
            previous,
            height: text.tmHeight,
            dpi,
        })
    }
    pub fn height(&self) -> i32 {
        self.height
    }
    /// Draw a menu label with the measured font, preserving the caller's DC.
    pub(crate) unsafe fn text(
        &self,
        dc: HDC,
        x: i32,
        y: i32,
        text: &str,
        color: COLORREF,
    ) -> Result<(), WireError> {
        let saved = unsafe { SaveDC(dc) };
        if saved == 0 {
            return Err(WireError::InvalidState);
        }
        let result = (|| {
            let previous = unsafe { SelectObject(dc, self.font.0) };
            if previous.is_null() || previous as isize == -1 {
                return Err(WireError::InvalidState);
            }
            let text: Vec<_> = text.encode_utf16().collect();
            unsafe {
                SetBkMode(dc, TRANSPARENT as i32);
                SetTextColor(dc, color);
            }
            if unsafe { TextOutW(dc, x, y, text.as_ptr(), text.len() as i32) } == 0 {
                return Err(WireError::InvalidState);
            }
            Ok(())
        })();
        unsafe { RestoreDC(dc, saved) };
        result
    }
    pub fn width(&self, text: &str) -> Result<i32, WireError> {
        if text.chars().count() > 4096 {
            return Err(WireError::TooLarge);
        }
        let text: Vec<_> = text.encode_utf16().collect();
        let mut size: SIZE = unsafe { mem::zeroed() };
        if unsafe { GetTextExtentPoint32W(self.dc.0, text.as_ptr(), text.len() as i32, &mut size) }
            == 0
        {
            return Err(WireError::InvalidState);
        }
        Ok(size.cx)
    }
    pub fn plan(
        &self,
        view: &TaskbarView,
        prefs: DisplayPreferences,
        now: i64,
        width: i32,
        height: i32,
    ) -> Result<Option<MeasuredPlan>, WireError> {
        measure(
            view,
            prefs,
            now,
            MeasureBounds {
                dpi: self.dpi,
                font_height: self.height,
                width,
                height,
            },
            |text| self.width(text),
        )
    }
    /// The HDC is borrowed from this window's paint cycle; caller owns its lifetime/thread.
    pub(crate) unsafe fn paint(
        &self,
        dc: HDC,
        plan: Option<&MeasuredPlan>,
        width: i32,
        height: i32,
        palette: Palette,
    ) -> Result<(), WireError> {
        unsafe { self.paint_mode(dc, plan, width, height, palette, true, None) }
    }
    pub(crate) unsafe fn paint_mode(
        &self,
        dc: HDC,
        plan: Option<&MeasuredPlan>,
        width: i32,
        height: i32,
        palette: Palette,
        marker: bool,
        data_font: Option<(&Self, &[usize])>,
    ) -> Result<(), WireError> {
        let saved = unsafe { SaveDC(dc) };
        if saved == 0 {
            acceptance_trace("save_dc", dc);
            return Err(WireError::InvalidState);
        }
        let result = (|| {
            if unsafe { IntersectClipRect(dc, 0, 0, width, height) } == 0 {
                return Err(WireError::InvalidState);
            }
            // A hidden/detached child has a valid DC but no visible pixels. FillRect can
            // return zero in that state. Clear the cached plan/caption at the canvas layer,
            // then repaint the latest plan when attached; never classify an empty region
            // as device failure or discard actual DC errors.
            if !unsafe { has_visible_clip(dc) }? {
                return Ok(());
            }
            let brush = unsafe { CreateSolidBrush(palette.background) };
            if brush.is_null() {
                acceptance_trace("create_brush", dc);
                return Err(WireError::InvalidState);
            }
            let brush = Object(brush);
            let rect = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            if unsafe { FillRect(dc, &rect, brush.0) } == 0 {
                acceptance_trace("fill_background", dc);
                return Err(WireError::InvalidState);
            }
            if let Some(plan) = plan {
                let selected = unsafe { SelectObject(dc, self.font.0 as HFONT) };
                if selected.is_null() || selected as isize == -1 {
                    return Err(WireError::InvalidState);
                }
                unsafe {
                    SetBkMode(dc, TRANSPARENT as i32);
                }
                for (index, placed) in plan.spans.iter().enumerate() {
                    let face = data_font
                        .filter(|(_, indices)| indices.binary_search(&index).is_ok())
                        .map(|(font, _)| font)
                        .unwrap_or(self);
                    let selected = unsafe { SelectObject(dc, face.font.0) };
                    if selected.is_null() || selected as isize == -1 {
                        return Err(WireError::InvalidState);
                    }
                    let text: Vec<_> = placed.span.text.encode_utf16().collect();
                    unsafe {
                        SetTextColor(dc, palette.tone(placed.span.tone));
                    }
                    if unsafe { TextOutW(dc, placed.x, placed.y, text.as_ptr(), text.len() as i32) }
                        == 0
                    {
                        return Err(WireError::InvalidState);
                    }
                }
                if marker {
                    let dot = (3 * self.dpi / 96) as i32;
                    let x = (3 * self.dpi / 96) as i32;
                    let top = (height - dot) / 2;
                    let marker = unsafe { CreateSolidBrush(palette.accent) };
                    if marker.is_null() {
                        return Err(WireError::InvalidState);
                    }
                    let marker = Object(marker);
                    let rect = RECT {
                        left: x,
                        top,
                        right: x + dot,
                        bottom: top + dot,
                    };
                    if unsafe { FillRect(dc, &rect, marker.0) } == 0 {
                        return Err(WireError::InvalidState);
                    }
                }
            }
            Ok(())
        })();
        unsafe {
            RestoreDC(dc, saved);
        }
        result
    }
    /// Render only this application's prepared text into a bounded bitmap for visual checks.
    /// No screen capture, window enumeration, account read or file write occurs here.
    pub fn bitmap(
        &self,
        plan: Option<&MeasuredPlan>,
        width: i32,
        height: i32,
        palette: Palette,
    ) -> Result<Vec<u8>, WireError> {
        self.bitmap_with(width, height, |dc| unsafe {
            self.paint(dc, plan, width, height, palette)
        })
    }
    /// Present a per-pixel surface containing only our glyphs and focus marker.
    /// A 1/255 input surface preserves clicks in the spacing between glyphs;
    /// a fully zero-alpha surface would pass those clicks to Explorer.
    pub(crate) fn present(
        &self,
        window: windows_sys::Win32::Foundation::HWND,
        plan: Option<&MeasuredPlan>,
        width: i32,
        height: i32,
        palette: Palette,
        focused: bool,
    ) -> Result<(), WireError> {
        use windows_sys::Win32::{
            Foundation::{POINT, SIZE},
            Graphics::Gdi::{AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION, DrawFocusRect},
            UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow},
        };
        let white = rgb(255, 255, 255);
        let mask = self.bitmap_with(width, height, |dc| unsafe {
            self.paint(
                dc,
                plan,
                width,
                height,
                Palette {
                    background: 0,
                    foreground: white,
                    muted: white,
                    cost: white,
                    warning: white,
                    accent: white,
                },
            )?;
            if focused {
                DrawFocusRect(
                    dc,
                    &RECT {
                        left: 1,
                        top: 1,
                        right: width - 1,
                        bottom: height - 1,
                    },
                );
            }
            Ok(())
        })?;
        let mut pixels = mask[54..].to_vec();
        // Span bounds come from the same system font measurement, not guessed
        // character widths. A pixel outside a span belongs to our marker/focus.
        let bounds = plan
            .map(|plan| {
                plan.spans
                    .iter()
                    .map(|span| {
                        Ok((
                            span.x,
                            span.y,
                            span.x + self.width(&span.span.text)?,
                            span.y + self.height,
                            palette.tone(span.span.tone),
                        ))
                    })
                    .collect::<Result<Vec<_>, WireError>>()
            })
            .transpose()?
            .unwrap_or_default();
        for (index, pixel) in pixels.chunks_exact_mut(4).enumerate() {
            let x = index as i32 % width;
            let y = index as i32 / width;
            let color = bounds
                .iter()
                .find(|&&(left, top, right, bottom, _)| {
                    x >= left && x < right && y >= top && y < bottom
                })
                .map(|bound| bound.4)
                .unwrap_or(palette.accent);
            let coverage = pixel[0].max(pixel[1]).max(pixel[2]);
            pixel.copy_from_slice(&premultiplied_pixel(color, coverage));
        }
        let dc = Dc(unsafe { CreateCompatibleDC(ptr::null_mut()) });
        if dc.0.is_null() {
            return Err(WireError::InvalidState);
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..unsafe { mem::zeroed() }
            },
            ..unsafe { mem::zeroed() }
        };
        let mut bits = ptr::null_mut();
        let bitmap = Object(unsafe {
            CreateDIBSection(dc.0, &info, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0)
        });
        if bitmap.0.is_null() || bits.is_null() {
            return Err(WireError::InvalidState);
        }
        unsafe {
            ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), pixels.len());
        }
        let previous = unsafe { SelectObject(dc.0, bitmap.0) };
        if previous.is_null() || previous as isize == -1 {
            return Err(WireError::InvalidState);
        }
        let size = SIZE {
            cx: width,
            cy: height,
        };
        let source = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let ok = unsafe {
            UpdateLayeredWindow(
                window,
                ptr::null_mut(),
                ptr::null(),
                &size,
                dc.0,
                &source,
                0,
                &blend,
                ULW_ALPHA,
            )
        } != 0;
        unsafe {
            SelectObject(dc.0, previous);
        }
        if ok {
            #[cfg(test)]
            PRESENTED_FRAMES.with(|frames| frames.borrow_mut().push(plan.is_some()));
            Ok(())
        } else {
            Err(WireError::InvalidState)
        }
    }
    pub(crate) fn bitmap_with(
        &self,
        width: i32,
        height: i32,
        paint: impl FnOnce(HDC) -> Result<(), WireError>,
    ) -> Result<Vec<u8>, WireError> {
        if !(1..=4096).contains(&width)
            || !(1..=2048).contains(&height)
            || i64::from(width) * i64::from(height) > 1_048_576
        {
            return Err(WireError::TooLarge);
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..unsafe { mem::zeroed() }
            },
            ..unsafe { mem::zeroed() }
        };
        let mut pixels = ptr::null_mut();
        let bitmap = unsafe {
            CreateDIBSection(
                self.dc.0,
                &info,
                DIB_RGB_COLORS,
                &mut pixels,
                ptr::null_mut(),
                0,
            )
        };
        if bitmap.is_null() || pixels.is_null() {
            return Err(WireError::InvalidState);
        }
        let bitmap = Object(bitmap);
        let previous = unsafe { SelectObject(self.dc.0, bitmap.0) };
        if previous.is_null() || previous as isize == -1 {
            return Err(WireError::InvalidState);
        }
        let result = paint(self.dc.0);
        let flushed = unsafe { GdiFlush() } != 0;
        let payload = if result.is_ok() {
            unsafe {
                std::slice::from_raw_parts(pixels.cast::<u8>(), (width * height * 4) as usize)
            }
            .to_vec()
        } else {
            vec![]
        };
        unsafe {
            SelectObject(self.dc.0, previous);
        }
        result?;
        if !flushed {
            return Err(WireError::InvalidState);
        }
        let mut output = vec![];
        output.extend_from_slice(b"BM");
        output.extend_from_slice(&((54 + payload.len()) as u32).to_le_bytes());
        output.extend_from_slice(&[0; 4]);
        output.extend_from_slice(&54u32.to_le_bytes());
        output.extend_from_slice(&40u32.to_le_bytes());
        output.extend_from_slice(&width.to_le_bytes());
        output.extend_from_slice(&(-height).to_le_bytes());
        output.extend_from_slice(&1u16.to_le_bytes());
        output.extend_from_slice(&32u16.to_le_bytes());
        output.extend_from_slice(&[0; 24]);
        output.extend_from_slice(&payload);
        Ok(output)
    }
}
fn premultiplied_pixel(color: COLORREF, coverage: u8) -> [u8; 4] {
    if coverage == 0 {
        return [0, 0, 0, 1];
    }
    let channel = |shift: u32| (((color >> shift) & 255_u32) * u32::from(coverage) / 255) as u8;
    [channel(16), channel(8), channel(0), coverage]
}
/// The DC is borrowed on its owning paint thread. Empty visibility is a valid no-op;
/// an invalid DC remains an error. Used by both the readout and its hidden details.
pub(crate) unsafe fn has_visible_clip(dc: HDC) -> Result<bool, WireError> {
    let mut clip: RECT = unsafe { mem::zeroed() };
    match unsafe { GetClipBox(dc, &mut clip) } {
        0 => Err(WireError::InvalidState),
        NULLREGION => Ok(false),
        _ => Ok(true),
    }
}
fn acceptance_trace(stage: &str, dc: HDC) {
    #[cfg(debug_assertions)]
    if std::env::var_os("TOKENPULSE_ACCEPTANCE_HOST_DIAGNOSTICS").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        let error = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        let mut clip: RECT = unsafe { mem::zeroed() };
        let kind = unsafe { windows_sys::Win32::Graphics::Gdi::GetClipBox(dc, &mut clip) };
        eprintln!(
            "NATIVE_GDI_FAILED: {stage} last_error={error} clip_type={kind} clip={},{},{},{}",
            clip.left, clip.top, clip.right, clip.bottom
        );
    }
    #[cfg(not(debug_assertions))]
    let _ = (stage, dc);
}
impl Drop for NativeFont {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc.0, self.previous);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Graphics::Gdi::{CreateRectRgn, SelectClipRgn};

    #[test]
    fn two_rows_fit_standard_taskbar_heights_at_four_dpis_with_real_fonts() {
        let view: TaskbarView =
            serde_json::from_str(include_str!("../../../../fixtures/taskbar-display.json"))
                .unwrap();
        for dpi in [96, 120, 144, 192] {
            let height = (40 * dpi / 96) as i32;
            let width = (600 * dpi / 96) as i32;
            for layout in [DisplayLayout::TwoRows, DisplayLayout::SingleRow] {
                let prefs = DisplayPreferences {
                    layout,
                    ..Default::default()
                };
                let font = NativeFont::for_taskbar(dpi, prefs, height).unwrap();
                let plan = font.plan(&view, prefs, 0, width, height).unwrap().unwrap();
                assert_eq!(plan.density, crate::display::Density::Full);
                let mut positions: Vec<_> = plan.spans.iter().map(|span| span.y).collect();
                positions.sort_unstable();
                positions.dedup();
                assert_eq!(
                    positions.len(),
                    if layout == DisplayLayout::TwoRows {
                        2
                    } else {
                        1
                    }
                );
                assert!(
                    plan.spans
                        .iter()
                        .all(|span| span.y >= 0 && span.y + font.height() <= height)
                );
                let bitmap = font
                    .bitmap(Some(&plan), plan.width, height, Palette::for_background(0))
                    .unwrap();
                for y in positions {
                    assert!(
                        bitmap[54..]
                            .chunks_exact(4)
                            .enumerate()
                            .any(|(index, pixel)| {
                                // BMP scanlines run bottom to top. Check actual non-background
                                // glyph pixels within each separately measured row.
                                let row = height - 1 - index as i32 / plan.width;
                                row >= y && row < y + font.height() && pixel[..3] != [0, 0, 0]
                            })
                    );
                }
            }
        }
    }
    #[test]
    fn alpha_pixels_preserve_input_spacing_and_exact_premultiplied_glyph_colors() {
        assert_eq!(premultiplied_pixel(rgb(241, 244, 248), 0), [0, 0, 0, 1]);
        assert_eq!(
            premultiplied_pixel(rgb(85, 220, 157), 255),
            [157, 220, 85, 255]
        );
        assert_eq!(
            premultiplied_pixel(rgb(85, 220, 157), 128),
            [78, 110, 42, 128]
        );
        assert_eq!(premultiplied_pixel(rgb(0, 0, 0), 255), [0, 0, 0, 255]);
    }

    #[test]
    fn empty_clip_preserves_pixels_and_dc_state_then_visible_repaint_succeeds() {
        let font = NativeFont::new(96).unwrap();
        let original = Palette::for_background(rgb(13, 29, 47));
        let next = Palette::for_background(rgb(71, 89, 107));
        let no_op = font
            .bitmap_with(32, 16, |dc| {
                unsafe { font.paint(dc, None, 32, 16, original) }?;
                let outer = unsafe { SaveDC(dc) };
                assert!(outer > 0);
                let empty = Object(unsafe { CreateRectRgn(0, 0, 0, 0) });
                assert!(!empty.0.is_null());
                assert_eq!(unsafe { SelectClipRgn(dc, empty.0) }, NULLREGION);
                assert_eq!(unsafe { has_visible_clip(dc) }, Ok(false));
                unsafe { font.paint(dc, None, 32, 16, next) }?;
                // paint restores the caller's empty region rather than erasing its clip.
                assert_eq!(unsafe { has_visible_clip(dc) }, Ok(false));
                assert_ne!(unsafe { RestoreDC(dc, outer) }, 0);
                assert_eq!(unsafe { has_visible_clip(dc) }, Ok(true));
                Ok(())
            })
            .unwrap();
        assert!(
            no_op[54..]
                .chunks_exact(4)
                .all(|pixel| pixel[..3] == [47, 29, 13])
        );
        let visible = font.bitmap(None, 32, 16, next).unwrap();
        assert!(
            visible[54..]
                .chunks_exact(4)
                .all(|pixel| pixel[..3] == [107, 89, 71])
        );
        // A null/invalid DC must still fail; the no-op rule only accepts valid empty clips.
        assert_eq!(
            unsafe { has_visible_clip(ptr::null_mut()) },
            Err(WireError::InvalidState)
        );
        assert_eq!(
            unsafe { font.paint(ptr::null_mut(), None, 32, 16, next) },
            Err(WireError::InvalidState)
        );
    }
}

#[cfg(test)]
#[test]
fn detail_metrics_use_data_glyphs_and_switch_back_to_body_without_changing_the_dc() {
    use crate::display::{Density, PlacedSpan, Span};
    use windows_sys::Win32::Graphics::Gdi::{GetCurrentObject, OBJ_FONT};
    for dpi in [96, 120, 144, 192] {
        let body = NativeFont::for_details(dpi).unwrap();
        let data = NativeFont::for_data(dpi).unwrap();
        let gap = body.height().max(data.height()) + 12;
        let width = 900;
        let height = gap * 2;
        let palette = Palette::for_background(rgb(250, 252, 255));
        let plan = MeasuredPlan {
            width,
            height,
            density: Density::Full,
            spans: vec![
                PlacedSpan {
                    span: Span {
                        text: "251.9M Token $53.08 90%".into(),
                        tone: Tone::Normal,
                    },
                    x: 8,
                    y: 4,
                    width: 880,
                },
                PlacedSpan {
                    span: Span {
                        text: "TokenPulse · 用量详情".into(),
                        tone: Tone::Normal,
                    },
                    x: 8,
                    y: gap,
                    width: 880,
                },
            ],
        };
        let previous = unsafe { GetCurrentObject(body.dc.0, OBJ_FONT as u32) };
        let actual = body
            .bitmap_with(width, height, |dc| unsafe {
                body.paint_mode(
                    dc,
                    Some(&plan),
                    width,
                    height,
                    palette,
                    false,
                    Some((&data, &[0])),
                )
            })
            .unwrap();
        assert_eq!(
            unsafe { GetCurrentObject(body.dc.0, OBJ_FONT as u32) },
            previous
        );
        // Independently draw the intended fonts through TextOutW, rather than
        // accepting a logical face name as proof that the right glyphs were painted.
        let expected = body
            .bitmap_with(width, height, |dc| unsafe {
                let brush = Object(CreateSolidBrush(palette.background));
                assert!(!brush.0.is_null());
                assert_ne!(
                    FillRect(
                        dc,
                        &RECT {
                            left: 0,
                            top: 0,
                            right: width,
                            bottom: height
                        },
                        brush.0
                    ),
                    0
                );
                data.text(dc, 8, 4, &plan.spans[0].span.text, palette.foreground)?;
                body.text(dc, 8, gap, &plan.spans[1].span.text, palette.foreground)
            })
            .unwrap();
        assert_eq!(actual, expected, "wrong glyphs at {dpi} DPI");
    }
}

#[cfg(test)]
#[test]
fn buffered_popup_publishes_only_a_complete_frame_and_retains_pixels_on_render_failure() {
    use windows_sys::Win32::Graphics::Gdi::GetPixel;
    let font = NativeFont::for_details(96).unwrap();
    let old = rgb(200, 30, 20);
    let background = rgb(20, 40, 180);
    let content = rgb(20, 180, 40);
    font.bitmap_with(64, 32, |dc| unsafe {
        let fill = |target, area: RECT, color| {
            let brush = Object(CreateSolidBrush(color));
            assert!(!brush.0.is_null());
            assert_ne!(FillRect(target, &area, brush.0), 0);
        };
        let bounds = RECT {
            left: 0,
            top: 0,
            right: 64,
            bottom: 32,
        };
        fill(dc, bounds, old);
        paint_buffered(dc, 64, 32, |buffer| {
            fill(buffer, bounds, background);
            assert_eq!(GetPixel(dc, 0, 0), old, "background must remain offscreen");
            fill(
                buffer,
                RECT {
                    left: 8,
                    top: 8,
                    right: 32,
                    bottom: 24,
                },
                content,
            );
            assert_eq!(
                GetPixel(dc, 16, 16),
                old,
                "partial contents must remain offscreen"
            );
            Ok(())
        })?;
        assert_eq!(GetPixel(dc, 0, 0), background);
        assert_eq!(GetPixel(dc, 16, 16), content);
        assert_eq!(
            paint_buffered(dc, 64, 32, |buffer| {
                fill(buffer, bounds, 0);
                Err(WireError::InvalidState)
            }),
            Err(WireError::InvalidState)
        );
        assert_eq!(GetPixel(dc, 0, 0), background);
        assert_eq!(GetPixel(dc, 16, 16), content);
        Ok(())
    })
    .unwrap();
}
