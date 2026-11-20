//! Original vector iconography for the Cheetah-style chrome. Everything is
//! painted with egui primitives; no third-party icon set or bitmap is used.

use crate::theme;
use eframe::egui::{
    Align2, Color32, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui,
    Vec2,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Select,
    Move,
    Rotate,
    Scale,
    ObjectMode,
    PointMode,
    EdgeMode,
    PolygonMode,
    Box,
    Ball,
    Cylinder,
    Cone,
    Torus,
    Plane,
    EditableMesh,
    Spline,
    Folder,
    Joint,
    Camera,
    PointLight,
    DirectionalLight,
    AreaLight,
    Subdivide,
    Render,
    Manager,
    Save,
    Grid,
    Fit,
    ZoomIn,
    ZoomOut,
    Play,
    Pause,
    Start,
    End,
    StepBack,
    StepForward,
    Record,
    Eye,
    EyeOff,
    RenderOff,
    Lock,
    Unlock,
    Trash,
    Info,
    Stop,
    Curves,
    Material,
    Nodes,
    Shading,
    Search,
}

pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str, enabled: bool) -> Response {
    icon_button_toggle(ui, icon, tooltip, false, enabled)
}

pub fn icon_button_toggle(
    ui: &mut Ui,
    icon: Icon,
    tooltip: &str,
    selected: bool,
    enabled: bool,
) -> Response {
    icon_button_sized(ui, icon, tooltip, selected, enabled, 28.0)
}

fn button_sense(enabled: bool) -> Sense {
    if enabled {
        Sense::click()
    } else {
        Sense::hover()
    }
}

pub fn icon_button_sized(
    ui: &mut Ui,
    icon: Icon,
    tooltip: &str,
    selected: bool,
    enabled: bool,
    size: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), button_sense(enabled));
    let painter = ui.painter();
    if enabled && selected {
        painter.rect_filled(rect, 3.0, theme::PRESS);
        painter.rect_stroke(
            rect,
            3.0,
            Stroke::new(1.0, Color32::from_rgb(150, 177, 212)),
            StrokeKind::Inside,
        );
    } else if enabled && response.hovered() {
        painter.rect_filled(rect, 3.0, theme::HOVER);
    }
    if response.is_pointer_button_down_on() && enabled {
        painter.rect_filled(rect, 3.0, theme::PRESS);
    }
    let tint = if enabled {
        theme::TEXT
    } else {
        theme::DISABLED
    };
    paint(painter, rect.shrink(size * 0.18), icon, tint);
    if enabled {
        response.on_hover_text(tooltip)
    } else {
        response.on_hover_text(format!("{tooltip} (unavailable)"))
    }
}

pub fn icon_label_button(ui: &mut Ui, icon: Icon, label: &str, enabled: bool) -> Response {
    let width = 28.0 + label.chars().count() as f32 * 6.2;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 38.0), button_sense(enabled));
    let painter = ui.painter();
    if enabled && response.hovered() {
        painter.rect_filled(rect, 3.0, theme::HOVER);
    }
    if response.is_pointer_button_down_on() && enabled {
        painter.rect_filled(rect, 3.0, theme::PRESS);
    }
    let tint = if enabled {
        theme::TEXT
    } else {
        theme::DISABLED
    };
    let icon_rect = Rect::from_min_size(
        Pos2::new(rect.left() + 3.0, rect.top() + 8.0),
        Vec2::splat(20.0),
    );
    paint(painter, icon_rect, icon, tint);
    painter.text(
        Pos2::new(icon_rect.right() + 4.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(10.5),
        tint,
    );
    if enabled {
        response.on_hover_text(label)
    } else {
        response
    }
}

