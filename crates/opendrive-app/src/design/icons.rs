//! Minimalist 1.5px stroke geometric vector iconography painted onto egui surfaces.

use eframe::egui::{self, Color32, Pos2, Rect, Stroke};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    Brand,
    Folder,
    Plus,
    Close,
    Pause,
    Play,
    Lock,
    Unlock,
    Trash,
    Purge,
    Check,
    Alert,
    Refresh,
    Sun,
    Moon,
    Copy,
    External,
    Key,
    Settings,
    Minimize,
    Maximize,
    Restore,
    Edit,
    Heart,
    Star,
}

fn map_pt(rect: Rect, x: f32, y: f32) -> Pos2 {
    Pos2::new(
        rect.left() + (x / 20.0) * rect.width(),
        rect.top() + (y / 20.0) * rect.height(),
    )
}

fn seg(p: &egui::Painter, r: Rect, a: (f32, f32), b: (f32, f32), s: Stroke) {
    p.line_segment([map_pt(r, a.0, a.1), map_pt(r, b.0, b.1)], s);
}

fn stroke_poly(
    painter: &egui::Painter,
    rect: Rect,
    pts: &[(f32, f32)],
    closed: bool,
    stroke: Stroke,
) {
    for win in pts.windows(2) {
        seg(painter, rect, win[0], win[1], stroke);
    }
    if closed && pts.len() >= 2 {
        seg(painter, rect, pts[pts.len() - 1], pts[0], stroke);
    }
}

