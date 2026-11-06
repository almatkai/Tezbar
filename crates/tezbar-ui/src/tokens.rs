use gpui::{Hsla, Window, WindowAppearance, rgb, rgba};
use std::sync::atomic::{AtomicU8, Ordering};
static CHOICE: AtomicU8 = AtomicU8::new(0);
pub fn set(choice: &str) -> Result<(), String> {
    CHOICE.store(
        match choice {
            "system" => 0,
            "light" | "white" => 1,
            "dark" => 2,
            _ => return Err("theme must be system, light or dark".into()),
        },
        Ordering::Relaxed,
    );
    Ok(())
}
#[derive(Clone, Copy)]
pub struct Tokens {
    pub zoom: f32,
    pub light: bool,
    pub settings_panel: Hsla,
    pub settings_accent_strong: Hsla,
    pub bar_float: Hsla,
    pub panel: Hsla,
    pub field: Hsla,
    pub popover: Hsla,
    pub popover_solid: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
    pub accent: Hsla,
    pub accent_strong: Hsla,
    pub accent_hover: Hsla,
    pub link: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    pub shell: Hsla,
    pub sheet: Hsla,
    pub rim: Hsla,
    pub hairline: Hsla,
    pub pill: Hsla,
    pub pill_strong: Hsla,
    pub bar: Hsla,
    pub text1: Hsla,
    pub text2: Hsla,
    pub text3: Hsla,
    pub text4: Hsla,
    pub danger: Hsla,
}
pub fn tokens(w: &Window) -> Tokens {
    let light = match CHOICE.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => matches!(
            w.appearance(),
            WindowAppearance::Light | WindowAppearance::VibrantLight
        ),
    };
    if light {
        Tokens {
            zoom: f32::from(w.rem_size()) / 16.,
            light: true,
            settings_panel: rgb(0xffffff).into(),
            settings_accent_strong: rgb(0x3e35a5).into(),
            bar_float: rgba(0xf7f7f9d1).into(),
            panel: rgba(0xffffffb3).into(),
            field: rgba(0xffffffe6).into(),
            popover: rgba(0xfffffffa).into(),
            popover_solid: rgb(0xffffff).into(),
            hover: rgba(0x0f172a0b).into(),
            selected: rgba(0x0f172a0f).into(),
            accent: rgb(0x4f52d9).into(),
            accent_strong: rgb(0x5b5ef0).into(),
            accent_hover: rgb(0x6a6df4).into(),
            link: rgb(0x4c44c1).into(),
            success: rgb(0x059669).into(),
            warning: rgb(0xd97706).into(),
            shell: rgb(0xf1f4f9).into(),
            sheet: if cfg!(windows) {
                rgb(0xfafafb).into()
            } else {
                rgba(0xfafafbf5).into()
            },
            rim: rgba(0x0f172a33).into(),
            hairline: rgba(0x0f172a1a).into(),
            pill: rgba(0x0f172a0d).into(),
            pill_strong: rgba(0x0f172a12).into(),
            bar: rgba(0x0f172a06).into(),
            text1: rgb(0x1b2539).into(),
            text2: rgb(0x3e4c65).into(),
            text3: rgb(0x5b6a81).into(),
            text4: rgb(0x637187).into(),
            danger: rgb(0xdc2626).into(),
        }
    } else {
        Tokens {
            zoom: f32::from(w.rem_size()) / 16.,
            light: false,
            settings_panel: rgb(0x12141a).into(),
            settings_accent_strong: rgb(0xa8aaff).into(),
            bar_float: rgba(0x16171bc7).into(),
            panel: rgba(0xffffff06).into(),
            field: rgba(0x00000038).into(),
            popover: rgba(0x202126fa).into(),
            popover_solid: rgb(0x202126).into(),
            hover: rgba(0xffffff0d).into(),
            selected: rgba(0xffffff12).into(),
            accent: rgb(0x4f52d9).into(),
            accent_strong: rgb(0x5b5ef0).into(),
            accent_hover: rgb(0x6a6df4).into(),
            link: rgb(0x8b8df7).into(),
            success: rgb(0x34d399).into(),
            warning: rgb(0xfbbf24).into(),
            shell: rgb(0x0a0b0e).into(),
            sheet: if cfg!(windows) {
                rgb(0x1c1d21).into()
            } else {
                rgba(0x1c1d21f2).into()
            },
            rim: rgba(0xffffff14).into(),
            hairline: rgba(0xffffff11).into(),
            pill: rgba(0xffffff0f).into(),
            pill_strong: rgba(0xffffff17).into(),
            bar: rgba(0x00000029).into(),
            text1: rgb(0xf5f6fa).into(),
            text2: rgb(0xbcbec8).into(),
            text3: rgb(0x868a96).into(),
            text4: rgb(0x5e6270).into(),
            danger: rgb(0xf87171).into(),
        }
    }
}

