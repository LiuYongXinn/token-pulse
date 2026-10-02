#![cfg(windows)]
use token_pulse_taskbar::{
    TaskbarView,
    display::{Density, DisplayPreferences},
    windows::render::{NativeFont, Palette, rgb},
};
fn fixture() -> TaskbarView {
    serde_json::from_str(include_str!("../../../fixtures/taskbar-display.json")).unwrap()
}
#[test]
fn actual_system_font_measurement_fits_four_dpis_and_native_clear_overwrites_all_pixels() {
    let view = fixture();
    for dpi in [96, 120, 144, 192] {
        let font = NativeFont::new(dpi).unwrap();
        let width = (600 * dpi / 96) as i32;
        let height = (48 * dpi / 96) as i32;
        let plan = font
            .plan(
                &view,
                DisplayPreferences::default(),
                1790899200000,
                width,
                height,
            )
            .unwrap()
            .unwrap();
        assert_eq!(plan.density, Density::Full);
        for placed in &plan.spans {
            assert!(placed.x + placed.width <= plan.width);
            assert!(placed.y + font.height() <= plan.height);
        }
        let palette = Palette::for_background(rgb(18, 18, 18));
        let drawn = font
            .bitmap(Some(&plan), plan.width, height, palette)
            .unwrap();
        let cleared = font.bitmap(None, plan.width, height, palette).unwrap();
        assert_eq!(&drawn[..2], b"BM");
        assert_eq!(drawn.len(), 54 + plan.width as usize * height as usize * 4);
        assert_ne!(drawn, cleared);
        assert!(
            cleared[54..]
                .chunks_exact(4)
                .all(|p| p[..3] == [18, 18, 18])
        );
    }
}
#[test]
fn real_font_crowding_and_bounded_bitmap_reject_unreadable_or_huge_surfaces() {
    let font = NativeFont::new(96).unwrap();
    let view = fixture();
    assert!(
        font.plan(&view, DisplayPreferences::default(), 0, 8, 48)
            .unwrap()
            .is_none()
    );
    assert!(
        font.bitmap(None, 4096, 2048, Palette::for_background(0))
            .is_err()
    );
    assert!(NativeFont::new(0).is_err());
    let current = Palette::system(rgb(18, 18, 18)).unwrap();
    assert_ne!(current.foreground, current.background);
}
