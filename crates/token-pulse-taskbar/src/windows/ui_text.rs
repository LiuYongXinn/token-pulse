//! Native opaque UI text uses DirectWrite's natural metrics, like the WebView.
use crate::WireError;
use windows::{
    Win32::{
        Foundation::RECT,
        Graphics::{
            Direct2D::{
                Common::{D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_PIXEL_FORMAT},
                D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
                D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE, D2D1CreateFactory, ID2D1DCRenderTarget,
                ID2D1Factory,
            },
            DirectWrite::{
                DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_FEATURE,
                DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES, DWRITE_FONT_METRICS,
                DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_LINE_SPACING_METHOD_UNIFORM, DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE,
                DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory, IDWriteFactory, IDWriteFont,
                IDWriteTextFormat, IDWriteTextLayout, IDWriteTextLayout1,
            },
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Gdi::HDC,
        },
    },
    core::{BOOL, Interface, PCWSTR},
};
use windows_numerics::Vector2;

fn checked<T>(result: windows::core::Result<T>) -> Result<T, WireError> {
    result.map_err(|_| WireError::InvalidState)
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
pub(super) struct UiText {
    faces: Vec<(Vec<u16>, IDWriteFont)>,
    factory: IDWriteFactory,
    format: IDWriteTextFormat,
    target: ID2D1DCRenderTarget,
    scale: f32,
    height: i32,
    data: bool,
}
impl UiText {
    pub(super) fn new(dpi: u32, faces: &[&str], data: bool) -> Result<Self, WireError> {
        let factory: IDWriteFactory =
            checked(unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) })?;
        let mut collection = None;
        checked(unsafe { factory.GetSystemFontCollection(&mut collection, false) })?;
        let collection = collection.ok_or(WireError::InvalidState)?;
        let mut available = Vec::new();
        for face in faces {
            let name = wide(face);
            let mut index = 0;
            let mut exists = BOOL(0);
            checked(unsafe {
                collection.FindFamilyName(PCWSTR(name.as_ptr()), &mut index, &mut exists)
            })?;
            if exists.as_bool() {
                let family = checked(unsafe { collection.GetFontFamily(index) })?;
                let font = checked(unsafe {
                    family.GetFirstMatchingFont(
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_FONT_STRETCH_NORMAL,
                        DWRITE_FONT_STYLE_NORMAL,
                    )
                })?;
                available.push((name, font));
            }
        }
        let (name, _) = available.first().ok_or(WireError::InvalidState)?;
        let locale = wide("zh-CN");
        // Match the project's 12 CSS px detail/control text; system message-font
        // settings must not redefine the application's type size or weight.
        let format = checked(unsafe {
            factory.CreateTextFormat(
                PCWSTR(name.as_ptr()),
                &collection,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                12.0,
                PCWSTR(locale.as_ptr()),
            )
        })?;
        checked(unsafe { format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP) })?;
        // Match the page's primary-font line box in CSS pixels, distributing
        // line gap around the baseline rather than putting it entirely above.
        let mut metrics = DWRITE_FONT_METRICS::default();
        unsafe {
            available[0].1.GetMetrics(&mut metrics);
        }
        let units = 12.0 / f32::from(metrics.designUnitsPerEm);
        let ascent = (f32::from(metrics.ascent) * units).round();
        let descent = (f32::from(metrics.descent) * units).round();
        let gap = (f32::from(metrics.lineGap) * units).round();
        checked(unsafe {
            format.SetLineSpacing(
                DWRITE_LINE_SPACING_METHOD_UNIFORM,
                ascent + descent + gap,
                ascent + gap / 2.0,
            )
        })?;
        let drawing: ID2D1Factory =
            checked(unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) })?;
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_IGNORE,
            },
            dpiX: dpi as f32,
            dpiY: dpi as f32,
            ..Default::default()
        };
        let target = checked(unsafe { drawing.CreateDCRenderTarget(&properties) })?;
        unsafe {
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE);
        }
        let mut font = Self {
            faces: available,
            factory,
            format,
            target,
            scale: dpi as f32 / 96.0,
            height: 0,
            data,
        };
        font.height = (font.metrics("Ag012")?.height * font.scale).ceil() as i32;
        Ok(font)
    }
    fn layout(&self, text: &str) -> Result<IDWriteTextLayout, WireError> {
        let units: Vec<_> = text.encode_utf16().collect();
        let layout = checked(unsafe {
            self.factory
                .CreateTextLayout(&units, &self.format, 100_000.0, 100_000.0)
        })?;
        if self.data {
            // --dataFont selectors in silver-mist.css use tnum and -0.5 CSS px.
            let range = DWRITE_TEXT_RANGE {
                startPosition: 0,
                length: units.len() as u32,
            };
            let natural: IDWriteTextLayout1 = checked(layout.cast())?;
            checked(unsafe { natural.SetCharacterSpacing(0.0, -0.5, 0.0, range) })?;
            let typography = checked(unsafe { self.factory.CreateTypography() })?;
            checked(unsafe {
                typography.AddFontFeature(DWRITE_FONT_FEATURE {
                    nameTag: DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES,
                    parameter: 1,
                })
            })?;
            checked(unsafe { layout.SetTypography(&typography, range) })?;
        }
        // CSS tries the declared families per Unicode scalar before system fallback.
        // DirectWrite's automatic font links alone need not honor that same order.
        let mut offset = 0;
        for ch in text.chars() {
            for (name, font) in &self.faces {
                if checked(unsafe { font.HasCharacter(ch as u32) })?.as_bool() {
                    checked(unsafe {
                        layout.SetFontFamilyName(
                            PCWSTR(name.as_ptr()),
                            DWRITE_TEXT_RANGE {
                                startPosition: offset,
                                length: ch.len_utf16() as u32,
                            },
                        )
                    })?;
                    break;
                }
            }
            offset += ch.len_utf16() as u32;
        }
        Ok(layout)
    }
    fn metrics(&self, text: &str) -> Result<DWRITE_TEXT_METRICS, WireError> {
        let layout = self.layout(text)?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        checked(unsafe { layout.GetMetrics(&mut metrics) })?;
        Ok(metrics)
    }
    pub(super) fn width(&self, text: &str) -> Result<i32, WireError> {
        // DirectWrite excludes a negative trailing character spacing from its
        // layout box; CSS includes it, including after the last character.
        let trailing = if self.data && !text.is_empty() {
            -0.5
        } else {
            0.0
        };
        Ok(
            ((self.metrics(text)?.widthIncludingTrailingWhitespace + trailing) * self.scale).ceil()
                as i32,
        )
    }
    pub(super) fn height(&self) -> i32 {
        self.height
    }
    pub(super) unsafe fn paint(
        &self,
        dc: windows_sys::Win32::Graphics::Gdi::HDC,
        x: i32,
        y: i32,
        text: &str,
        color: u32,
    ) -> Result<(), WireError> {
        let mut bounds: windows_sys::Win32::Foundation::RECT = unsafe { std::mem::zeroed() };
        if unsafe { windows_sys::Win32::Graphics::Gdi::GetClipBox(dc, &mut bounds) } == 0 {
            return Err(WireError::InvalidState);
        }
        let area = RECT {
            left: 0,
            top: 0,
            right: bounds.right,
            bottom: bounds.bottom,
        };
        if area.right <= 0 || area.bottom <= 0 {
            return Ok(());
        }
        checked(unsafe { self.target.BindDC(HDC(dc), &area) })?;
        let brush = checked(unsafe {
            self.target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: (color & 255) as f32 / 255.0,
                    g: ((color >> 8) & 255) as f32 / 255.0,
                    b: ((color >> 16) & 255) as f32 / 255.0,
                    a: 1.0,
                },
                None,
            )
        })?;
        let layout = self.layout(text)?;
        unsafe {
            self.target.BeginDraw();
            self.target.DrawTextLayout(
                Vector2::new(x as f32 / self.scale, y as f32 / self.scale),
                &layout,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
        }
        checked(unsafe { self.target.EndDraw(None, None) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_stack_selects_chinese_face_and_keeps_surrogate_pairs_together() {
        let font = UiText::new(
            144,
            &[
                "Unavailable TokenPulse Font",
                "Segoe UI",
                "Microsoft YaHei UI",
            ],
            false,
        )
        .unwrap();
        let layout = font.layout("A中🙂B").unwrap();
        let family = |position| {
            let mut name = [0u16; 64];
            checked(unsafe { layout.GetFontFamilyName(position, &mut name, None) }).unwrap();
            String::from_utf16_lossy(&name[..name.iter().position(|unit| *unit == 0).unwrap()])
        };
        assert_eq!(family(0), "Segoe UI");
        assert_eq!(family(1), "Microsoft YaHei UI");
        assert_eq!(family(2), family(3));
        assert_eq!(family(4), "Segoe UI");
        assert!(font.width("A中🙂B").unwrap() > font.width("AB").unwrap());
    }

    #[test]
    fn numeric_metrics_follow_browser_reference_without_gdi_glyph_rounding() {
        // Chromium, project --dataFont, 12 CSS px, tnum, letter-spacing:-.5px:
        // this sample measures 123.15625 CSS px before device-pixel rounding.
        for dpi in [96, 120, 144, 192] {
            let font = UiText::new(dpi, &["Bahnschrift"], true).unwrap();
            let actual = font
                .metrics("263.6M Token $55.93 89%")
                .unwrap()
                .widthIncludingTrailingWhitespace;
            assert!(
                (actual - 0.5 - 123.15625).abs() < 0.2,
                "natural width={actual} at {dpi} DPI"
            );
            let rounded = font.width("263.6M Token $55.93 89%").unwrap();
            let browser = (123.15625 * dpi as f32 / 96.0).ceil() as i32;
            assert!((rounded - browser).abs() <= 1);
        }
    }
}
