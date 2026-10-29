#![allow(non_snake_case)]
use branchroom::settings::Settings;
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};

// Shared scale: compact metadata, readable controls, and page headings.
pub(super) const TEXT_SMALL: f32 = 12.;
pub(super) const TEXT_BODY: f32 = 14.;
pub(super) const TEXT_TITLE: f32 = 24.;
pub(super) const SPACE: [f32; 6] = [4., 8., 12., 16., 24., 32.];

// Semantic colors are selected together so custom-painted views follow the theme.
thread_local! { static LIGHT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
fn color(dark: (u8, u8, u8), light: (u8, u8, u8)) -> Color32 {
    let (r, g, b) = LIGHT.with(|value| if value.get() { light } else { dark });
    Color32::from_rgb(r, g, b)
}
#[allow(non_snake_case)]
pub(super) fn PAPER() -> Color32 {
    color((20, 23, 29), (248, 250, 253))
}
pub(super) fn SURFACE() -> Color32 {
    color((27, 31, 39), (255, 255, 255))
}
pub(super) fn RAISED() -> Color32 {
    color((35, 40, 50), (237, 241, 247))
}
pub(super) fn SIDEBAR() -> Color32 {
    color((17, 20, 26), (237, 242, 249))
}
pub(super) fn INK() -> Color32 {
    color((229, 233, 240), (29, 38, 55))
}
pub(super) fn MUTED() -> Color32 {
    color((154, 165, 183), (74, 87, 108))
}
pub(super) fn ACCENT() -> Color32 {
    color((146, 177, 255), (47, 75, 152))
}
pub(super) fn PRIMARY() -> Color32 {
    color((64, 91, 174), (197, 214, 252))
}
pub(super) fn SELECTED() -> Color32 {
    color((41, 54, 82), (214, 225, 248))
}
pub(super) fn LINE() -> Color32 {
    color((49, 56, 69), (172, 183, 200))
}
pub(super) fn CONNECTOR() -> Color32 {
    color((112, 125, 145), (90, 104, 124))
}
pub(super) fn INPUT_BORDER() -> Color32 {
    color((112, 125, 145), (90, 104, 124))
}
pub(super) fn SUCCESS() -> Color32 {
    color((114, 211, 165), (24, 107, 67))
}
pub(super) fn WARNING() -> Color32 {
    color((239, 194, 112), (126, 74, 8))
}
pub(super) fn DANGER() -> Color32 {
    color((255, 147, 151), (166, 35, 48))
}
pub(super) fn SNAPSHOT() -> Color32 {
    color((196, 166, 250), (105, 62, 155))
}

fn bundled_fonts() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    // Hack ships with egui and covers symbols (including arrows) missing from Ubuntu Light.
    // Keep it as a fallback so text still uses the proportional face where available.
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .push("Hack".into());
    fonts
}

pub(super) fn install(ctx: &egui::Context, light: bool) {
    LIGHT.with(|value| value.set(light));
    let fonts = bundled_fonts();
    #[cfg(all(windows, not(test)))]
    let mut fonts = fonts;
    // Prefer the installed system face in production. Tests use bundled fonts on every OS.
    // Windows fonts are never redistributed with the app.
    #[cfg(all(windows, not(test)))]
    if let Ok(bytes) = std::fs::read("C:/Windows/Fonts/segoeui.ttf") {
        fonts
            .font_data
            .insert("system-ui".into(), egui::FontData::from_owned(bytes).into());
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "system-ui".into());
    }
    #[cfg(all(windows, not(test)))]
    for (name, path) in [
        ("system-symbols", "C:/Windows/Fonts/seguisym.ttf"),
        ("system-languages", "C:/Windows/Fonts/Nirmala.ttf"),
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert(name.into(), egui::FontData::from_owned(bytes).into());
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push(name.into());
            }
        }
    }
    ctx.set_fonts(fonts);
    ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    let theme = if light {
        egui::Theme::Light
    } else {
        egui::Theme::Dark
    };
    let mut style = (*ctx.style_of(theme)).clone();
    style.visuals = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    style.visuals.override_text_color = Some(INK());
    style.visuals.panel_fill = PAPER();
    style.visuals.window_fill = SURFACE();
    style.visuals.extreme_bg_color = SIDEBAR();
    style.visuals.faint_bg_color = SURFACE();
    style.visuals.code_bg_color = SIDEBAR();
    style.visuals.hyperlink_color = ACCENT();
    style.visuals.warn_fg_color = WARNING();
    style.visuals.error_fg_color = DANGER();
    style.visuals.selection.bg_fill = SELECTED();
    style.visuals.selection.stroke = Stroke::new(1.5, ACCENT());
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1., LINE());
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1., MUTED());
    for widgets in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widgets.corner_radius = 5.into();
        widgets.fg_stroke = Stroke::new(1., INK());
    }
    style.visuals.widgets.inactive.bg_fill = RAISED();
    style.visuals.widgets.inactive.weak_bg_fill = SURFACE();
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1., INPUT_BORDER());
    style.visuals.widgets.hovered.bg_fill = SELECTED();
    style.visuals.widgets.hovered.weak_bg_fill = RAISED();
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1., ACCENT());
    style.visuals.widgets.active.bg_fill = SELECTED();
    style.visuals.widgets.active.weak_bg_fill = SELECTED();
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT());
    style.spacing.item_spacing = egui::vec2(SPACE[2], SPACE[1]);
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.spacing.button_padding = egui::vec2(SPACE[2], SPACE[1]);
    style.spacing.interact_size = egui::vec2(32., 32.);
    style.spacing.window_margin = egui::Margin::same(20);
    style.animation_time = 0.;
    for (role, size) in [
        (egui::TextStyle::Heading, TEXT_TITLE),
        (egui::TextStyle::Body, TEXT_BODY),
        (egui::TextStyle::Button, TEXT_BODY),
        (egui::TextStyle::Small, TEXT_SMALL),
    ] {
        style
            .text_styles
            .insert(role, egui::FontId::proportional(size));
    }
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        egui::FontId::monospace(TEXT_BODY),
    );
    ctx.set_style_of(theme, style);
    ctx.set_theme(theme);
}

