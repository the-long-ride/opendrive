//! Extended 1.5px stroke geometric vector icons for window controls, drive explorer, cloud states, and media types.

use crate::design::icons::{map_pt, seg, stroke_poly, IconKind};
use eframe::egui::{self, Rect, Stroke};

fn stroke_box(p: &egui::Painter, r: Rect, x0: f32, y0: f32, x1: f32, y1: f32, s: Stroke) {
    stroke_poly(p, r, &[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], true, s);
}

fn cloud_outline(p: &egui::Painter, r: Rect, closed: bool, s: Stroke) {
    let pts = [
        (6.0, 13.0),
        (3.5, 13.0),
        (2.5, 10.5),
        (4.0, 8.0),
        (6.5, 7.5),
        (8.0, 4.5),
        (12.0, 4.5),
        (14.0, 7.0),
        (16.5, 7.5),
        (17.5, 10.5),
        (16.5, 13.0),
        (14.0, 13.0),
    ];
    stroke_poly(p, r, &pts, closed, s);
}

fn folded_sheet(p: &egui::Painter, r: Rect, s: Stroke) {
    let sheet = [
        (4.5, 2.5),
        (12.5, 2.5),
        (16.0, 6.0),
        (16.0, 17.5),
        (4.5, 17.5),
    ];
    stroke_poly(p, r, &sheet, true, s);
    stroke_poly(p, r, &[(12.5, 2.5), (12.5, 6.0), (16.0, 6.0)], false, s);
}