pub fn paint(painter: &Painter, rect: Rect, icon: Icon, tint: Color32) {
    match icon {
        Icon::Select => cursor(painter, rect, tint),
        Icon::Move => arrows(painter, rect, tint),
        Icon::Rotate => rotate(painter, rect, tint),
        Icon::Scale => scale(painter, rect, tint),
        Icon::ObjectMode => cube_outline(painter, rect, tint, false),
        Icon::PointMode => {
            for (dx, dy) in [(-0.42, 0.35), (0.0, -0.4), (0.42, 0.35)] {
                painter.circle_filled(
                    rect.center() + Vec2::new(dx * rect.width(), dy * rect.height()),
                    2.1,
                    tint,
                );
            }
        }
        Icon::EdgeMode => {
            painter.line_segment(
                [
                    rect.left_bottom() + Vec2::new(3.0, -3.0),
                    rect.right_top() + Vec2::new(-3.0, 3.0),
                ],
                Stroke::new(1.5, tint),
            );
            painter.circle_filled(rect.left_bottom() + Vec2::new(3.0, -3.0), 2.0, tint);
            painter.circle_filled(rect.right_top() + Vec2::new(-3.0, 3.0), 2.0, tint);
        }
        Icon::PolygonMode => {
            let pts = vec![
                rect.center() + Vec2::new(0.0, -rect.height() * 0.38),
                rect.center() + Vec2::new(rect.width() * 0.42, 0.0),
                rect.center() + Vec2::new(0.0, rect.height() * 0.38),
                rect.center() + Vec2::new(-rect.width() * 0.42, 0.0),
            ];
            painter.add(Shape::convex_polygon(pts, tint, Stroke::NONE));
        }
        Icon::Box => box_icon(painter, rect),
        Icon::Ball => ball_icon(painter, rect, Vec2::ZERO),
        Icon::Cylinder => cylinder_icon(painter, rect),
        Icon::Cone => cone_icon(painter, rect),
        Icon::Torus => torus_icon(painter, rect),
        Icon::Plane => plane_icon(painter, rect),
        Icon::EditableMesh => editable_mesh_icon(painter, rect, tint),
        Icon::Spline => spline_icon(painter, rect),
        Icon::Folder => folder_icon(painter, rect),
        Icon::Joint => joint_icon(painter, rect),
        Icon::Camera => camera_icon(painter, rect),
        Icon::PointLight => light_icon(painter, rect, false),
        Icon::DirectionalLight => directional_light_icon(painter, rect),
        Icon::AreaLight => light_icon(painter, rect, true),
        Icon::Subdivide => subdivide_icon(painter, rect, tint),
        Icon::Render => render_icon(painter, rect),
        Icon::Manager => clock_icon(painter, rect),
        Icon::Save => save_icon(painter, rect),
        Icon::Grid => grid_icon(painter, rect, tint),
        Icon::Fit => fit_icon(painter, rect, tint),
        Icon::ZoomIn => zoom_icon(painter, rect, tint, true),
        Icon::ZoomOut => zoom_icon(painter, rect, tint, false),
        Icon::Play => triangle(painter, rect, tint),
        Icon::Pause => {
            let w = rect.width() * 0.18;
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(rect.center().x - w * 2.2, rect.top() + 3.0),
                    Vec2::new(w, rect.height() - 6.0),
                ),
                0.0,
                tint,
            );
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(rect.center().x + w * 1.2, rect.top() + 3.0),
                    Vec2::new(w, rect.height() - 6.0),
                ),
                0.0,
                tint,
            );
        }
        Icon::Start => {
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(rect.left() + 2.0, rect.top() + 4.0),
                    Vec2::new(2.0, rect.height() - 8.0),
                ),
                0.0,
                tint,
            );
            triangle_at(painter, rect.translate(Vec2::new(3.0, 0.0)), tint);
        }
        Icon::End => {
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(rect.right() - 4.0, rect.top() + 4.0),
                    Vec2::new(2.0, rect.height() - 8.0),
                ),
                0.0,
                tint,
            );
            triangle_left(painter, rect.translate(Vec2::new(-3.0, 0.0)), tint);
        }
        Icon::StepBack => triangle_left(painter, rect, tint),
        Icon::StepForward => triangle(painter, rect, tint),
        Icon::Record => {
            painter.circle_filled(
                rect.center(),
                rect.width().min(rect.height()) * 0.36,
                theme::RECORD,
            );
        }
        Icon::Eye => eye_icon(painter, rect, tint, true),
        Icon::EyeOff => eye_icon(painter, rect, tint, false),
        Icon::RenderOff => {
            camera_icon(painter, rect);
            painter.line_segment(
                [rect.left_bottom(), rect.right_top()],
                Stroke::new(1.6, theme::RECORD),
            );
        }
        Icon::Lock => lock_icon(painter, rect, tint, true),
        Icon::Unlock => lock_icon(painter, rect, tint, false),
        Icon::Trash => trash_icon(painter, rect, tint),
        Icon::Info => {
            painter.circle_stroke(rect.center(), rect.width() * 0.42, Stroke::new(1.2, tint));
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "i",
                FontId::proportional(rect.height() * 0.72),
                tint,
            );
        }
        Icon::Stop => {
            let color = if tint == theme::DISABLED {
                tint
            } else {
                theme::RECORD
            };
            painter.rect_filled(rect.shrink(3.0), 1.0, color);
        }
        Icon::Curves => {
            let pts = vec![
                rect.left_bottom() + Vec2::new(2.0, -4.0),
                rect.left_top() + Vec2::new(rect.width() * 0.32, rect.height() * 0.25),
                rect.right_top() + Vec2::new(-rect.width() * 0.28, rect.height() * 0.7),
                rect.right_top() + Vec2::new(-2.0, 2.0),
            ];
            painter.add(Shape::line(pts, Stroke::new(1.3, tint)));
            painter.circle_filled(rect.left_bottom() + Vec2::new(2.0, -4.0), 1.8, tint);
            painter.circle_filled(rect.right_top() + Vec2::new(-2.0, 2.0), 1.8, tint);
        }
        Icon::Material => ball_icon(painter, rect.shrink(1.0), Vec2::ZERO),
        Icon::Nodes => {
            let a = Rect::from_min_size(rect.left_top() + Vec2::new(0.0, 1.0), Vec2::new(7.0, 5.0));
            let b =
                Rect::from_min_size(rect.right_top() + Vec2::new(-7.0, 1.0), Vec2::new(7.0, 5.0));
            let c = Rect::from_min_size(
                rect.center_bottom() + Vec2::new(-4.0, -6.0),
                Vec2::new(8.0, 5.0),
            );
            for r in [a, b, c] {
                painter.rect_filled(r, 1.0, tint);
            }
            painter.line_segment([a.right_center(), b.left_center()], Stroke::new(1.0, tint));
            painter.line_segment([a.center_bottom(), c.center_top()], Stroke::new(1.0, tint));
            painter.line_segment([b.center_bottom(), c.center_top()], Stroke::new(1.0, tint));
        }
        Icon::Shading => {
            painter.circle_filled(
                rect.center(),
                rect.width() * 0.42,
                Color32::from_rgb(220, 220, 220),
            );
            painter.add(Shape::convex_polygon(
                vec![
                    rect.center() + Vec2::new(0.0, -rect.height() * 0.42),
                    rect.center() + Vec2::new(0.0, rect.height() * 0.42),
                    rect.center() + Vec2::new(rect.width() * 0.42, 0.0),
                ],
                Color32::from_rgb(90, 100, 112),
                Stroke::NONE,
            ));
            painter.circle_stroke(rect.center(), rect.width() * 0.42, Stroke::new(1.0, tint));
        }
        Icon::Search => search_icon(painter, rect, tint),
    }
}

