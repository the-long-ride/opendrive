//! Geometric vector iconography painted onto egui surfaces.

use crate::design::logo::{CYAN, INDIGO, PINK};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};

const THEME_DOTS: [(f32, f32); 3] = [(10.0, 5.8), (5.6, 13.5), (14.4, 13.5)];

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
    ThemeColor,
    ThemeMono,
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
    Explore,
    ArrowLeft,
    ChevronRight,
    ChevronUp,
    ChevronDown,
    Search,
    Grid,
    List,
    Cloud,
    CloudDownload,
    CloudUpload,
    CloudClear,
    DownloadCheck,
    MoveRight,
    Image,
    Video,
    Pdf,
    Audio,
    FileText,
    DocSheet,
    Monitor,
    Link,
}

pub(crate) fn map_pt(rect: Rect, x: f32, y: f32) -> Pos2 {
    Pos2::new(
        rect.left() + (x / 20.0) * rect.width(),
        rect.top() + (y / 20.0) * rect.height(),
    )
}

pub(crate) fn seg(p: &egui::Painter, r: Rect, a: (f32, f32), b: (f32, f32), s: Stroke) {
    p.line_segment([map_pt(r, a.0, a.1), map_pt(r, b.0, b.1)], s);
}

pub(crate) fn stroke_poly(
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
        IconKind::Folder => {
            let pts = [
                (2.5, 5.),
                (8., 5.),
                (10., 7.),
                (17.5, 7.),
                (17.5, 16.),
                (2.5, 16.),
            ];
            stroke_poly(painter, rect, &pts, true, s);
        }
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
            seg(painter, rect, (6.8, 4.5), (6.8, 15.5), bar);
            seg(painter, rect, (13.2, 4.5), (13.2, 15.5), bar);
        }
        IconKind::Play => stroke_poly(painter, rect, &[(6., 4.5), (16., 10.), (6., 15.5)], true, s),
        IconKind::Lock | IconKind::Unlock => {
            let shackle: &[(f32, f32)] = if icon == IconKind::Lock {
                &[(6.5, 9.), (6.5, 6.), (10., 4.2), (13.5, 6.), (13.5, 9.)]
            } else {
                &[(6.5, 9.), (6.5, 6.), (10., 4.2), (13.5, 5.8)]
            };
            stroke_poly(painter, rect, shackle, false, s);
            stroke_poly(
                painter,
                rect,
                &[(4.5, 9.), (15.5, 9.), (15.5, 16.5), (4.5, 16.5)],
                true,
                s,
            );
        }
        IconKind::Trash => {
            seg(painter, rect, (4., 6.), (16., 6.), s);
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
            seg(painter, rect, (4., 15.5), (15.5, 4.), s);
            seg(painter, rect, (4.5, 5.5), (7.5, 5.5), s);
            seg(painter, rect, (6., 4.), (6., 7.), s);
            seg(painter, rect, (12.5, 15.5), (15.5, 15.5), s);
            seg(painter, rect, (14., 14.), (14., 17.), s);
        }
        IconKind::Check => stroke_poly(
            painter,
            rect,
            &[(4., 10.5), (8.5, 15.), (16.5, 5.5)],
            false,
            s,
        ),
        IconKind::Alert => {
            stroke_poly(painter, rect, &[(10., 3.5), (17., 16.), (3., 16.)], true, s);
            seg(painter, rect, (10., 8.5), (10., 12.), s);
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
        IconKind::ThemeColor => {
            let radius = rect.width() * 0.165;
            let rim = Stroke::new(1.25, color);
            let alpha = color.a();
            let fills = [INDIGO, PINK, CYAN]
                .map(|[r, g, b, _]| Color32::from_rgba_unmultiplied(r, g, b, alpha));
            for (pt, fill) in THEME_DOTS.into_iter().zip(fills) {
                let center = map_pt(rect, pt.0, pt.1);
                painter.circle_filled(center, radius, fill);
                painter.circle_stroke(center, radius, rim);
            }
        }
        IconKind::ThemeMono => {
            let radius = rect.width() * 0.175;
            for pt in THEME_DOTS {
                painter.circle_filled(map_pt(rect, pt.0, pt.1), radius, color);
            }
        }
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
        }
        IconKind::Monitor => {
            stroke_poly(
                painter,
                rect,
                &[(3.0, 4.0), (17.0, 4.0), (17.0, 13.5), (3.0, 13.5)],
                true,
                s,
            );
            seg(painter, rect, (10.0, 13.5), (10.0, 16.5), s);
            seg(painter, rect, (6.5, 16.5), (13.5, 16.5), s);
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
        }
        other => crate::design::icons_media::paint_media_icon(painter, rect, other, s),
    }
}