pub(super) fn surface() -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE())
        .stroke(Stroke::new(1., LINE()))
        .corner_radius(6)
        .inner_margin(16)
}
pub(super) fn primary(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(INK()).strong())
            .fill(PRIMARY())
            .stroke(Stroke::NONE)
            .min_size(egui::vec2(0., 34.)),
    )
}
pub(super) fn badge(ui: &mut egui::Ui, label: &str, color: Color32) {
    egui::Frame::new()
        .fill({
            let bg = SURFACE();
            let mix = |a: u8, b: u8| ((a as u16 * 7 + b as u16) / 8) as u8;
            Color32::from_rgb(
                mix(bg.r(), color.r()),
                mix(bg.g(), color.g()),
                mix(bg.b(), color.b()),
            )
        })
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(220.));
            ui.add(egui::Label::new(RichText::new(label).color(color).size(12.)).truncate())
                .on_hover_text(label);
        });
}
pub(super) fn section(ui: &mut egui::Ui, label: &str) {
    ui.label(RichText::new(label).size(12.).strong().color(MUTED()));
}
pub(super) fn lane_color(index: usize) -> Color32 {
    [
        ACCENT(),
        SUCCESS(),
        SNAPSHOT(),
        WARNING(),
        Color32::from_rgb(114, 211, 226),
    ][index % 5]
}
pub(super) fn short(id: &str) -> &str {
    &id[..id.len().min(8)]
}
pub(super) fn ide_combo(ui: &mut egui::Ui, id: &str, settings: &Settings, index: &mut usize) {
    egui::ComboBox::from_id_salt(id)
        .width(ui.available_width().min(240.))
        .selected_text(
            settings
                .ides
                .get(*index)
                .map(|i| i.name.as_str())
                .unwrap_or("Select IDE"),
        )
        .show_ui(ui, |ui| {
            for (i, ide) in settings.ides.iter().enumerate() {
                ui.selectable_value(index, i, &ide.name);
            }
        })
        .response
        .on_hover_text("IDE");
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub(super) enum Icon {
    Graph,
    Branch,
    Layers,
    Files,
    Archive,
    Conflict,
    Cloud,
    Code,
    Settings,
    Folder,
}
pub(super) fn icon(painter: &egui::Painter, rect: egui::Rect, kind: Icon, color: Color32) {
    let origin = rect.center() - Vec2::splat(8.);
    let point = |x: f32, y: f32| origin + egui::vec2(x, y);
    let stroke = Stroke::new(1.5, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        painter.line_segment([point(a.0, a.1), point(b.0, b.1)], stroke);
    };
    let node = |x: f32, y: f32| {
        painter.circle_stroke(point(x, y), 2.2, stroke);
    };
    match kind {
        Icon::Graph | Icon::Branch => {
            line((4., 3.), (4., 13.));
            line((4., 9.), (12., 5.));
            node(4., 2.);
            node(4., 14.);
            node(12., 3.);
        }
        Icon::Layers => {
            for y in [2., 7., 12.] {
                line((1., y + 2.), (8., y - 1.));
                line((8., y - 1.), (15., y + 2.));
                line((1., y + 2.), (8., y + 5.));
                line((8., y + 5.), (15., y + 2.));
            }
        }
        Icon::Files | Icon::Archive => {
            painter.rect_stroke(
                egui::Rect::from_min_max(point(3., 2.), point(13., 15.)),
                2.,
                stroke,
                egui::StrokeKind::Inside,
            );
            line((6., 6.), (10., 6.));
            line((6., 9.), (11., 9.));
        }
        Icon::Conflict => {
            line((2., 14.), (8., 2.));
            line((8., 2.), (14., 14.));
            line((14., 14.), (2., 14.));
            line((8., 6.), (8., 9.));
            node(8., 12.);
        }
        Icon::Cloud => {
            painter.circle_stroke(point(6., 7.), 4., stroke);
            painter.circle_stroke(point(11., 9.), 4., stroke);
            line((3., 13.), (13., 13.));
        }
        Icon::Code => {
            line((5., 3.), (1., 8.));
            line((1., 8.), (5., 13.));
            line((11., 3.), (15., 8.));
            line((15., 8.), (11., 13.));
        }
        Icon::Settings => {
            painter.circle_stroke(point(8., 8.), 6., stroke);
            node(8., 8.);
        }
        Icon::Folder => {
            painter.rect_stroke(
                egui::Rect::from_min_max(point(1., 5.), point(15., 14.)),
                2.,
                stroke,
                egui::StrokeKind::Inside,
            );
            line((2., 5.), (2., 2.));
            line((2., 2.), (7., 2.));
            line((7., 2.), (9., 5.));
        }
    }
}

// Draw disclosure geometry instead of relying on a symbol in the active font.
pub(super) fn menu_chevron(ui: &egui::Ui, response: &egui::Response) {
    let center = response.rect.right_center() - egui::vec2(10., 0.);
    ui.painter().add(egui::Shape::line(
        vec![
            center + egui::vec2(-3., -1.5),
            center + egui::vec2(0., 1.5),
            center + egui::vec2(3., -1.5),
        ],
        egui::Stroke::new(1.25, INK()),
    ));
}

/// A predictable toolbar target that reserves space for its disclosure indicator.
pub(super) fn toolbar_selector<R>(
    ui: &mut egui::Ui,
    width: f32,
    caption: &str,
    value: &str,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::Response {
    let response = ui
        .allocate_ui_with_layout(
            egui::vec2(width, 48.),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                egui::containers::menu::MenuButton::from_button(
                    egui::Button::new("").min_size(egui::vec2(width, 48.)),
                )
                .config(
                    egui::containers::menu::MenuConfig::new()
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                )
                .ui(ui, contents)
                .0
            },
        )
        .inner;
    let text_clip = egui::Rect::from_min_max(
        response.rect.min + egui::vec2(10., 4.),
        response.rect.max - egui::vec2(25., 4.),
    );
    let painter = ui.painter().with_clip_rect(text_clip);
    painter.text(
        response.rect.min + egui::vec2(11., 7.),
        egui::Align2::LEFT_TOP,
        caption,
        egui::FontId::proportional(11.),
        MUTED(),
    );
    let mut job = egui::text::LayoutJob::simple(
        value.to_owned(),
        egui::FontId::proportional(14.),
        INK(),
        text_clip.width(),
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    painter.galley(response.rect.min + egui::vec2(11., 23.), galley, INK());
    menu_chevron(ui, &response);
    response
}

pub(super) fn paint_file_label(ui: &egui::Ui, rect: egui::Rect, text: &str) {
    let clip = rect.shrink2(egui::vec2(8., 2.));
    let font = egui::FontId::proportional(14.);
    let available = clip.width().max(0.);
    let full = ui.fonts_mut(|fonts| fonts.layout_no_wrap(text.to_owned(), font.clone(), INK()));
    let galley = if full.size().x <= available {
        full
    } else {
        // Keep both ends of a filename visible, including extensions and distinguishing suffixes.
        let chars: Vec<char> = text.chars().collect();
        let mut low = 0;
        let mut high = chars.len();
        let mut best = ui.fonts_mut(|fonts| fonts.layout_no_wrap("…".into(), font.clone(), INK()));
        while low < high {
            let count = (low + high).div_ceil(2);
            let prefix = count.div_ceil(2);
            let suffix = count - prefix;
            let label = format!(
                "{}…{}",
                chars[..prefix].iter().collect::<String>(),
                chars[chars.len() - suffix..].iter().collect::<String>()
            );
            let candidate = ui.fonts_mut(|fonts| fonts.layout_no_wrap(label, font.clone(), INK()));
            if candidate.size().x <= available {
                best = candidate;
                low = count;
            } else {
                high = count - 1;
            }
        }
        best
    };
    let y = rect.center().y - galley.size().y / 2.;
    ui.painter()
        .with_clip_rect(clip.intersect(ui.clip_rect()))
        .galley(egui::pos2(clip.left(), y), galley, INK());
}

pub(super) fn shorten_label(value: &str, max_chars: usize) -> String {
    if value.chars().count() > max_chars {
        format!(
            "{}…",
            value
                .chars()
                .take(max_chars.saturating_sub(1))
                .collect::<String>()
        )
    } else {
        value.to_owned()
    }
}

/// Full-width picker row with an accessible name and a concise secondary line.
pub(super) fn picker_row(
    ui: &mut egui::Ui,
    title: &str,
    detail: &str,
    selected: bool,
    enabled: bool,
) -> egui::Response {
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            ui.add_sized(
                [ui.available_width(), 48.],
                egui::Button::new("")
                    .selected(selected)
                    .frame_when_inactive(selected),
            )
        })
        .inner;
    let clip = response
        .rect
        .shrink2(egui::vec2(10., 3.))
        .intersect(ui.clip_rect());
    let painter = ui.painter().with_clip_rect(clip);
    for (text, size, color, y) in [(title, 14., INK(), 5.), (detail, 11., MUTED(), 25.)] {
        let mut job = egui::text::LayoutJob::simple(
            text.to_owned(),
            egui::FontId::proportional(size),
            color,
            (clip.width() - 4.).max(0.),
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        painter.galley(response.rect.min + egui::vec2(10., y), galley, color);
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::Role::Button,
            enabled,
            selected,
            format!("{title} — {detail}"),
        )
    });
    response.on_hover_text(format!("{title}\n{detail}"))
}