fn search_icon(p: &Painter, r: Rect, c: Color32) {
    let radius = r.width().min(r.height()) * 0.30;
    let center = r.center() + Vec2::new(-1.2, -1.2);
    p.circle_stroke(center, radius, Stroke::new(1.5, c));
    let start = center + Vec2::splat(radius * 0.72);
    p.line_segment(
        [start, r.right_bottom() - Vec2::new(1.4, 1.4)],
        Stroke::new(1.6, c),
    );
}

fn cursor(p: &Painter, r: Rect, c: Color32) {
    let pts = vec![
        Pos2::new(r.left() + r.width() * 0.18, r.top() + r.height() * 0.08),
        Pos2::new(r.left() + r.width() * 0.18, r.bottom() - r.height() * 0.12),
        Pos2::new(r.left() + r.width() * 0.4, r.bottom() - r.height() * 0.34),
        Pos2::new(r.left() + r.width() * 0.55, r.bottom() - r.height() * 0.06),
        Pos2::new(r.left() + r.width() * 0.68, r.bottom() - r.height() * 0.16),
        Pos2::new(r.left() + r.width() * 0.53, r.bottom() - r.height() * 0.43),
        Pos2::new(r.left() + r.width() * 0.82, r.bottom() - r.height() * 0.42),
    ];
    p.add(Shape::convex_polygon(
        pts,
        c,
        Stroke::new(1.0, Color32::WHITE),
    ));
}

fn arrows(p: &Painter, r: Rect, c: Color32) {
    let center = r.center();
    let s = Stroke::new(1.4, c);
    p.line_segment(
        [
            center + Vec2::new(-r.width() * 0.35, 0.0),
            center + Vec2::new(r.width() * 0.35, 0.0),
        ],
        s,
    );
    p.line_segment(
        [
            center + Vec2::new(0.0, -r.height() * 0.35),
            center + Vec2::new(0.0, r.height() * 0.35),
        ],
        s,
    );
    for (dir, tip) in [
        (Vec2::X, center + Vec2::new(r.width() * 0.38, 0.0)),
        (
            Vec2::new(-1.0, 0.0),
            center + Vec2::new(-r.width() * 0.38, 0.0),
        ),
        (Vec2::Y, center + Vec2::new(0.0, r.height() * 0.38)),
        (
            Vec2::new(0.0, -1.0),
            center + Vec2::new(0.0, -r.height() * 0.38),
        ),
    ] {
        let n = Vec2::new(-dir.y, dir.x) * 2.2;
        p.add(Shape::convex_polygon(
            vec![tip, tip - dir * 4.0 + n, tip - dir * 4.0 - n],
            c,
            Stroke::NONE,
        ));
    }
    p.circle_filled(center, 1.8, c);
}

fn rotate(p: &Painter, r: Rect, c: Color32) {
    let pts = ellipse(r.center(), r.width() * 0.36, r.height() * 0.32, 24);
    p.add(Shape::line(pts[3..21].to_vec(), Stroke::new(1.4, c)));
    let tip = pts[3];
    p.add(Shape::convex_polygon(
        vec![
            tip + Vec2::new(-1.0, -4.0),
            tip + Vec2::new(4.0, 0.0),
            tip + Vec2::new(-2.0, 2.0),
        ],
        c,
        Stroke::NONE,
    ));
    p.circle_filled(r.center(), 1.5, c);
}

