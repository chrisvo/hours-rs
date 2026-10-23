use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Theme {
    Light,
    #[default]
    Parchment,
    Dark,
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub page: Color32,
    pub sidebar: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub line: Color32,
    pub accent: Color32,
    pub active: Color32,
    pub highlight: Color32,
}

impl Theme {
    pub const ALL: [Self; 3] = [Self::Light, Self::Parchment, Self::Dark];

    pub fn title(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Parchment => "Parchment",
            Self::Dark => "Dark",
        }
    }

    pub fn palette(self) -> Palette {
        let c = Color32::from_rgb;
        match self {
            Self::Light => Palette {
                page: c(253, 252, 249),
                sidebar: c(244, 242, 237),
                text: c(47, 45, 40),
                muted: c(110, 105, 95),
                faint: c(147, 141, 130),
                line: c(225, 221, 213),
                accent: c(138, 63, 49),
                active: c(233, 225, 214),
                highlight: c(244, 229, 181),
            },
            Self::Parchment => Palette {
                page: c(249, 245, 235),
                sidebar: c(239, 233, 221),
                text: c(63, 54, 43),
                muted: c(125, 110, 90),
                faint: c(157, 142, 121),
                line: c(221, 211, 193),
                accent: c(139, 67, 46),
                active: c(227, 216, 197),
                highlight: c(237, 218, 164),
            },
            Self::Dark => Palette {
                page: c(30, 31, 29),
                sidebar: c(25, 26, 24),
                text: c(225, 220, 208),
                muted: c(164, 158, 143),
                faint: c(125, 122, 112),
                line: c(57, 58, 52),
                accent: c(209, 160, 123),
                active: c(61, 57, 47),
                highlight: c(78, 69, 42),
            },
        }
    }

    pub fn apply(self, ctx: &egui::Context) {
        let p = self.palette();
        let mut style = (*ctx.style()).clone();
        style.visuals = if self == Self::Dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        style.visuals.panel_fill = p.page;
        style.visuals.window_fill = p.page;
        style.visuals.extreme_bg_color = p.sidebar;
        style.visuals.faint_bg_color = p.active;
        style.visuals.override_text_color = Some(p.text);
        style.visuals.window_stroke = Stroke::new(1.0_f32, p.line);
        style.visuals.hyperlink_color = p.accent;
        style.visuals.selection.bg_fill = p.active;
        style.visuals.selection.stroke = Stroke::new(1.0_f32, p.accent);
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.line);
        style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);
        style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        style.visuals.widgets.inactive.bg_fill = p.sidebar;
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, p.line);
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, p.text);
        style.visuals.widgets.hovered.weak_bg_fill = p.active;
        style.visuals.widgets.hovered.bg_fill = p.active;
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.faint);
        style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, p.text);
        style.visuals.widgets.active.bg_fill = p.active;
        style.visuals.widgets.active.weak_bg_fill = p.active;
        style.visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, p.accent);
        style.spacing.item_spacing = egui::vec2(9.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        style.spacing.interact_size = egui::vec2(32.0, 30.0);
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(13.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(11.0));
        style.text_styles.insert(TextStyle::Heading, heading(22.0));
        ctx.set_style(style);
    }
}

pub fn heading(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("Cinzel".into()))
}
pub fn reading(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("Literata".into()))
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        (
            "Cinzel",
            include_bytes!("../assets/fonts/Cinzel.ttf").as_slice(),
        ),
        (
            "Literata",
            include_bytes!("../assets/fonts/Literata.ttf").as_slice(),
        ),
        (
            "Inter",
            include_bytes!("../assets/fonts/Inter.ttf").as_slice(),
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
        let mut family = vec![name.to_string()];
        family.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
        fonts.families.insert(FontFamily::Name(name.into()), family);
    }
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, "Inter".into());
    ctx.set_fonts(fonts);
}