pub const HEADER: f32 = 44.;
pub const ROW: f32 = 36.;
pub const FOOTER: f32 = 32.;
pub const SHEET_RADIUS: f32 = 18.;
pub const PANEL_RADIUS: f32 = 10.;
pub const ROW_RADIUS: f32 = 8.;
pub const FIELD_RADIUS: f32 = 7.;
pub const CHIP_RADIUS: f32 = 5.;
pub const SPACING: [f32; 6] = [4., 8., 12., 16., 20., 24.];
pub const TYPE_SCALE: [f32; 8] = [15., 13.5, 13., 12.5, 12., 11., 10.5, 10.];
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub zoom: f32,
    pub reduced_motion: bool,
}
impl Default for Metrics {
    fn default() -> Self {
        Self {
            zoom: 1.,
            reduced_motion: false,
        }
    }
}
impl Metrics {
    pub fn for_window(w: &Window, cx: &gpui::App) -> Self {
        Self {
            zoom: f32::from(w.rem_size()) / 16.,
            reduced_motion: cx.reduce_motion(),
        }
    }
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = if zoom.is_finite() {
            zoom.clamp(0.75, 2.)
        } else {
            1.
        };
    }
    pub fn px(self, value: f32) -> gpui::Pixels {
        gpui::px(value * self.zoom)
    }
    pub fn surface_duration(self) -> std::time::Duration {
        std::time::Duration::from_millis(if self.reduced_motion { 0 } else { 90 })
    }
}
pub fn font_family(cx: &gpui::App, mono: bool) -> gpui::SharedString {
    static FONTS: std::sync::OnceLock<(gpui::SharedString, gpui::SharedString)> =
        std::sync::OnceLock::new();
    let fonts = FONTS.get_or_init(|| (choose_font(cx, false), choose_font(cx, true)));
    if mono {
        fonts.1.clone()
    } else {
        fonts.0.clone()
    }
}
fn choose_font(cx: &gpui::App, mono: bool) -> gpui::SharedString {
    let available = cx.text_system().all_font_names();
    let choices: &[&str] = if mono {
        &["Geist Mono", "SF Mono", "Menlo", "Consolas"]
    } else {
        &["Geist", ".SystemUIFont", "Segoe UI"]
    };
    choices
        .iter()
        .find(|name| available.iter().any(|font| font.as_str() == **name))
        .copied()
        .unwrap_or(if mono { "monospace" } else { ".SystemUIFont" })
        .into()
}

#[cfg(test)]
mod repair_tests {
    use super::*;
    fn contrast(foreground: u32, background: u32) -> f64 {
        let luminance = |color: u32| {
            [16, 8, 0]
                .into_iter()
                .zip([0.2126, 0.7152, 0.0722])
                .map(|(shift, weight)| {
                    let value = ((color >> shift) & 255) as f64 / 255.;
                    weight
                        * if value <= 0.04045 {
                            value / 12.92
                        } else {
                            ((value + 0.055) / 1.055).powf(2.4)
                        }
                })
                .sum::<f64>()
        };
        let a = luminance(foreground);
        let b = luminance(background);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
    #[test]
    fn tokens_contrast_test() {
        // Main readable text, secondary text, Settings link and primary button in both themes.
        for (text, sheet) in [
            (0x1b2539, 0xfafafb),
            (0x3e4c65, 0xfafafb),
            (0x4c44c1, 0xffffff),
            (0xf5f6fa, 0x1c1d21),
            (0xbcbec8, 0x1c1d21),
            (0x8b8df7, 0x12141a),
            (0xffffff, 0x5b5ef0),
        ] {
            assert!(contrast(text, sheet) >= 4.5, "{text:x} on {sheet:x}");
        }
    }
    #[gpui::test]
    fn launcher_bar_and_float_are_distinct_and_os_motion_is_consumed(
        cx: &mut gpui::TestAppContext,
    ) {
        struct View;
        impl gpui::Render for View {
            fn render(
                &mut self,
                _: &mut Window,
                _: &mut gpui::Context<Self>,
            ) -> impl gpui::IntoElement {
                gpui::div()
            }
        }
        let (_, cx) = cx.add_window_view(|_, _| View);
        cx.update(|w, cx| {
            let t = tokens(w);
            assert_ne!(t.bar, t.bar_float);
            assert_ne!(t.panel, t.settings_panel);
            cx.set_reduce_motion(true);
            assert!(Metrics::for_window(w, cx).surface_duration().is_zero());
        });
    }
}
