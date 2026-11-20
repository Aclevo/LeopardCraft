//! Cheetah3D-inspired light chrome: palette, spacing, section headers and
//! compact inspector/tab helpers. All visual assets are original.

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Response, RichText, Sense, Shape, Stroke,
    StrokeKind, TextStyle, Ui, Vec2,
};
use glam::Vec3;

pub const MENU: Color32 = Color32::from_rgb(247, 247, 247);
pub const CHROME: Color32 = Color32::from_rgb(236, 236, 236);
pub const TOOLBAR: Color32 = Color32::from_rgb(227, 227, 227);
pub const PANEL: Color32 = Color32::from_rgb(239, 239, 239);
pub const PANEL_DARK: Color32 = Color32::from_rgb(228, 228, 228);
pub const HEADER: Color32 = Color32::from_rgb(248, 248, 248);
pub const GROUP: Color32 = Color32::from_rgb(224, 224, 224);
pub const GROUP_HI: Color32 = Color32::from_rgb(242, 242, 242);
pub const FIELD: Color32 = Color32::WHITE;
pub const BORDER: Color32 = Color32::from_rgb(191, 191, 191);
pub const BORDER_SOFT: Color32 = Color32::from_rgb(206, 206, 206);
pub const TEXT: Color32 = Color32::from_rgb(31, 31, 31);
pub const TEXT_WEAK: Color32 = Color32::from_rgb(110, 110, 110);
pub const DISABLED: Color32 = Color32::from_rgb(158, 158, 158);
pub const SELECTION: Color32 = Color32::from_rgb(46, 111, 208);
pub const SELECTION_SOFT: Color32 = Color32::from_rgb(205, 221, 242);
pub const ACCENT: Color32 = Color32::from_rgb(240, 138, 36);
pub const RECORD: Color32 = Color32::from_rgb(214, 59, 47);
pub const HOVER: Color32 = Color32::from_rgb(245, 247, 250);
pub const PRESS: Color32 = Color32::from_rgb(213, 226, 243);
pub const VIEWPORT_TOP: Color32 = Color32::from_rgb(100, 116, 130);
pub const VIEWPORT_BOTTOM: Color32 = Color32::from_rgb(170, 179, 186);

pub fn install(ctx: &egui::Context) {
    let mut style = (*ctx.style_of(egui::Theme::Light)).clone();
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = FIELD;
    visuals.faint_bg_color = Color32::from_rgb(244, 244, 244);
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = SELECTION;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.hyperlink_color = SELECTION;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_fill = PANEL;
    visuals.menu_corner_radius = egui::CornerRadius::same(3);
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_SOFT);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(243, 243, 243);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(243, 243, 243);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_SOFT);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(3);
    visuals.widgets.hovered.bg_fill = HOVER;
    visuals.widgets.hovered.weak_bg_fill = HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, SELECTION);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(3);
    visuals.widgets.active.bg_fill = PRESS;
    visuals.widgets.active.weak_bg_fill = PRESS;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, SELECTION);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(3);
    visuals.widgets.open.bg_fill = PRESS;
    visuals.widgets.open.weak_bg_fill = PRESS;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, SELECTION);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(3);
    style.visuals = visuals;

    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(10.0));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(11.5));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(11.5));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(12.5));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(10.5));
    style.spacing.item_spacing = Vec2::new(5.0, 3.0);
    style.spacing.button_padding = Vec2::new(6.0, 2.0);
    style.spacing.interact_size = Vec2::new(26.0, 18.0);
    style.spacing.indent = 14.0;
    style.spacing.scroll.bar_width = 9.0;
    style.spacing.menu_margin = egui::Margin::same(4);
    ctx.set_theme(egui::ThemePreference::Light);
    ctx.set_style_of(egui::Theme::Light, style.clone());
    ctx.set_style_of(egui::Theme::Dark, style);
}

pub fn panel_frame(fill: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(7, 4))
}

pub fn section(ui: &mut Ui, salt: &str, title: &str, add: impl FnOnce(&mut Ui)) {
    let id = ui.make_persistent_id(("lc-section", salt));
    let open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(true);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 17.0), Sense::click());
    section_header(ui, rect, title, open, response.hovered());
    if response.clicked() {
        ui.data_mut(|d| d.insert_temp(id, !open));
    }
    if open {
        add(ui);
        ui.add_space(4.0);
    }
}

pub fn group_title(ui: &mut Ui, title: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 17.0), Sense::hover());
    section_header(ui, rect, title, true, false);
}

fn section_header(ui: &Ui, rect: Rect, title: &str, open: bool, hovered: bool) {
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, if hovered { GROUP_HI } else { GROUP });
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, Color32::from_rgb(198, 198, 198)),
    );
    painter.line_segment(
        [rect.left_top(), rect.right_top()],
        Stroke::new(1.0, Color32::WHITE),
    );
    let c = Pos2::new(rect.left() + 9.0, rect.center().y);
    let tri = if open {
        vec![
            c + Vec2::new(-3.5, -2.0),
            c + Vec2::new(3.5, -2.0),
            c + Vec2::new(0.0, 2.5),
        ]
    } else {
        vec![
            c + Vec2::new(-2.0, -3.5),
            c + Vec2::new(2.5, 0.0),
            c + Vec2::new(-2.0, 3.5),
        ]
    };
    painter.add(Shape::convex_polygon(tri, TEXT, Stroke::NONE));
    painter.text(
        Pos2::new(rect.center().x, rect.center().y),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(11.0),
        TEXT,
    );
}