fn scale(p: &Painter, r: Rect, c: Color32) {
    let a = r.left_bottom() + Vec2::new(4.0, -4.0);
    let b = r.right_top() + Vec2::new(-4.0, 4.0);
    p.line_segment([a, b], Stroke::new(1.4, c));
    for tip in [a, b] {
        let dir = if tip == a {
            Vec2::new(-0.7, 0.7)
        } else {
            Vec2::new(0.7, -0.7)
        };
        let n = Vec2::new(-dir.y, dir.x) * 2.2;
        p.add(Shape::convex_polygon(
            vec![tip, tip - dir * 4.0 + n, tip - dir * 4.0 - n],
            c,
            Stroke::NONE,
        ));
    }
    p.rect_stroke(
        Rect::from_center_size(r.center(), Vec2::splat(r.width() * 0.32)),
        1.0,
        Stroke::new(1.0, c),
        StrokeKind::Inside,
    );
}

fn cube_outline(p: &Painter, r: Rect, c: Color32, fill: bool) {
    let pts = iso_cube_points(r);
    if fill {
        for (face, col) in [
            (pts.0, Color32::from_rgb(200, 92, 82)),
            (pts.1, Color32::from_rgb(150, 60, 52)),
            (pts.2, Color32::from_rgb(176, 74, 64)),
        ] {
            p.add(Shape::convex_polygon(
                face.to_vec(),
                col,
                Stroke::new(1.0, c),
            ));
        }
    } else {
        for face in [pts.0, pts.1, pts.2] {
            p.add(Shape::closed_line(face.to_vec(), Stroke::new(1.0, c)));
        }
    }
}

struct CubeFaces([Pos2; 4], [Pos2; 4], [Pos2; 4]);

fn iso_cube_points(r: Rect) -> CubeFaces {
    let l = r.left() + 1.0;
    let rr = r.right() - 1.0;
    let t = r.top() + 1.0;
    let b = r.bottom() - 2.0;
    let cx = r.center().x;
    let my = r.center().y;
    CubeFaces(
        [
            Pos2::new(cx, t),
            Pos2::new(rr, my - 2.5),
            Pos2::new(cx, my + 2.0),
            Pos2::new(l, my - 2.5),
        ],
        [
            Pos2::new(l, my - 2.5),
            Pos2::new(cx, my + 2.0),
            Pos2::new(cx, b - 2.0),
            Pos2::new(l, b - 4.5),
        ],
        [
            Pos2::new(rr, my - 2.5),
            Pos2::new(cx, my + 2.0),
            Pos2::new(cx, b - 2.0),
            Pos2::new(rr, b - 4.5),
        ],
    )
}

fn box_icon(p: &Painter, r: Rect) {
    let faces = iso_cube_points(r);
    for (pts, col) in [
        (faces.0, Color32::from_rgb(232, 112, 102)),
        (faces.1, Color32::from_rgb(170, 61, 53)),
        (faces.2, Color32::from_rgb(205, 83, 72)),
    ] {
        p.add(Shape::convex_polygon(
            pts.to_vec(),
            col,
            Stroke::new(1.0, Color32::from_rgb(112, 40, 35)),
        ));
    }
    p.line_segment(
        [
            Pos2::new(r.left() + 1.0, r.center().y - 2.5),
            Pos2::new(r.center().x, r.center().y + 2.0),
        ],
        Stroke::new(0.8, Color32::from_rgb(130, 45, 39)),
    );
    p.line_segment(
        [
            Pos2::new(r.right() - 1.0, r.center().y - 2.5),
            Pos2::new(r.center().x, r.center().y + 2.0),
        ],
        Stroke::new(0.8, Color32::from_rgb(130, 45, 39)),
    );
}

fn ball_icon(p: &Painter, r: Rect, offset: Vec2) {
    let c = r.center() + offset;
    let radius = r.width().min(r.height()) * 0.44;
    p.circle_filled(c, radius, Color32::from_rgb(118, 170, 222));
    p.circle_filled(
        c + Vec2::new(radius * 0.28, radius * 0.3),
        radius * 0.9,
        Color32::from_rgba_unmultiplied(30, 62, 96, 60),
    );
    p.circle_filled(
        c + Vec2::new(-radius * 0.3, -radius * 0.36),
        radius * 0.28,
        Color32::from_rgba_unmultiplied(240, 249, 255, 200),
    );
    p.circle_stroke(c, radius, Stroke::new(1.0, Color32::from_rgb(58, 95, 136)));
}

