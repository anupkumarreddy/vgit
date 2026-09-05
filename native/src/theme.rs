use gpui::{Div, FontWeight, div, prelude::*, px, rgb};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            Self::Dark => Palette {
                app: 0x181818,
                titlebar: 0x181818,
                activity: 0x181818,
                sidebar: 0x181818,
                editor: 0x1f1f1f,
                editor_alt: 0x181818,
                panel: 0x202020,
                elevated: 0x252526,
                line: 0x2b2b2b,
                line_strong: 0x3b3b3b,
                text: 0xd4d4d4,
                text_bright: 0xf0f0f0,
                muted: 0x9d9d9d,
                dim: 0x727272,
                selection: 0x263b35,
                hover: 0x2a2d2e,
                code_gutter: 0x858585,
                local: 0x73c991,
                remote: 0x75beff,
                tag: 0xc586c0,
                merge: 0xd7ba7d,
                red: 0xf48771,
                added_bg: 0x1d3328,
                removed_bg: 0x3a2325,
            },
            Self::Light => Palette {
                app: 0xf3f3f3,
                titlebar: 0xdddddd,
                activity: 0x2c2c2c,
                sidebar: 0xf3f3f3,
                editor: 0xffffff,
                editor_alt: 0xf8f8f8,
                panel: 0xf3f3f3,
                elevated: 0xffffff,
                line: 0xd4d4d4,
                line_strong: 0xb8b8b8,
                text: 0x3b3b3b,
                text_bright: 0x1e1e1e,
                muted: 0x616161,
                dim: 0x8a8a8a,
                selection: 0xdcefe5,
                hover: 0xe8e8e8,
                code_gutter: 0x6e7681,
                local: 0x18864b,
                remote: 0x1673b1,
                tag: 0x8b4a92,
                merge: 0x9a6700,
                red: 0xb42318,
                added_bg: 0xe6f4ea,
                removed_bg: 0xfce8e6,
            },
        }
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub app: u32,
    pub titlebar: u32,
    pub activity: u32,
    pub sidebar: u32,
    pub editor: u32,
    pub editor_alt: u32,
    pub panel: u32,
    pub elevated: u32,
    pub line: u32,
    pub line_strong: u32,
    pub text: u32,
    pub text_bright: u32,
    pub muted: u32,
    pub dim: u32,
    pub selection: u32,
    pub hover: u32,
    pub code_gutter: u32,
    pub local: u32,
    pub remote: u32,
    pub tag: u32,
    pub merge: u32,
    pub red: u32,
    pub added_bg: u32,
    pub removed_bg: u32,
}

impl Palette {
    pub fn branch(self, lane: usize) -> u32 {
        match lane {
            0 => self.local,
            1 => self.remote,
            _ => self.tag,
        }
    }
}

pub fn row() -> Div {
    div().flex().items_center()
}

pub fn column() -> Div {
    div().flex().flex_col()
}

pub fn section_label(colors: Palette, text: impl Into<String>) -> Div {
    div()
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(colors.muted))
        .child(text.into())
}

pub fn badge(_colors: Palette, text: impl Into<String>, color: u32) -> Div {
    div()
        .flex_none()
        .px_2()
        .py(px(2.))
        .rounded(px(3.))
        .bg(gpui::rgba((color << 8) | 26))
        .text_color(rgb(color))
        .text_size(px(10.))
        .child(text.into())
}
