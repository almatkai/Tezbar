use gpui::{Hsla, Window, WindowAppearance, rgb, rgba};
use std::sync::atomic::{AtomicU8, Ordering};
static CHOICE: AtomicU8 = AtomicU8::new(0);
pub fn set(choice: &str) -> Result<(), String> {
    CHOICE.store(
        match choice {
            "system" => 0,
            "light" => 1,
            "dark" => 2,
            _ => return Err("theme must be system, light or dark".into()),
        },
        Ordering::Relaxed,
    );
    Ok(())
}
#[derive(Clone, Copy)]
pub struct Tokens {
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
            sheet: rgba(0xfafafbf5).into(),
            rim: rgba(0x0f172a33).into(),
            hairline: rgba(0x0f172a1a).into(),
            pill: rgba(0x0f172a0d).into(),
            pill_strong: rgba(0x0f172a12).into(),
            bar: rgba(0xf7f7f9d1).into(),
            text1: rgb(0x1b2539).into(),
            text2: rgb(0x3e4c65).into(),
            text3: rgb(0x5b6a81).into(),
            text4: rgb(0x637187).into(),
            danger: rgb(0xdc2626).into(),
        }
    } else {
        Tokens {
            sheet: rgba(0x1c1d21f2).into(),
            rim: rgba(0xffffff14).into(),
            hairline: rgba(0xffffff11).into(),
            pill: rgba(0xffffff0f).into(),
            pill_strong: rgba(0xffffff17).into(),
            bar: rgba(0x16171bc7).into(),
            text1: rgb(0xf5f6fa).into(),
            text2: rgb(0xbcbec8).into(),
            text3: rgb(0x868a96).into(),
            text4: rgb(0x5e6270).into(),
            danger: rgb(0xf87171).into(),
        }
    }
}