/// A visible pointer handle; callers also provide a numeric keyboard control.
pub(super) fn tree_divider(
    ui: &mut egui::Ui,
    id: &'static str,
    x: f32,
    top: f32,
) -> egui::Response {
    let hit = egui::Rect::from_min_size(egui::pos2(x - 12., top), egui::vec2(24., 36.));
    let response = ui
        .interact(hit, egui::Id::unique(id), egui::Sense::drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
        .on_hover_text("Drag to resize the source column. Use Tree layout for keyboard controls.");
    let color = if response.hovered() || response.dragged() {
        ACCENT()
    } else {
        LINE()
    };
    ui.painter().line_segment(
        [egui::pos2(x, top + 6.), egui::pos2(x, top + 30.)],
        egui::Stroke::new(if response.hovered() { 2. } else { 1. }, color),
    );
    response
}

/// Preserve visible keyboard and hover feedback for diagram nodes with custom colors.
pub(super) fn diagram_focus(ui: &egui::Ui, response: &egui::Response) {
    if response.has_focus() || response.hovered() {
        let stroke = egui::Stroke::new(if response.has_focus() { 2. } else { 1. }, ACCENT());
        ui.painter().rect_stroke(
            response.rect.shrink(1.),
            6.,
            stroke,
            egui::StrokeKind::Inside,
        );
    }
}

#[cfg(test)]
mod contrast_tests {
    use super::*;
    fn luminance(c: Color32) -> f64 {
        let component = |v: u8| {
            let v = v as f64 / 255.;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * component(c.r()) + 0.7152 * component(c.g()) + 0.0722 * component(c.b())
    }
    fn ratio(a: Color32, b: Color32) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
    #[test]
    fn semantic_text_meets_aa_on_main_surfaces() {
        for light in [false, true] {
            LIGHT.with(|v| v.set(light));
            for bg in [PAPER(), SURFACE(), SIDEBAR()] {
                for fg in [INK(), MUTED(), ACCENT(), SUCCESS(), WARNING(), DANGER()] {
                    assert!(ratio(bg, fg) >= 4.5, "theme={light} bg={bg:?} fg={fg:?}");
                }
            }
            assert!(
                ratio(PRIMARY(), INK()) >= 4.5,
                "primary button theme={light}"
            );
        }
        LIGHT.with(|v| v.set(false));
    }
}

#[cfg(all(test, feature = "ui-preview"))]
mod bundled_font_tests {
    use super::*;
    #[test]
    fn bundled_fonts_render_ui_symbols_without_system_fonts() {
        let mut harness = egui_kittest::Harness::builder()
            .with_context_setup(|ctx| ctx.set_fonts(bundled_fonts()))
            .build_ui(|ui| {
                ui.label("→ ← ↑ ↓ ↔ − … · × Δ – —");
                ui.monospace("→ ← ↑ ↓ ↔ − … · × Δ – —");
            });
        // The harness uses MissingGlyphPolicy::Panic, so a missing symbol fails this test.
        harness.run_steps(3);
    }
}