fn cylinder_icon(p: &Painter, r: Rect) {
    let cx = r.center().x;
    let half = r.width() * 0.34;
    let top = r.top() + 3.0;
    let bot = r.bottom() - 3.0;
    let body = Rect::from_min_max(Pos2::new(cx - half, top), Pos2::new(cx + half, bot));
    p.rect_filled(body, 0.0, Color32::from_rgb(224, 190, 100));
    p.add(Shape::convex_polygon(
        vec![
            Pos2::new(cx + half, top),
            Pos2::new(cx + half, bot),
            Pos2::new(cx + half * 0.25, bot),
            Pos2::new(cx + half * 0.25, top),
        ],
        Color32::from_rgba_unmultiplied(120, 85, 25, 45),
        Stroke::NONE,
    ));
    let top_pts = ellipse(Pos2::new(cx, top), half, 3.2, 20);
    p.add(Shape::convex_polygon(
        top_pts.clone(),
        Color32::from_rgb(244, 216, 137),
        Stroke::new(1.0, Color32::from_rgb(128, 94, 33)),
    ));
    p.add(Shape::line(
        vec![top_pts[10], Pos2::new(cx + half, bot), top_pts[10]],
        Stroke::NONE,
    ));
    p.line_segment(
        [Pos2::new(cx - half, top), Pos2::new(cx - half, bot)],
        Stroke::new(1.0, Color32::from_rgb(128, 94, 33)),
    );
    p.line_segment(
        [Pos2::new(cx + half, top), Pos2::new(cx + half, bot)],
        Stroke::new(1.0, Color32::from_rgb(128, 94, 33)),
    );
}

fn cone_icon(p: &Painter, r: Rect) {
    let cx = r.center().x;
    let bot = r.bottom() - 4.0;
    let half = r.width() * 0.4;
    p.add(Shape::convex_polygon(
        vec![
            Pos2::new(cx, r.top() + 2.0),
            Pos2::new(cx + half, bot),
            Pos2::new(cx - half, bot),
        ],
        Color32::from_rgb(220, 143, 91),
        Stroke::new(1.0, Color32::from_rgb(128, 73, 38)),
    ));
    p.add(Shape::convex_polygon(
        vec![
            Pos2::new(cx, r.top() + 2.0),
            Pos2::new(cx + half * 0.4, bot),
            Pos2::new(cx + half, bot),
        ],
        Color32::from_rgba_unmultiplied(110, 56, 20, 55),
        Stroke::NONE,
    ));
    p.add(Shape::convex_polygon(
        ellipse(Pos2::new(cx, bot), half, 3.0, 18),
        Color32::from_rgb(238, 167, 116),
        Stroke::new(1.0, Color32::from_rgb(128, 73, 38)),
    ));
}

fn torus_icon(p: &Painter, r: Rect) {
    let c = r.center();
    let rx = r.width() * 0.4;
    let ry = r.height() * 0.30;
    for (scale, color) in [
        (1.0, Color32::from_rgb(119, 178, 107)),
        (0.72, Color32::from_rgb(155, 205, 135)),
    ] {
        let pts = ellipse(c, rx * scale, ry * scale, 24);
        p.add(Shape::closed_line(pts, Stroke::new(3.4, color)));
    }
    p.add(Shape::closed_line(
        ellipse(c, rx, ry, 24),
        Stroke::new(0.8, Color32::from_rgb(58, 107, 51)),
    ));
}

fn plane_icon(p: &Painter, r: Rect) {
    let c = r.center();
    let w = r.width() * 0.42;
    let h = r.height() * 0.25;
    let quad = vec![
        c + Vec2::new(0.0, -h),
        c + Vec2::new(w, 0.0),
        c + Vec2::new(0.0, h),
        c + Vec2::new(-w, 0.0),
    ];
    p.add(Shape::convex_polygon(
        quad.clone(),
        Color32::from_rgb(174, 193, 214),
        Stroke::new(1.0, Color32::from_rgb(83, 103, 126)),
    ));
    for i in 1..4 {
        let t = i as f32 / 4.0;
        p.line_segment(
            [
                quad[0] + (quad[1] - quad[0]) * t,
                quad[3] + (quad[2] - quad[3]) * t,
            ],
            Stroke::new(0.7, Color32::from_rgb(112, 132, 154)),
        );
        p.line_segment(
            [
                quad[0] + (quad[3] - quad[0]) * t,
                quad[1] + (quad[2] - quad[1]) * t,
            ],
            Stroke::new(0.7, Color32::from_rgb(112, 132, 154)),
        );
    }
}

fn editable_mesh_icon(p: &Painter, r: Rect, c: Color32) {
    let a = Pos2::new(r.center().x, r.top() + 2.0);
    let b = r.right_bottom() + Vec2::new(-2.0, -2.0);
    let ctr = r.left_bottom() + Vec2::new(2.0, -2.0);
    let ab = a + (b - a) * 0.5;
    let bc = b + (ctr - b) * 0.5;
    let ca = ctr + (a - ctr) * 0.5;
    let stroke = Stroke::new(1.0, c);
    p.add(Shape::closed_line(vec![a, b, ctr], stroke));
    p.line_segment([ab, ctr], Stroke::new(0.75, c));
    p.line_segment([bc, a], Stroke::new(0.75, c));
    p.line_segment([ca, b], Stroke::new(0.75, c));
    for vertex in [a, b, ctr] {
        p.circle_filled(vertex, 1.5, c);
    }
}

