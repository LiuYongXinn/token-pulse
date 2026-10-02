//! System-font measurement and GDI drawing. No screenshots of other applications or file I/O.
use crate::{
    TaskbarView, WireError,
    display::{DisplayPreferences, MeasureBounds, MeasuredPlan, Tone, measure},
};
use std::{mem, ptr};
use windows_sys::Win32::{
    Foundation::{COLORREF, RECT, SIZE},
    Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, COLOR_WINDOW, COLOR_WINDOWTEXT, CreateCompatibleDC,
        CreateDIBSection, CreateFontIndirectW, CreateSolidBrush, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, FillRect, GdiFlush, GetSysColor, GetTextExtentPoint32W, GetTextMetricsW, HDC,
        HFONT, HGDIOBJ, IntersectClipRect, RestoreDC, SaveDC, SelectObject, SetBkMode,
        SetTextColor, TEXTMETRICW, TRANSPARENT, TextOutW,
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
pub struct NativeFont {
    dc: Dc,
    font: Object,
    previous: HGDIOBJ,
    height: i32,
    dpi: u32,
}
impl NativeFont {
    pub fn new(dpi: u32) -> Result<Self, WireError> {
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
        let saved = unsafe { SaveDC(dc) };
        if saved == 0 {
            return Err(WireError::InvalidState);
        }
        let result = (|| {
            unsafe {
                IntersectClipRect(dc, 0, 0, width, height);
            }
            let brush = unsafe { CreateSolidBrush(palette.background) };
            if brush.is_null() {
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
                for placed in &plan.spans {
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
        let result = unsafe { self.paint(self.dc.0, plan, width, height, palette) };
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
impl Drop for NativeFont {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc.0, self.previous);
        }
    }
}
