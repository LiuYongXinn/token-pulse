//! Explicit development-only visual fixtures. Never linked into the production host.
fn main() {
    #[cfg(windows)]
    {
        use std::{fs, path::PathBuf};
        use token_pulse_taskbar::{
            TaskbarView,
            display::{DisplayLayout, DisplayPreferences},
            windows::render::{NativeFont, Palette, rgb},
        };
        if std::env::args().skip(1).collect::<Vec<_>>() != ["--render-development-fixtures"] {
            std::process::exit(2);
        }
        let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-results/taskbar-native-visual");
        fs::create_dir_all(&output).unwrap();
        let fixture: TaskbarView =
            serde_json::from_str(include_str!("../../../fixtures/taskbar-display.json")).unwrap();
        for (name, dpi, width, height, layout, light, privacy, low) in [
            (
                "full-dark-100",
                96,
                600,
                48,
                DisplayLayout::TwoRows,
                false,
                false,
                false,
            ),
            (
                "compact-dark-100",
                96,
                198,
                48,
                DisplayLayout::TwoRows,
                false,
                false,
                false,
            ),
            (
                "single-light-125",
                120,
                600,
                40,
                DisplayLayout::SingleRow,
                true,
                false,
                false,
            ),
            (
                "full-dark-150-low",
                144,
                600,
                48,
                DisplayLayout::TwoRows,
                false,
                false,
                true,
            ),
            (
                "privacy-dark-200",
                192,
                600,
                48,
                DisplayLayout::TwoRows,
                false,
                true,
                false,
            ),
        ] {
            let mut view = fixture.clone();
            if low {
                view.quota.as_mut().unwrap().windows[1].remaining_percent = Some(0.0);
            }
            if privacy {
                view.privacy = true;
                view.scope_label = None;
                view.costs.clear();
                view.quota = None;
            }
            let font = NativeFont::new(dpi).unwrap();
            let width = (width * dpi / 96) as i32;
            let height = (height * dpi / 96) as i32;
            let prefs = DisplayPreferences {
                layout,
                ..Default::default()
            };
            let plan = font
                .plan(&view, prefs, 1790899200000, width, height)
                .unwrap()
                .expect("readable fixture");
            let palette = Palette::for_background(if light {
                rgb(241, 241, 241)
            } else {
                rgb(18, 18, 18)
            });
            fs::write(
                output.join(format!("DEVELOPMENT-FIXTURE-{name}.bmp")),
                font.bitmap(Some(&plan), plan.width, height, palette)
                    .unwrap(),
            )
            .unwrap();
            println!(
                "DEVELOPMENT_FIXTURE {name}: {:?}, {}x{} px",
                plan.density, plan.width, plan.height
            );
        }
        fs::write(output.join("README.txt"),"DEVELOPMENT VISUAL FIXTURES ONLY\nSynthetic consumption, hypothetical price and quota; not real account evidence.\nRendered by the same Windows system font, GDI measurement and painting as the native host.\nRequested font DPI is tested; this does not switch physical monitor DPI or embed in Explorer.\n").unwrap();
    }
    #[cfg(not(windows))]
    std::process::exit(2);
}
