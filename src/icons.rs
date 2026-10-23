use eframe::egui::{
    self, Color32, Painter, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
};

#[derive(Clone, Copy)]
pub enum Icon {
    Cross,
    Book,
    Sunrise,
    Sun,
    Sunset,
    Moon,
    Left,
    Right,
    Search,
    Sidebar,
    Settings,
    Bookmark,
    Download,
    Focus,
    Check,
    Close,
    Calendar,
    Note,
}

pub fn button(ui: &mut Ui, icon: Icon, label: &str, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    if response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(rect, 5.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    draw(ui.painter(), rect.shrink(6.0), icon, color);
    response.on_hover_text(label)
}

pub fn draw(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let at = |x: f32, y: f32| {
        Pos2::new(
            rect.left() + x * rect.width(),
            rect.top() + y * rect.height(),
        )
    };
    let stroke = Stroke::new(1.35_f32, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([at(a.0, a.1), at(b.0, b.1)], stroke);
    };
    match icon {
        Icon::Cross => {
            line((0.5, 0.08), (0.5, 0.92));
            line((0.23, 0.36), (0.77, 0.36));
        }
        Icon::Left => {
            line((0.65, 0.2), (0.35, 0.5));
            line((0.35, 0.5), (0.65, 0.8));
        }
        Icon::Right => {
            line((0.35, 0.2), (0.65, 0.5));
            line((0.65, 0.5), (0.35, 0.8));
        }
        Icon::Close => {
            line((0.25, 0.25), (0.75, 0.75));
            line((0.25, 0.75), (0.75, 0.25));
        }
        Icon::Check => {
            line((0.14, 0.52), (0.4, 0.77));
            line((0.4, 0.77), (0.87, 0.23));
        }
        Icon::Search => {
            p.circle_stroke(at(0.43, 0.42), rect.width() * 0.28, stroke);
            line((0.64, 0.64), (0.89, 0.89));
        }
        Icon::Sun => {
            p.circle_stroke(at(0.5, 0.5), rect.width() * 0.23, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                line(
                    (0.5 + 0.33 * a.cos(), 0.5 + 0.33 * a.sin()),
                    (0.5 + 0.45 * a.cos(), 0.5 + 0.45 * a.sin()),
                );
            }
        }
        Icon::Sunrise | Icon::Sunset => {
            let arc = (0..=20)
                .map(|i| {
                    let a = std::f32::consts::PI + i as f32 * std::f32::consts::PI / 20.0;
                    at(0.5 + 0.25 * a.cos(), 0.65 + 0.25 * a.sin())
                })
                .collect::<Vec<_>>();
            p.add(egui::Shape::line(arc, stroke));
            for i in 0..5 {
                let a = std::f32::consts::PI + i as f32 * std::f32::consts::PI / 4.0;
                line(
                    (0.5 + 0.34 * a.cos(), 0.65 + 0.34 * a.sin()),
                    (0.5 + 0.45 * a.cos(), 0.65 + 0.45 * a.sin()),
                );
            }
            line((0.04, 0.68), (0.96, 0.68));
            line((0.23, 0.86), (0.77, 0.86));
        }
        Icon::Moon => {
            let points = (0..=24)
                .map(|i| {
                    let a = 0.5 + i as f32 * 5.3 / 24.0;
                    at(0.5 + 0.39 * a.cos(), 0.5 + 0.39 * a.sin())
                })
                .collect::<Vec<_>>();
            p.add(egui::Shape::line(points, stroke));
            let points = (0..=18)
                .map(|i| {
                    let a = 1.0 + i as f32 * 4.15 / 18.0;
                    at(0.76 + 0.35 * a.cos(), 0.38 + 0.35 * a.sin())
                })
                .collect::<Vec<_>>();
            p.add(egui::Shape::line(points, stroke));
        }
        Icon::Book => {
            for side in [-1.0, 1.0] {
                p.add(egui::Shape::line(
                    vec![
                        at(0.5, 0.24),
                        at(0.5 + 0.36 * side, 0.15),
                        at(0.5 + 0.36 * side, 0.78),
                        at(0.5, 0.88),
                    ],
                    stroke,
                ));
            }
            line((0.5, 0.24), (0.5, 0.88));
        }
        Icon::Sidebar => {
            p.rect_stroke(
                Rect::from_min_max(at(0.08, 0.18), at(0.92, 0.82)),
                2.0,
                stroke,
                StrokeKind::Inside,
            );
            line((0.39, 0.18), (0.39, 0.82));
        }
        Icon::Bookmark => {
            p.add(egui::Shape::closed_line(
                vec![
                    at(0.25, 0.12),
                    at(0.75, 0.12),
                    at(0.75, 0.9),
                    at(0.5, 0.7),
                    at(0.25, 0.9),
                ],
                stroke,
            ));
        }
        Icon::Download => {
            line((0.5, 0.08), (0.5, 0.64));
            line((0.27, 0.43), (0.5, 0.66));
            line((0.5, 0.66), (0.73, 0.43));
            line((0.15, 0.68), (0.15, 0.9));
            line((0.15, 0.9), (0.85, 0.9));
            line((0.85, 0.9), (0.85, 0.68));
        }
        Icon::Focus => {
            for (x, y, dx, dy) in [
                (0.15, 0.15, 1.0, 1.0),
                (0.85, 0.15, -1.0, 1.0),
                (0.15, 0.85, 1.0, -1.0),
                (0.85, 0.85, -1.0, -1.0),
            ] {
                line((x, y), (x + dx * 0.22, y));
                line((x, y), (x, y + dy * 0.22));
            }
        }
        Icon::Settings => {
            for (x, y) in [(0.22, 0.34), (0.5, 0.67), (0.78, 0.4)] {
                line((x, 0.12), (x, 0.88));
                p.circle_filled(at(x, y), 3.2, color);
            }
        }
        Icon::Calendar => {
            p.rect_stroke(
                Rect::from_min_max(at(0.1, 0.2), at(0.9, 0.9)),
                2.0,
                stroke,
                StrokeKind::Inside,
            );
            line((0.1, 0.42), (0.9, 0.42));
            line((0.3, 0.08), (0.3, 0.3));
            line((0.7, 0.08), (0.7, 0.3));
        }
        Icon::Note => {
            p.rect_stroke(
                Rect::from_min_max(at(0.18, 0.1), at(0.82, 0.9)),
                2.0,
                stroke,
                StrokeKind::Inside,
            );
            for y in [0.35, 0.52, 0.69] {
                line((0.32, y), (0.68, y));
            }
        }
    }
}
