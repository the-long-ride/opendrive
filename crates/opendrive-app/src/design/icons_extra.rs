//! Extra UI chrome icons (Settings gear, Monitor, Link, Terminal, HelpBook).

use crate::design::icons::{map_pt, seg, stroke_poly, IconKind};
use eframe::egui::{self, Color32, Rect, Stroke};

pub(crate) fn paint_extra_icon(
    painter: &egui::Painter,
    rect: Rect,
    icon: IconKind,
    color: Color32,
    s: Stroke,
) -> bool {
    match icon {
        IconKind::Settings => {
            let gear_stroke = Stroke::new(1.35, color);
            painter.circle_stroke(map_pt(rect, 10.0, 10.0), rect.width() * 0.135, gear_stroke);
            let mut teeth = Vec::with_capacity(30);
            for step in 0..6 {
                let tooth_angle =
                    -std::f32::consts::FRAC_PI_2 + (step as f32) * std::f32::consts::FRAC_PI_3;
                for (angle_offset, radius) in [
                    (-0.27, 5.5),
                    (-0.17, 8.1),
                    (0.17, 8.1),
                    (0.27, 5.5),
                    (std::f32::consts::FRAC_PI_6, 5.35),
                ] {
                    let (sin, cos) = (tooth_angle + angle_offset).sin_cos();
                    teeth.push(map_pt(rect, 10.0 + cos * radius, 10.0 + sin * radius));
                }
            }
            painter.add(egui::Shape::closed_line(teeth, gear_stroke));
            true
        }
        IconKind::Monitor => {
            let box_pts = [(3.0, 4.0), (17.0, 4.0), (17.0, 13.5), (3.0, 13.5)];
            stroke_poly(painter, rect, &box_pts, true, s);
            seg(painter, rect, (10.0, 13.5), (10.0, 16.5), s);
            seg(painter, rect, (6.5, 16.5), (13.5, 16.5), s);
            true
        }
        IconKind::Link => {
            let left_link = [(8.5, 7.), (5.5, 7.), (3.5, 10.), (5.5, 13.), (8.5, 13.)];
            let right_link = [
                (11.5, 7.),
                (14.5, 7.),
                (16.5, 10.),
                (14.5, 13.),
                (11.5, 13.),
            ];
            stroke_poly(painter, rect, &left_link, false, s);
            stroke_poly(painter, rect, &right_link, false, s);
            seg(painter, rect, (7.0, 10.0), (13.0, 10.0), s);
            true
        }
        IconKind::Terminal => {
            let frame = [(2.5, 4.), (17.5, 4.), (17.5, 16.), (2.5, 16.)];
            stroke_poly(painter, rect, &frame, true, s);
            stroke_poly(
                painter,
                rect,
                &[(5.5, 7.5), (8.5, 10.), (5.5, 12.5)],
                false,
                s,
            );
            seg(painter, rect, (10.5, 12.5), (14.5, 12.5), s);
            true
        }
        IconKind::HelpBook => {
            let book = [(4.5, 3.), (16., 3.), (16., 17.), (4.5, 17.)];
            stroke_poly(painter, rect, &book, true, s);
            seg(painter, rect, (7., 3.), (7., 17.), s);
            let q = [
                (9.8, 7.5),
                (11.5, 6.2),
                (13.2, 7.5),
                (11.5, 9.5),
                (11.5, 11.2),
            ];
            stroke_poly(painter, rect, &q, false, s);
            painter.circle_filled(map_pt(rect, 11.5, 13.6), 0.9, color);
            true
        }
        IconKind::FolderIndexed => {
            let pts = [
                (2.5, 5.),
                (8., 5.),
                (10., 7.),
                (17.5, 7.),
                (17.5, 16.),
                (2.5, 16.),
            ];
            stroke_poly(painter, rect, &pts, true, s);
            seg(painter, rect, (6.0, 10.2), (14.0, 10.2), s);
            seg(painter, rect, (6.0, 13.0), (12.0, 13.0), s);
            true
        }
        IconKind::DownloadPartial => {
            let half_arc: Vec<egui::Pos2> = (0..=8)
                .map(|i| {
                    let a = std::f32::consts::FRAC_PI_2 + (i as f32) * std::f32::consts::PI / 8.0;
                    map_pt(rect, 10.0 + a.cos() * 7.0, 10.0 + a.sin() * 7.0)
                })
                .collect();
            painter.add(egui::Shape::line(half_arc, s));
            seg(painter, rect, (10.0, 5.5), (10.0, 13.5), s);
            stroke_poly(
                painter,
                rect,
                &[(7.2, 10.8), (10.0, 13.5), (12.8, 10.8)],
                false,
                s,
            );
            seg(painter, rect, (14.6, 5.2), (16.1, 6.8), s);
            seg(painter, rect, (14.6, 14.8), (16.1, 13.2), s);
            true
        }
        _ => false,
    }
}