fn spline_icon(p: &Painter, r: Rect) {
    let pts = vec![
        r.left_bottom() + Vec2::new(2.0, -4.0),
        r.left_top() + Vec2::new(r.width() * 0.3, r.height() * 0.25),
        r.center_bottom() + Vec2::new(0.0, -2.0),
        r.right_top() + Vec2::new(-2.0, 5.0),
    ];
    p.add(Shape::line(
        pts.clone(),
        Stroke::new(1.6, Color32::from_rgb(200, 155, 55)),
    ));
    for point in pts {
        p.circle_filled(point, 2.0, Color32::from_rgb(93, 70, 22));
        p.circle_filled(point, 1.1, Color32::WHITE);
    }
}

fn folder_icon(p: &Painter, r: Rect) {
    let tab = Rect::from_min_size(
        r.left_top() + Vec2::new(1.0, r.height() * 0.24),
        Vec2::new(r.width() * 0.42, r.height() * 0.2),
    );
    p.rect_filled(tab, 1.0, Color32::from_rgb(214, 170, 69));
    let body = Rect::from_min_max(
        r.left_top() + Vec2::new(1.0, r.height() * 0.34),
        r.right_bottom() + Vec2::new(-1.0, -2.0),
    );
    p.rect_filled(body, 2.0, Color32::from_rgb(234, 194, 93));
    p.rect_stroke(
        body,
        2.0,
        Stroke::new(1.0, Color32::from_rgb(133, 99, 26)),
        StrokeKind::Inside,
    );
    p.line_segment(
        [
            body.left_top() + Vec2::new(1.0, 3.0),
            body.right_top() + Vec2::new(-1.0, 3.0),
        ],
        Stroke::new(1.0, Color32::from_rgb(250, 221, 145)),
    );
}

fn joint_icon(p: &Painter, r: Rect) {
    let c = r.center();
    p.add(Shape::convex_polygon(
        vec![
            c + Vec2::new(0.0, -r.height() * 0.38),
            c + Vec2::new(r.width() * 0.3, 0.0),
            c + Vec2::new(0.0, r.height() * 0.38),
            c + Vec2::new(-r.width() * 0.3, 0.0),
        ],
        Color32::from_rgb(143, 194, 128),
        Stroke::new(1.0, Color32::from_rgb(62, 103, 49)),
    ));
    p.circle_filled(c, 2.6, Color32::from_rgb(61, 105, 47));
    p.circle_stroke(c, 4.2, Stroke::new(1.0, Color32::from_rgb(68, 116, 53)));
}

fn camera_icon(p: &Painter, r: Rect) {
    let body = Rect::from_min_size(
        r.left_top() + Vec2::new(1.0, r.height() * 0.28),
        Vec2::new(r.width() - 2.0, r.height() * 0.55),
    );
    p.rect_filled(body, 2.0, Color32::from_rgb(108, 155, 210));
    p.rect_stroke(
        body,
        2.0,
        Stroke::new(1.0, Color32::from_rgb(43, 78, 116)),
        StrokeKind::Inside,
    );
    p.rect_filled(
        Rect::from_min_size(body.left_top() + Vec2::new(2.0, -3.5), Vec2::new(5.0, 3.5)),
        1.0,
        Color32::from_rgb(74, 114, 158),
    );
    p.circle_filled(
        body.center(),
        r.width() * 0.21,
        Color32::from_rgb(214, 234, 250),
    );
    p.circle_stroke(
        body.center(),
        r.width() * 0.21,
        Stroke::new(1.0, Color32::from_rgb(43, 78, 116)),
    );
    p.circle_filled(
        body.center() + Vec2::new(-1.0, -1.0),
        r.width() * 0.07,
        Color32::WHITE,
    );
}

fn directional_light_icon(p: &Painter, r: Rect) {
    let sun = r.left_top() + Vec2::new(r.width() * 0.27, r.height() * 0.28);
    let radius = r.width() * 0.16;
    p.circle_filled(sun, radius, Color32::from_rgb(244, 198, 82));
    p.circle_stroke(
        sun,
        radius,
        Stroke::new(1.0, Color32::from_rgb(139, 92, 22)),
    );
    let dir = Vec2::new(0.78, 0.42).normalized();
    let normal = Vec2::new(-dir.y, dir.x);
    for offset in [-0.22_f32, 0.0, 0.22] {
        let start = sun + normal * r.height() * offset + dir * radius * 1.6;
        let end = start + dir * r.width() * 0.58;
        let color = Color32::from_rgb(188, 126, 36);
        p.line_segment([start, end], Stroke::new(1.15, color));
        p.add(Shape::convex_polygon(
            vec![
                end,
                end - dir * 3.6 + normal * 1.7,
                end - dir * 3.6 - normal * 1.7,
            ],
            color,
            Stroke::NONE,
        ));
    }
}

