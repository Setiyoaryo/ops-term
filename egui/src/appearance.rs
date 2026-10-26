//! Persisted terminal typography and colors, independent of the surrounding UI theme.
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};
use eframe::egui::{self, Color32, FontId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalFont {
    JetbrainsRegular,
    JetbrainsMedium,
    Ubuntu,
    Dejavu,
    Liberation,
    Builtin,
    #[default]
    #[serde(other)]
    System,
}

impl TerminalFont {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System default",
            Self::JetbrainsRegular => "JetBrains Mono · Regular",
            Self::JetbrainsMedium => "JetBrains Mono · Medium",
            Self::Ubuntu => "Ubuntu Mono",
            Self::Dejavu => "DejaVu Sans Mono",
            Self::Liberation => "Liberation Mono",
            Self::Builtin => "Built-in monospace",
        }
    }
}

pub struct FontOption {
    pub font: TerminalFont,
    pub path: Option<PathBuf>,
}

// Discover paths once; do not retain the bytes of unselected fonts.
pub fn font_options() -> Vec<FontOption> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".local/share")));
    let mut dirs = Vec::new();
    if let Some(data) = data {
        dirs.push(data.join("fonts"));
    }
    if let Some(home) = home {
        dirs.push(home.join(".fonts"));
    }
    let jetbrains = |weight: &str| {
        let mut paths: Vec<_> = dirs
            .iter()
            .map(|dir| dir.join(format!("JetBrainsMono-{weight}.ttf")))
            .collect();
        paths.push(PathBuf::from(format!(
            "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-{weight}.ttf"
        )));
        paths
    };
    let regular = jetbrains("Regular");
    let medium = jetbrains("Medium");
    // Preserve the previous System search order exactly.
    let mut system = Vec::new();
    for dir in &dirs {
        system.push(dir.join("JetBrainsMono-Medium.ttf"));
        system.push(dir.join("JetBrainsMono-Regular.ttf"));
    }
    system.extend(
        [
            "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
            "/usr/share/fonts/truetype/ubuntu/UbuntuMono-R.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        ]
        .map(PathBuf::from),
    );
    let candidates = [
        (TerminalFont::System, system),
        (TerminalFont::JetbrainsRegular, regular),
        (TerminalFont::JetbrainsMedium, medium),
        (
            TerminalFont::Ubuntu,
            vec![
                PathBuf::from("/usr/share/fonts/truetype/ubuntu/UbuntuMono-R.ttf"),
                PathBuf::from("/usr/share/fonts/truetype/ubuntu/UbuntuMono[wght].ttf"),
            ],
        ),
        (
            TerminalFont::Dejavu,
            vec![PathBuf::from(
                "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            )],
        ),
        (
            TerminalFont::Liberation,
            vec![
                PathBuf::from("/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf"),
                PathBuf::from("/usr/share/fonts/truetype/liberation2/LiberationMono-Regular.ttf"),
            ],
        ),
        (TerminalFont::Builtin, Vec::new()),
    ];
    candidates
        .into_iter()
        .filter_map(|(font, paths)| {
            let path = paths
                .into_iter()
                .find(|path| path.is_file() && std::fs::File::open(path).is_ok());
            (path.is_some() || matches!(font, TerminalFont::System | TerminalFont::Builtin))
                .then_some(FontOption { font, path })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Palette {
    Soft,
    Warm,
    Cool,
    #[default]
    #[serde(other)]
    Original,
}

impl Palette {
    fn label(self) -> &'static str {
        match self {
            Self::Original => "Original · vivid",
            Self::Soft => "Soft · muted",
            Self::Warm => "Warm · earthy",
            Self::Cool => "Cool · blue grey",
        }
    }

    fn ansi(self) -> &'static [[u8; 3]; 16] {
        match self {
            Self::Original => &ORIGINAL,
            Self::Soft => &[
                [18, 20, 23],
                [191, 111, 116],
                [139, 171, 133],
                [190, 171, 121],
                [113, 145, 180],
                [159, 139, 177],
                [126, 166, 171],
                [193, 197, 202],
                [106, 114, 124],
                [203, 135, 139],
                [158, 187, 152],
                [205, 189, 147],
                [142, 170, 199],
                [179, 162, 195],
                [151, 188, 191],
                [214, 217, 221],
            ],
            Self::Warm => &[
                [24, 22, 19],
                [191, 118, 104],
                [153, 166, 119],
                [195, 165, 105],
                [130, 150, 162],
                [169, 137, 154],
                [135, 167, 154],
                [203, 191, 166],
                [124, 115, 99],
                [207, 143, 127],
                [173, 185, 140],
                [211, 186, 135],
                [155, 172, 180],
                [189, 160, 174],
                [158, 188, 176],
                [221, 211, 190],
            ],
            Self::Cool => &[
                [18, 23, 29],
                [190, 123, 137],
                [134, 177, 155],
                [189, 180, 139],
                [116, 156, 195],
                [159, 147, 191],
                [129, 180, 191],
                [190, 204, 216],
                [104, 124, 144],
                [208, 148, 160],
                [159, 195, 177],
                [207, 198, 163],
                [145, 179, 212],
                [181, 170, 207],
                [157, 200, 208],
                [214, 225, 232],
            ],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub font: TerminalFont,
    pub palette: Palette,
    pub brightness: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            font: TerminalFont::System,
            palette: Palette::Original,
            brightness: 100.0,
        }
    }
}

impl Appearance {
    pub fn normalize(&mut self, fonts: &[FontOption]) {
        if !fonts.iter().any(|option| option.font == self.font) {
            self.font = TerminalFont::System;
        }
        self.brightness = if self.brightness.is_finite() {
            self.brightness.clamp(55.0, 100.0)
        } else {
            100.0
        };
    }

    pub fn colors(&self, background: Color32, ui_text: Color32) -> Colors {
        let foreground = match self.palette {
            Palette::Original => ui_text,
            Palette::Soft => Color32::from_rgb(193, 197, 202),
            Palette::Warm => Color32::from_rgb(203, 191, 166),
            Palette::Cool => Color32::from_rgb(190, 204, 216),
        };
        Colors {
            ansi: self.palette.ansi(),
            background,
            foreground,
            brightness: self.brightness / 100.0,
        }
    }

    // Returns (any setting changed, selected font changed).
    pub fn controls(
        &mut self,
        ui: &mut egui::Ui,
        fonts: &[FontOption],
        size: &mut f32,
        background: Color32,
        ui_text: Color32,
    ) -> (bool, bool) {
        let previous_font = self.font;
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Terminal font");
            egui::ComboBox::from_id_salt("terminal-font")
                .width(220.0)
                .selected_text(self.font.label())
                .show_ui(ui, |ui| {
                    for choice in fonts {
                        changed |= ui
                            .selectable_value(&mut self.font, choice.font, choice.font.label())
                            .changed();
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Terminal font size");
            changed |= ui.add(egui::Slider::new(size, 10.0..=30.0)).changed();
        });
        ui.horizontal(|ui| {
            ui.label("Text palette");
            egui::ComboBox::from_id_salt("terminal-palette")
                .width(220.0)
                .selected_text(self.palette.label())
                .show_ui(ui, |ui| {
                    for palette in [
                        Palette::Original,
                        Palette::Soft,
                        Palette::Warm,
                        Palette::Cool,
                    ] {
                        changed |= ui
                            .selectable_value(&mut self.palette, palette, palette.label())
                            .changed();
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Text brightness");
            changed |= ui
                .add(
                    egui::Slider::new(&mut self.brightness, 55.0..=100.0)
                        .suffix("%")
                        .fixed_decimals(0),
                )
                .changed();
        });
        let colors = self.colors(background, ui_text);
        let font = FontId::monospace(*size);
        egui::Frame::new()
            .fill(background)
            .inner_margin(8.0)
            .corner_radius(4.0)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Aa Bb  0O 1l  {} [] ->")
                        .font(font.clone())
                        .color(colors.dim(colors.foreground)),
                );
                ui.horizontal_wrapped(|ui| {
                    for (sample, color) in [
                        ("directory", NamedColor::Blue),
                        ("success", NamedColor::Green),
                        ("warning", NamedColor::Yellow),
                        ("error", NamedColor::Red),
                    ] {
                        ui.label(
                            egui::RichText::new(sample)
                                .font(font.clone())
                                .color(colors.dim(colors.resolve(AnsiColor::Named(color), false))),
                        );
                    }
                });
            });
        (changed, previous_font != self.font)
    }
}

#[derive(Clone, Copy)]
pub struct Colors {
    ansi: &'static [[u8; 3]; 16],
    pub background: Color32,
    pub foreground: Color32,
    brightness: f32,
}

impl Colors {
    pub fn resolve(self, color: AnsiColor, is_background: bool) -> Color32 {
        let default = if is_background {
            self.background
        } else {
            self.foreground
        };
        match color {
            AnsiColor::Named(n) => named_index(n).map_or(default, |i| {
                let [r, g, b] = self.ansi[i];
                Color32::from_rgb(r, g, b)
            }),
            AnsiColor::Indexed(i) => self.indexed(i),
            AnsiColor::Spec(rgb) => Color32::from_rgb(rgb.r, rgb.g, rgb.b),
        }
    }

    pub fn dim(self, color: Color32) -> Color32 {
        if self.brightness >= 1.0 {
            return color;
        }
        let channel = |value: u8| (value as f32 * self.brightness).round() as u8;
        Color32::from_rgba_premultiplied(
            channel(color.r()),
            channel(color.g()),
            channel(color.b()),
            color.a(),
        )
    }

    fn indexed(self, i: u8) -> Color32 {
        if i < 16 {
            let [r, g, b] = self.ansi[i as usize];
            return Color32::from_rgb(r, g, b);
        }
        if i < 232 {
            let i = i - 16;
            let conv = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            return Color32::from_rgb(conv((i / 36) % 6), conv((i / 6) % 6), conv(i % 6));
        }
        let value = 8 + (i - 232) * 10;
        Color32::from_rgb(value, value, value)
    }
}

const ORIGINAL: [[u8; 3]; 16] = [
    [10, 10, 10],
    [255, 107, 122],
    [92, 230, 138],
    [255, 200, 87],
    [77, 159, 255],
    [180, 142, 255],
    [77, 216, 230],
    [255, 255, 255],
    [138, 138, 138],
    [255, 138, 149],
    [127, 240, 168],
    [255, 219, 138],
    [143, 194, 255],
    [205, 176, 255],
    [140, 239, 247],
    [255, 255, 255],
];

fn named_index(n: NamedColor) -> Option<usize> {
    use NamedColor::*;
    Some(match n {
        Black => 0,
        Red => 1,
        Green => 2,
        Yellow => 3,
        Blue => 4,
        Magenta => 5,
        Cyan => 6,
        White => 7,
        BrightBlack => 8,
        BrightRed => 9,
        BrightGreen => 10,
        BrightYellow => 11,
        BrightBlue => 12,
        BrightMagenta => 13,
        BrightCyan => 14,
        BrightWhite => 15,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

    fn colors(appearance: Appearance) -> Colors {
        appearance.colors(
            Color32::from_rgb(10, 10, 10),
            Color32::from_rgb(250, 250, 250),
        )
    }

    #[test]
    fn unknown_choices_preserve_other_appearance_fields() {
        let mut appearance: Appearance = serde_json::from_value(serde_json::json!({
            "font": "future-font", "palette": "future-palette", "brightness": 80.0
        }))
        .unwrap();
        appearance.normalize(&font_options());
        assert_eq!(appearance.font, TerminalFont::System);
        assert_eq!(appearance.palette, Palette::Original);
        assert_eq!(appearance.brightness, 80.0);
    }

    #[test]
    fn partial_settings_get_defaults_and_brightness_is_bounded() {
        let mut appearance: Appearance =
            serde_json::from_value(serde_json::json!({"brightness": 200.0})).unwrap();
        appearance.normalize(&font_options());
        assert_eq!(appearance.palette, Palette::Original);
        assert_eq!(appearance.brightness, 100.0);
        appearance.brightness = -1.0;
        appearance.normalize(&font_options());
        assert_eq!(appearance.brightness, 55.0);
        appearance.brightness = f32::NAN;
        appearance.normalize(&font_options());
        assert_eq!(appearance.brightness, 100.0);
    }

    #[test]
    fn missing_installed_font_falls_back_without_losing_colors() {
        let mut appearance = Appearance {
            font: TerminalFont::JetbrainsRegular,
            palette: Palette::Soft,
            brightness: 85.0,
        };
        appearance.normalize(&[FontOption {
            font: TerminalFont::System,
            path: None,
        }]);
        assert_eq!(appearance.font, TerminalFont::System);
        assert_eq!(appearance.palette, Palette::Soft);
    }

    #[test]
    fn discovered_external_fonts_exist_and_builtin_is_always_available() {
        let choices = font_options();
        assert!(choices
            .iter()
            .any(|choice| choice.font == TerminalFont::Builtin && choice.path.is_none()));
        for choice in choices {
            if let Some(path) = choice.path {
                assert!(path.is_file());
                assert!(std::fs::File::open(path).is_ok());
            }
        }
    }

    #[test]
    fn original_preserves_ansi_and_theme_default_colors() {
        let original = colors(Appearance::default());
        assert_eq!(
            original.resolve(Color::Named(NamedColor::Red), false),
            Color32::from_rgb(255, 107, 122)
        );
        assert_eq!(
            original.resolve(Color::Named(NamedColor::Foreground), false),
            Color32::from_rgb(250, 250, 250)
        );
        assert_eq!(original.dim(Color32::WHITE), Color32::WHITE);
    }

    #[test]
    fn soft_palette_matches_named_and_indexed_ansi_for_backgrounds_too() {
        let soft = colors(Appearance {
            palette: Palette::Soft,
            ..Default::default()
        });
        let blue = soft.resolve(Color::Named(NamedColor::Blue), false);
        assert_eq!(blue, Color32::from_rgb(113, 145, 180));
        assert_eq!(soft.resolve(Color::Indexed(4), false), blue);
        assert_eq!(soft.resolve(Color::Indexed(4), true), blue);
    }

    #[test]
    fn truecolor_and_extended_indexes_are_not_replaced_by_ansi_palette() {
        let original = colors(Appearance::default());
        let soft = colors(Appearance {
            palette: Palette::Soft,
            brightness: 80.0,
            ..Default::default()
        });
        let rgb = Color::Spec(Rgb {
            r: 10,
            g: 100,
            b: 200,
        });
        assert_eq!(soft.resolve(rgb, false), Color32::from_rgb(10, 100, 200));
        assert_eq!(soft.resolve(rgb, true), Color32::from_rgb(10, 100, 200));
        for index in 16..=255 {
            assert_eq!(
                soft.resolve(Color::Indexed(index), false),
                original.resolve(Color::Indexed(index), false)
            );
        }
        // Brightness is applied once to final foregrounds, after highlights.
        assert_eq!(
            soft.dim(soft.resolve(rgb, false)),
            Color32::from_rgb(8, 80, 160)
        );
    }
}