pub(crate) fn paint_media_icon(painter: &egui::Painter, rect: Rect, icon: IconKind, s: Stroke) {
    match icon {
        IconKind::Copy => {
            stroke_poly(
                painter,
                rect,
                &[(7., 3.5), (16.5, 3.5), (16.5, 13.5)],
                false,
                s,
            );
            stroke_box(painter, rect, 3.5, 6.5, 13.5, 16.5, s);
        }
        IconKind::External => {
            let b = [(10., 4.5), (4., 4.5), (4., 16.), (15.5, 16.), (15.5, 10.)];
            stroke_poly(painter, rect, &b, false, s);
            stroke_poly(
                painter,
                rect,
                &[(11.5, 3.5), (16.5, 3.5), (16.5, 8.5)],
                false,
                s,
            );
            seg(painter, rect, (9.5, 10.5), (16.5, 3.5), s);
        }
        IconKind::Key => {
            painter.circle_stroke(map_pt(rect, 7.0, 10.0), rect.width() * 0.16, s);
            seg(painter, rect, (10.2, 10.0), (17.0, 10.0), s);
            seg(painter, rect, (14.2, 10.0), (14.2, 13.0), s);
            seg(painter, rect, (16.5, 10.0), (16.5, 12.5), s);
        }
        IconKind::Minimize => seg(painter, rect, (4.5, 14.5), (15.5, 14.5), s),
        IconKind::Maximize => stroke_box(painter, rect, 4.5, 4.5, 15.5, 15.5, s),
        IconKind::Restore => {
            stroke_poly(
                painter,
                rect,
                &[(7.0, 4.0), (16.0, 4.0), (16.0, 13.0)],
                false,
                s,
            );
            stroke_box(painter, rect, 4.0, 7.0, 13.0, 16.0, s);
        }
        IconKind::Edit => {
            let pts = [
                (4.0, 16.0),
                (4.5, 12.5),
                (13.5, 3.5),
                (16.5, 6.5),
                (7.5, 15.5),
            ];
            stroke_poly(painter, rect, &pts, true, s);
            seg(painter, rect, (11.5, 5.5), (14.5, 8.5), s);
        }
        IconKind::Heart => {
            let pts = [
                (10.0, 16.5),
                (3.5, 10.5),
                (3.5, 6.5),
                (6.5, 4.0),
                (10.0, 6.5),
                (13.5, 4.0),
                (16.5, 6.5),
                (16.5, 10.5),
            ];
            stroke_poly(painter, rect, &pts, true, s);
        }
        IconKind::Star => {
            let pts = [
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
            ];
            stroke_poly(painter, rect, &pts, true, s);
        }
        IconKind::Explore => {
            painter.circle_stroke(map_pt(rect, 10.0, 10.0), rect.width() * 0.35, s);
            let needle = [(13.2, 6.8), (11.2, 11.2), (6.8, 13.2), (8.8, 8.8)];
            stroke_poly(painter, rect, &needle, true, s);
        }
        IconKind::ArrowLeft => {
            seg(painter, rect, (16.0, 10.0), (4.5, 10.0), s);
            stroke_poly(
                painter,
                rect,
                &[(9.5, 5.0), (4.5, 10.0), (9.5, 15.0)],
                false,
                s,
            );
        }
        IconKind::ChevronRight => stroke_poly(
            painter,
            rect,
            &[(7.5, 4.5), (13.0, 10.0), (7.5, 15.5)],
            false,
            s,
        ),
        IconKind::ChevronUp => stroke_poly(
            painter,
            rect,
            &[(4.5, 12.5), (10.0, 7.0), (15.5, 12.5)],
            false,
            s,
        ),
        IconKind::ChevronDown => stroke_poly(
            painter,
            rect,
            &[(4.5, 7.5), (10.0, 13.0), (15.5, 7.5)],
            false,
            s,
        ),
        IconKind::Search => {
            painter.circle_stroke(map_pt(rect, 8.5, 8.5), rect.width() * 0.24, s);
            seg(painter, rect, (12.0, 12.0), (16.5, 16.5), s);
        }
        IconKind::Grid => {
            stroke_box(painter, rect, 3.5, 3.5, 8.5, 8.5, s);
            stroke_box(painter, rect, 11.5, 3.5, 16.5, 8.5, s);
            stroke_box(painter, rect, 3.5, 11.5, 8.5, 16.5, s);
            stroke_box(painter, rect, 11.5, 11.5, 16.5, 16.5, s);
        }
        IconKind::List => {
            for y in [5.5, 10.0, 14.5] {
                seg(painter, rect, (3.5, y), (4.8, y), s);
                seg(painter, rect, (7.5, y), (16.5, y), s);
            }
        }
        IconKind::Cloud => {
            let pts = [
                (6.0, 14.5),
                (3.5, 14.5),
                (2.5, 11.8),
                (4.0, 9.2),
                (6.5, 8.7),
                (8.0, 5.7),
                (12.0, 5.7),
                (14.0, 8.2),
                (16.5, 8.7),
                (17.5, 11.8),
                (16.5, 14.5),
                (14.0, 14.5),
            ];
            stroke_poly(painter, rect, &pts, true, s);
        }
        IconKind::DownloadCheck => {
            seg(painter, rect, (7.2, 3.5), (7.2, 12.0), s);
            stroke_poly(
                painter,
                rect,
                &[(4.6, 9.4), (7.2, 12.0), (9.8, 9.4)],
                false,
                s,
            );
            stroke_poly(
                painter,
                rect,
                &[(11.4, 9.8), (13.5, 12.0), (17.2, 7.2)],
                false,
                s,
            );
            stroke_poly(
                painter,
                rect,
                &[(3.0, 13.5), (3.0, 16.5), (17.0, 16.5), (17.0, 13.5)],
                false,
                s,
            );
        }
        IconKind::MoveRight => {
            seg(painter, rect, (3.5, 10.0), (16.0, 10.0), s);
            stroke_poly(
                painter,
                rect,
                &[(11.2, 5.2), (16.0, 10.0), (11.2, 14.8)],
                false,
                s,
            );
        }
        IconKind::CloudDownload => {
            cloud_outline(painter, rect, false, s);
            seg(painter, rect, (10.0, 9.0), (10.0, 16.5), s);
            stroke_poly(
                painter,
                rect,
                &[(7.2, 13.8), (10.0, 16.5), (12.8, 13.8)],
                false,
                s,
            );
        }
        IconKind::CloudUpload => {
            cloud_outline(painter, rect, false, s);
            seg(painter, rect, (10.0, 16.5), (10.0, 9.0), s);
            stroke_poly(
                painter,
                rect,
                &[(7.2, 11.7), (10.0, 9.0), (12.8, 11.7)],
                false,
                s,
            );
        }
        IconKind::CloudClear => {
            cloud_outline(painter, rect, true, s);
            seg(painter, rect, (7.5, 9.8), (12.5, 9.8), s);
            seg(painter, rect, (7.0, 16.0), (13.0, 16.0), s);
        }
        IconKind::Image => {
            stroke_box(painter, rect, 3.0, 4.0, 17.0, 16.0, s);
            painter.circle_stroke(map_pt(rect, 7.0, 8.0), rect.width() * 0.08, s);
            let mtn = [
                (3.5, 15.5),
                (8.0, 10.5),
                (11.0, 13.5),
                (13.5, 9.5),
                (16.5, 15.5),
            ];
            stroke_poly(painter, rect, &mtn, false, s);
        }
        IconKind::Video => {
            stroke_box(painter, rect, 2.5, 4.5, 17.5, 15.5, s);
            stroke_poly(
                painter,
                rect,
                &[(8.2, 7.2), (13.5, 10.0), (8.2, 12.8)],
                true,
                s,
            );
        }
        IconKind::Pdf => {
            folded_sheet(painter, rect, s);
            stroke_box(painter, rect, 6.5, 10.0, 13.5, 14.0, s);
            seg(painter, rect, (8.0, 12.0), (12.0, 12.0), s);
        }
        IconKind::Audio => {
            painter.circle_stroke(map_pt(rect, 6.2, 14.5), rect.width() * 0.1, s);
            painter.circle_stroke(map_pt(rect, 13.8, 13.0), rect.width() * 0.1, s);
            let stem = [(8.2, 14.5), (8.2, 5.0), (15.8, 3.5), (15.8, 13.0)];
            stroke_poly(painter, rect, &stem, false, s);
        }
        IconKind::FileText => {
            stroke_box(painter, rect, 4.5, 2.5, 15.5, 17.5, s);
            seg(painter, rect, (7.0, 7.0), (13.0, 7.0), s);
            seg(painter, rect, (7.0, 10.5), (13.0, 10.5), s);
            seg(painter, rect, (7.0, 14.0), (11.0, 14.0), s);
        }
        IconKind::DocSheet => {
            folded_sheet(painter, rect, s);
            seg(painter, rect, (7.0, 9.5), (13.5, 9.5), s);
            seg(painter, rect, (7.0, 12.5), (13.5, 12.5), s);
            seg(painter, rect, (7.0, 15.0), (11.0, 15.0), s);
        }
        _ => {}
    }
}