fn light_icon(p: &Painter, r: Rect, area: bool) {
    let c = r.center();
    if area {
        let body = Rect::from_center_size(c, Vec2::new(r.width() * 0.5, r.height() * 0.24));
        p.rect_filled(body, 1.0, Color32::from_rgb(238, 170, 72));
        p.rect_stroke(
            body,
            1.0,
            Stroke::new(1.0, Color32::from_rgb(136, 87, 20)),
            StrokeKind::Inside,
        );
    } else {
        p.circle_filled(c, r.width() * 0.22, Color32::from_rgb(244, 198, 82));
        p.circle_stroke(
            c,
            r.width() * 0.22,
            Stroke::new(1.0, Color32::from_rgb(139, 92, 22)),
        );
    }
    for i in 0..8 {
        let a = std::f32::consts::TAU * i as f32 / 8.0;
        let dir = Vec2::new(a.cos(), a.sin());
        p.line_segment(
            [c + dir * r.width() * 0.31, c + dir * r.width() * 0.46],
            Stroke::new(1.2, Color32::from_rgb(188, 126, 36)),
        );
    }
}

fn subdivide_icon(p: &Painter, r: Rect, tint: Color32) {
    let c = r.center();
    if tint == theme::DISABLED {
        cube_outline(p, r, tint, false);
        let line = Stroke::new(0.8, tint);
        p.line_segment(
            [
                Pos2::new(c.x, r.top() + 3.0),
                Pos2::new(c.x, r.bottom() - 4.0),
            ],
            line,
        );
        p.line_segment(
            [
                Pos2::new(r.left() + 3.0, c.y + 1.0),
                Pos2::new(r.right() - 3.0, c.y + 1.0),
            ],
            line,
        );
        return;
    }
    box_icon(p, r);
    p.line_segment(
        [
            Pos2::new(c.x, r.top() + 3.0),
            Pos2::new(c.x, r.bottom() - 4.0),
        ],
        Stroke::new(0.8, Color32::WHITE),
    );
    p.line_segment(
        [
            Pos2::new(r.left() + 3.0, c.y + 1.0),
            Pos2::new(r.right() - 3.0, c.y + 1.0),
        ],
        Stroke::new(0.8, Color32::WHITE),
    );
}

fn render_icon(p: &Painter, r: Rect) {
    camera_icon(p, r);
    p.circle_filled(
        r.right_top() + Vec2::new(-3.0, 4.0),
        2.1,
        Color32::from_rgb(210, 62, 48),
    );
}

fn clock_icon(p: &Painter, r: Rect) {
    let c = r.center();
    let radius = r.width() * 0.40;
    p.circle_filled(c, radius, Color32::from_rgb(190, 205, 221));
    p.circle_stroke(c, radius, Stroke::new(1.2, Color32::from_rgb(82, 101, 121)));
    p.line_segment(
        [c, c + Vec2::new(0.0, -radius * 0.62)],
        Stroke::new(1.4, Color32::from_rgb(55, 72, 89)),
    );
    p.line_segment(
        [c, c + Vec2::new(radius * 0.45, 0.0)],
        Stroke::new(1.4, Color32::from_rgb(55, 72, 89)),
    );
    p.circle_filled(c, 1.3, Color32::from_rgb(55, 72, 89));
}

fn save_icon(p: &Painter, r: Rect) {
    let body = r.shrink(2.0);
    p.rect_filled(body, 2.0, Color32::from_rgb(102, 144, 195));
    p.rect_stroke(
        body,
        2.0,
        Stroke::new(1.0, Color32::from_rgb(40, 70, 106)),
        StrokeKind::Inside,
    );
    p.rect_filled(
        Rect::from_min_size(
            body.left_top() + Vec2::new(3.0, 2.0),
            Vec2::new(body.width() - 7.0, 4.0),
        ),
        0.0,
        Color32::from_rgb(67, 99, 136),
    );
    p.rect_filled(
        Rect::from_min_size(
            body.left_bottom() + Vec2::new(3.0, -7.0),
            Vec2::new(body.width() - 6.0, 6.0),
        ),
        1.0,
        Color32::from_rgb(235, 240, 246),
    );
}

fn grid_icon(p: &Painter, r: Rect, c: Color32) {
    for i in 0..3 {
        let t = r.left() + 2.0 + (r.width() - 4.0) * i as f32 / 2.0;
        p.line_segment(
            [Pos2::new(t, r.top() + 2.0), Pos2::new(t, r.bottom() - 2.0)],
            Stroke::new(0.8, c),
        );
        let y = r.top() + 2.0 + (r.height() - 4.0) * i as f32 / 2.0;
        p.line_segment(
            [Pos2::new(r.left() + 2.0, y), Pos2::new(r.right() - 2.0, y)],
            Stroke::new(0.8, c),
        );
    }
}

fn fit_icon(p: &Painter, r: Rect, c: Color32) {
    let s = Stroke::new(1.2, c);
    let d = 5.0;
    for (corner, dx, dy) in [
        (r.left_top(), 1.0, 1.0),
        (r.right_top(), -1.0, 1.0),
        (r.left_bottom(), 1.0, -1.0),
        (r.right_bottom(), -1.0, -1.0),
    ] {
        p.line_segment(
            [
                corner + Vec2::new(dx * 2.0, dy * 2.0),
                corner + Vec2::new(dx * 2.0, dy * (2.0 + d)),
            ],
            s,
        );
        p.line_segment(
            [
                corner + Vec2::new(dx * 2.0, dy * 2.0),
                corner + Vec2::new(dx * (2.0 + d), dy * 2.0),
            ],
            s,
        );
    }
}