pub fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(82.0, 18.0), Sense::hover());
        ui.painter().text(
            Pos2::new(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(11.0),
            TEXT,
        );
        add(ui);
    });
}

pub fn field(ui: &mut Ui, value: &mut f32, speed: f32, width: f32) -> bool {
    ui.add_sized(
        [width, 18.0],
        egui::DragValue::new(value).speed(speed).max_decimals(3),
    )
    .changed()
}

pub fn vec3_row(ui: &mut Ui, label: &str, value: &mut Vec3, speed: f32) -> bool {
    let mut changed = false;
    row(ui, label, |ui| {
        changed |= field(ui, &mut value.x, speed, 57.0);
        changed |= field(ui, &mut value.y, speed, 57.0);
        changed |= field(ui, &mut value.z, speed, 57.0);
    });
    changed
}

pub fn toolbar_sep(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(7.0, 28.0), Sense::hover());
    ui.painter().line_segment(
        [
            Pos2::new(rect.center().x, rect.top() + 5.0),
            Pos2::new(rect.center().x, rect.bottom() - 5.0),
        ],
        Stroke::new(1.0, Color32::from_rgb(190, 190, 190)),
    );
}

/// Small light segmented tab row (3D view / UVs / Nodes and browser tabs).
pub fn tabs<T: PartialEq + Copy>(ui: &mut Ui, current: &mut T, items: &[(&str, T)]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        for (label, value) in items {
            let selected = current == value;
            let text = RichText::new(*label).size(11.0).color(if selected {
                TEXT
            } else {
                Color32::from_rgb(74, 74, 74)
            });
            let mut button = egui::Button::new(text)
                .fill(if selected {
                    Color32::WHITE
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(if selected {
                    Stroke::new(1.0, BORDER)
                } else {
                    Stroke::NONE
                })
                .min_size(Vec2::new(0.0, 20.0));
            button = button.corner_radius(egui::CornerRadius::same(2));
            if ui.add(button).clicked() && !selected {
                *current = *value;
            }
        }
    });
}

/// A thin full-width separator used between chrome rows.
pub fn rule(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, BORDER),
    );
}

pub fn empty(ui: &mut Ui, text: &str) {
    ui.add_space(18.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(text).size(11.0).color(TEXT_WEAK));
    });
}

pub fn window_header(ui: &mut Ui, title: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 22.0),
        Sense::click_and_drag(),
    );
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, HEADER);
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, BORDER),
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(11.0),
        TEXT,
    );
    response
}

/// Blue/white 2D sphere used by the material shelf when a texture preview is
/// not available. The path tracer remains the source of truth for materials.
pub fn material_ball(ui: &Ui, rect: Rect, color: Vec3, roughness: f32, selected: bool) {
    let p = ui.painter();
    let base = Vec3::new(
        color.x.clamp(0.0, 1.0),
        color.y.clamp(0.0, 1.0),
        color.z.clamp(0.0, 1.0),
    );
    let r = rect.width().min(rect.height()) * 0.5 - 2.0;
    let c = rect.center();
    p.circle_filled(
        c + Vec2::new(1.0, 2.5),
        r + 1.0,
        Color32::from_black_alpha(40),
    );
    let light = Vec2::new(-0.32, -0.38);
    let steps = 18;
    for i in (0..=steps).rev() {
        let t = i as f32 / steps as f32;
        let radius = r * (0.16 + 0.84 * t);
        let center = c + light * r * (1.0 - t) * 0.38;
        let shade = (0.30 + 0.94 * (1.0 - t)).min(1.24);
        let col = Color32::from_rgb(
            (base.x * shade * 255.0).clamp(0.0, 255.0) as u8,
            (base.y * shade * 255.0).clamp(0.0, 255.0) as u8,
            (base.z * shade * 255.0).clamp(0.0, 255.0) as u8,
        );
        p.circle_filled(center, radius, col);
    }
    let highlight = 0.95 - roughness.clamp(0.02, 1.0) * 0.55;
    let hl = Color32::from_rgba_unmultiplied(255, 255, 255, (55.0 + highlight * 145.0) as u8);
    p.circle_filled(c + light * r * 0.46, r * 0.20, hl);
    p.circle_stroke(
        c,
        r,
        Stroke::new(
            if selected { 2.0 } else { 1.0 },
            if selected {
                SELECTION
            } else {
                Color32::from_rgb(120, 120, 120)
            },
        ),
    );
}

pub fn close_icon(ui: &mut Ui, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let p = ui.painter();
    let stroke = Stroke::new(1.2, TEXT_WEAK);
    p.line_segment(
        [
            rect.left_top() + Vec2::splat(4.0),
            rect.right_bottom() - Vec2::splat(4.0),
        ],
        stroke,
    );
    p.line_segment(
        [
            rect.right_top() + Vec2::new(-4.0, 4.0),
            rect.left_bottom() + Vec2::new(4.0, -4.0),
        ],
        stroke,
    );
    response
}

pub fn rect_stroke(ui: &Ui, rect: Rect, color: Color32) {
    ui.painter()
        .rect_stroke(rect, 0.0, Stroke::new(1.0, color), StrokeKind::Inside);
}