pub fn paint_icon(painter: &egui::Painter, rect: Rect, icon: IconKind, color: Color32) {
    let s = Stroke::new(1.5, color);
    match icon {
        IconKind::Brand => {
            let hex = [
                (10., 2.5),
                (17., 6.5),
                (17., 13.5),
                (10., 17.5),
                (3., 13.5),
                (3., 6.5),
            ];
            stroke_poly(painter, rect, &hex, true, s);
            for pt in [(10., 17.5), (3., 6.5), (17., 6.5)] {
                seg(painter, rect, (10., 10.), pt, s);
            }
        }
        IconKind::Folder => stroke_poly(
            painter,
            rect,
            &[
                (2.5, 5.),
                (8., 5.),
                (10., 7.),
                (17.5, 7.),
                (17.5, 16.),
                (2.5, 16.),
            ],
            true,
            s,
        ),
        IconKind::Plus => {
            seg(painter, rect, (10., 4.5), (10., 15.5), s);
            seg(painter, rect, (4.5, 10.), (15.5, 10.), s);
        }
        IconKind::Close => {
            seg(painter, rect, (5., 5.), (15., 15.), s);
            seg(painter, rect, (15., 5.), (5., 15.), s);
        }
        IconKind::Pause => {
            let bar = Stroke::new(2.0, color);
            painter.line_segment([map_pt(rect, 6.8, 4.5), map_pt(rect, 6.8, 15.5)], bar);
            painter.line_segment([map_pt(rect, 13.2, 4.5), map_pt(rect, 13.2, 15.5)], bar);
        }
        IconKind::Play => stroke_poly(painter, rect, &[(6., 4.5), (16., 10.), (6., 15.5)], true, s),
        IconKind::Lock | IconKind::Unlock => {
            let shackle: &[(f32, f32)] = if icon == IconKind::Lock {
                &[(6.5, 9.), (6.5, 6.), (10., 4.2), (13.5, 6.), (13.5, 9.)]
            } else {
                &[(6.5, 9.), (6.5, 6.), (10., 4.2), (13.5, 5.8)]
            };
            stroke_poly(painter, rect, shackle, false, s);
            let body = [(4.5, 9.), (15.5, 9.), (15.5, 16.5), (4.5, 16.5)];
            stroke_poly(painter, rect, &body, true, s);
        }
        IconKind::Trash => {
            painter.line_segment([map_pt(rect, 4., 6.), map_pt(rect, 16., 6.)], s);
            stroke_poly(
                painter,
                rect,
                &[(8., 6.), (8., 4.), (12., 4.), (12., 6.)],
                false,
                s,
            );
            stroke_poly(
                painter,
                rect,
                &[(5.5, 6.), (6.5, 16.5), (13.5, 16.5), (14.5, 6.)],
                false,
                s,
            );
        }
        IconKind::Purge => {
            painter.line_segment([map_pt(rect, 4., 15.5), map_pt(rect, 15.5, 4.)], s);
            painter.line_segment([map_pt(rect, 4.5, 5.5), map_pt(rect, 7.5, 5.5)], s);
            painter.line_segment([map_pt(rect, 6., 4.), map_pt(rect, 6., 7.)], s);
            painter.line_segment([map_pt(rect, 12.5, 15.5), map_pt(rect, 15.5, 15.5)], s);
            painter.line_segment([map_pt(rect, 14., 14.), map_pt(rect, 14., 17.)], s);
        }
        IconKind::Check => {
            stroke_poly(
                painter,
                rect,
                &[(4., 10.5), (8.5, 15.), (16.5, 5.5)],
                false,
                s,
            );
        }
        IconKind::Alert => {
            stroke_poly(painter, rect, &[(10., 3.5), (17., 16.), (3., 16.)], true, s);
            painter.line_segment([map_pt(rect, 10., 8.5), map_pt(rect, 10., 12.)], s);
            painter.circle_filled(map_pt(rect, 10., 14.), 1.0, color);
        }
        IconKind::Refresh => {
            stroke_poly(painter, rect, &[(4.5, 9.), (4.5, 5.), (14.5, 5.)], false, s);
            stroke_poly(
                painter,
                rect,
                &[(12., 2.8), (15., 5.), (12., 7.2)],
                false,
                s,
            );
            stroke_poly(
                painter,
                rect,
                &[(15.5, 11.), (15.5, 15.), (5.5, 15.)],
                false,
                s,
            );
            stroke_poly(
                painter,
                rect,
                &[(8., 12.8), (5., 15.), (8., 17.2)],
                false,
                s,
            );
        }
        IconKind::Sun => {
            painter.circle_stroke(map_pt(rect, 10., 10.), rect.width() * 0.175, s);
            for step in 0..8 {
                let a = (step as f32) * std::f32::consts::FRAC_PI_4;
                let (sin, cos) = a.sin_cos();
                let p1 = map_pt(rect, 10. + cos * 5.4, 10. + sin * 5.4);
                let p2 = map_pt(rect, 10. + cos * 7.6, 10. + sin * 7.6);
                painter.line_segment([p1, p2], s);
            }
        }
        IconKind::Moon => {
            let cres = [
                (12.2, 3.6),
                (7.8, 5.2),
                (5.2, 9.6),
                (6.8, 14.4),
                (11.6, 16.4),
                (16.0, 14.4),
                (11.8, 13.6),
                (9.8, 10.0),
                (11.6, 6.2),
            ];
            stroke_poly(painter, rect, &cres, true, s);
        }
        IconKind::Copy => {
            stroke_poly(
                painter,
                rect,
                &[(7., 3.5), (16.5, 3.5), (16.5, 13.5)],
                false,
                s,
            );
            stroke_poly(
                painter,
                rect,
                &[(3.5, 6.5), (13.5, 6.5), (13.5, 16.5), (3.5, 16.5)],
                true,
                s,
            );
        }
        IconKind::External => {
            stroke_poly(
                painter,
                rect,
                &[(10., 4.5), (4., 4.5), (4., 16.), (15.5, 16.), (15.5, 10.)],
                false,
                s,
            );
            stroke_poly(
                painter,
                rect,
                &[(11.5, 3.5), (16.5, 3.5), (16.5, 8.5)],
                false,
                s,
            );
            painter.line_segment([map_pt(rect, 9.5, 10.5), map_pt(rect, 16.5, 3.5)], s);
        }
        IconKind::Key => {
            painter.circle_stroke(map_pt(rect, 7.0, 10.0), rect.width() * 0.16, s);
            painter.line_segment([map_pt(rect, 10.2, 10.0), map_pt(rect, 17.0, 10.0)], s);
            painter.line_segment([map_pt(rect, 14.2, 10.0), map_pt(rect, 14.2, 13.0)], s);
            painter.line_segment([map_pt(rect, 16.5, 10.0), map_pt(rect, 16.5, 12.5)], s);
        }
        IconKind::Settings => {
            painter.circle_stroke(map_pt(rect, 10., 10.), rect.width() * 0.16, s);
            for step in 0..6 {
                let a = (step as f32) * std::f32::consts::FRAC_PI_3;
                let (sin, cos) = a.sin_cos();
                painter.line_segment(
                    [
                        map_pt(rect, 10. + cos * 5.0, 10. + sin * 5.0),
                        map_pt(rect, 10. + cos * 7.6, 10. + sin * 7.6),
                    ],
                    s,
                );
            }
        }
        IconKind::Minimize => {
            painter.line_segment([map_pt(rect, 4.5, 14.5), map_pt(rect, 15.5, 14.5)], s);
        }
        IconKind::Maximize => {
            stroke_poly(
                painter,
                rect,
                &[(4.5, 4.5), (15.5, 4.5), (15.5, 15.5), (4.5, 15.5)],
                true,
                s,
            );
        }
        IconKind::Restore => {
            stroke_poly(
                painter,
                rect,
                &[(7.0, 4.0), (16.0, 4.0), (16.0, 13.0)],
                false,
                s,
            );
            stroke_poly(
                painter,
                rect,
                &[(4.0, 7.0), (13.0, 7.0), (13.0, 16.0), (4.0, 16.0)],
                true,
                s,
            );
        }
        IconKind::Edit => {
            stroke_poly(
                painter,
                rect,
                &[
                    (4.0, 16.0),
                    (4.5, 12.5),
                    (13.5, 3.5),
                    (16.5, 6.5),
                    (7.5, 15.5),
                ],
                true,
                s,
            );
            painter.line_segment([map_pt(rect, 11.5, 5.5), map_pt(rect, 14.5, 8.5)], s);
        }
        IconKind::Heart => {
            stroke_poly(
                painter,
                rect,
                &[
                    (10.0, 16.5),
                    (3.5, 10.5),
                    (3.5, 6.5),
                    (6.5, 4.0),
                    (10.0, 6.5),
                    (13.5, 4.0),
                    (16.5, 6.5),
                    (16.5, 10.5),
                ],
                true,
                s,
            );
        }
        IconKind::Star => {
            stroke_poly(
                painter,
                rect,
                &[
                    (10.0, 3.0),
                    (12.1, 7.3),
                    (16.8, 8.0),
                    (13.4, 11.3),
                    (14.2, 16.0),
                    (10.0, 13.8),
                    (5.8, 16.0),
                    (6.6, 11.3),
                    (3.2, 8.0),
                    (7.9, 7.3),
                ],
                true,
                s,
            );
        }
    }
}