fn zoom_icon(p: &Painter, r: Rect, c: Color32, plus: bool) {
    let center = r.center() + Vec2::new(-1.5, -1.5);
    let radius = r.width() * 0.29;
    p.circle_stroke(center, radius, Stroke::new(1.4, c));
    p.line_segment(
        [
            center + Vec2::new(radius * 0.7, radius * 0.7),
            r.right_bottom() - Vec2::new(2.0, 2.0),
        ],
        Stroke::new(1.6, c),
    );
    p.line_segment(
        [
            center + Vec2::new(-radius * 0.45, 0.0),
            center + Vec2::new(radius * 0.45, 0.0),
        ],
        Stroke::new(1.0, c),
    );
    if plus {
        p.line_segment(
            [
                center + Vec2::new(0.0, -radius * 0.45),
                center + Vec2::new(0.0, radius * 0.45),
            ],
            Stroke::new(1.0, c),
        );
    }
}

fn triangle(p: &Painter, r: Rect, c: Color32) {
    let inset = r.width().min(r.height()) * 0.26;
    p.add(Shape::convex_polygon(
        vec![
            Pos2::new(r.left() + inset, r.top() + inset),
            Pos2::new(r.right() - inset + 1.0, r.center().y),
            Pos2::new(r.left() + inset, r.bottom() - inset),
        ],
        c,
        Stroke::NONE,
    ));
}

fn triangle_left(p: &Painter, r: Rect, c: Color32) {
    let inset = r.width().min(r.height()) * 0.26;
    p.add(Shape::convex_polygon(
        vec![
            Pos2::new(r.right() - inset, r.top() + inset),
            Pos2::new(r.left() + inset - 1.0, r.center().y),
            Pos2::new(r.right() - inset, r.bottom() - inset),
        ],
        c,
        Stroke::NONE,
    ));
}

fn triangle_at(p: &Painter, r: Rect, c: Color32) {
    triangle(p, r, c);
}

fn eye_icon(p: &Painter, r: Rect, c: Color32, open: bool) {
    let ctr = r.center();
    let rx = r.width() * 0.42;
    let ry = r.height() * 0.26;
    let pts = ellipse(ctr, rx, ry, 24);
    p.add(Shape::closed_line(pts, Stroke::new(1.2, c)));
    p.circle_filled(ctr, r.width() * 0.12, c);
    if !open {
        p.line_segment(
            [
                r.left_bottom() + Vec2::new(2.0, -2.0),
                r.right_top() + Vec2::new(-2.0, 2.0),
            ],
            Stroke::new(1.4, theme::RECORD),
        );
    }
}

fn lock_icon(p: &Painter, r: Rect, c: Color32, locked: bool) {
    let body = Rect::from_min_size(
        r.left_top() + Vec2::new(3.0, r.height() * 0.44),
        Vec2::new(r.width() - 6.0, r.height() * 0.45),
    );
    p.rect_filled(body, 1.0, c);
    let shackle = if locked {
        ellipse(
            Pos2::new(r.center().x, r.top() + r.height() * 0.44),
            r.width() * 0.22,
            r.height() * 0.25,
            14,
        )
    } else {
        ellipse(
            Pos2::new(r.center().x + 2.0, r.top() + r.height() * 0.44),
            r.width() * 0.22,
            r.height() * 0.25,
            14,
        )
    };
    p.add(Shape::line(shackle, Stroke::new(1.4, c)));
}

fn trash_icon(p: &Painter, r: Rect, c: Color32) {
    p.line_segment(
        [
            r.left_top() + Vec2::new(4.0, 4.0),
            r.right_top() + Vec2::new(-4.0, 4.0),
        ],
        Stroke::new(1.4, c),
    );
    p.rect_stroke(
        Rect::from_min_max(
            r.left_top() + Vec2::new(5.0, 6.0),
            r.right_bottom() + Vec2::new(-5.0, -2.0),
        ),
        1.0,
        Stroke::new(1.2, c),
        StrokeKind::Inside,
    );
    for i in 0..2 {
        let x = r.center().x - 3.0 + i as f32 * 6.0;
        p.line_segment(
            [Pos2::new(x, r.top() + 8.0), Pos2::new(x, r.bottom() - 4.0)],
            Stroke::new(0.9, c),
        );
    }
}

fn ellipse(center: Pos2, rx: f32, ry: f32, segments: usize) -> Vec<Pos2> {
    (0..segments)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / segments as f32;
            center + Vec2::new(a.cos() * rx, a.sin() * ry)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::button_sense;

    #[test]
    fn disabled_buttons_do_not_accept_clicks() {
        assert!(!button_sense(false).senses_click());
        assert!(button_sense(true).senses_click());
    }
}
