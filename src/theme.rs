use gpui::{Hsla, WindowAppearance, rgb, rgba};

pub const UI_FONT: &str = ".SystemUIFont";
pub const MONO_FONT: &str = "Menlo";

pub struct Theme {
    pub background: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub faint: Hsla,
    pub accent: Hsla,
    pub link: Hsla,
    pub caret: Hsla,
    pub text_selection: Hsla,
    pub block_selection: Hsla,
    pub code_background: Hsla,
    pub scrollbar_thumb: Hsla,
    /// Behind the thumb, while the pointer is on the scrollbar.
    pub scrollbar_track: Hsla,
}

impl Theme {
    pub fn for_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self {
                background: rgb(0x1b1c1f).into(),
                text: rgb(0xe4e5e7).into(),
                muted: rgb(0x8d939c).into(),
                faint: rgb(0x3a3d43).into(),
                accent: rgb(0x5b9dff).into(),
                link: rgb(0x6cb1ff).into(),
                caret: rgb(0x5b9dff).into(),
                text_selection: rgba(0x5b9dff59).into(),
                block_selection: rgba(0x5b9dff2e).into(),
                code_background: rgb(0x26282c).into(),
                scrollbar_thumb: rgba(0xffffff66).into(),
                scrollbar_track: rgba(0xffffff12).into(),
            },
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self {
                background: rgb(0xffffff).into(),
                text: rgb(0x1f2328).into(),
                muted: rgb(0x6e7781).into(),
                faint: rgb(0xd0d7de).into(),
                accent: rgb(0x2f6feb).into(),
                link: rgb(0x0969da).into(),
                caret: rgb(0x2f6feb).into(),
                text_selection: rgba(0x2f6feb40).into(),
                block_selection: rgba(0x2f6feb1f).into(),
                code_background: rgb(0xf3f4f6).into(),
                scrollbar_thumb: rgba(0x00000066).into(),
                scrollbar_track: rgba(0x0000000f).into(),
            },
        }
    }
}
