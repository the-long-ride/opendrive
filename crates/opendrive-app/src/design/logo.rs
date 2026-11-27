//! Custom OpenDrive Concept-C hexagonal parcel vault mark for titlebar, window icon, and tray.

use crate::design::theme::ThemePalette;
use eframe::egui::{self, Color32, IconData, Pos2, Rect, Shape, Stroke};

const INK: [u8; 4] = [0x1c, 0x1e, 0x24, 0xff];
pub(crate) const PINK: [u8; 4] = [0xff, 0x3d, 0x7f, 0xff];
pub(crate) const CYAN: [u8; 4] = [0x00, 0xd4, 0xff, 0xff];
pub(crate) const INDIGO: [u8; 4] = [0x5b, 0x52, 0xf6, 0xff];
const CARD: [u8; 4] = [0xf8, 0xfa, 0xfc, 0xff];

const TOP: [f32; 2] = [0.50, 0.06];
const TR: [f32; 2] = [0.88, 0.28];
const BR: [f32; 2] = [0.88, 0.72];
const BOT: [f32; 2] = [0.50, 0.94];
const BL: [f32; 2] = [0.12, 0.72];
const TL: [f32; 2] = [0.12, 0.28];
const CTR: [f32; 2] = [0.50, 0.50];

const SEAL: [[f32; 2]; 4] = [[0.50, 0.19], [0.66, 0.28], [0.50, 0.37], [0.34, 0.28]];
const LATCH: [[f32; 2]; 4] = [[0.45, 0.45], [0.55, 0.45], [0.55, 0.62], [0.45, 0.62]];

fn point_in_poly(px: f32, py: f32, poly: &[[f32; 2]]) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = (poly[i][0], poly[i][1]);
        let (xj, yj) = (poly[j][0], poly[j][1]);
        let hit = ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi);
        if hit {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn dist_to_seg(px: f32, py: f32, a: [f32; 2], b: [f32; 2]) -> f32 {
    let (vx, vy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = vx * vx + vy * vy;
    let t = if len2 > 0.0 {
        (((px - a[0]) * vx + (py - a[1]) * vy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (px - (a[0] + t * vx)).hypot(py - (a[1] + t * vy))
}

fn near_edges(px: f32, py: f32, edges: &[([f32; 2], [f32; 2])], half_w: f32) -> bool {
    edges
        .iter()
        .any(|&(a, b)| dist_to_seg(px, py, a, b) <= half_w)
}

pub fn render_logo_rgba(size: u32) -> Vec<u8> {
    let n = size.max(16) as usize;
    let sz = n as f32;
    let mut buf = vec![0u8; n * n * 4];
    let top_face = [TOP, TR, CTR, TL];
    let left_face = [TL, CTR, BOT, BL];
    let right_face = [CTR, TR, BR, BOT];
    let edges = [
        (TOP, TR),
        (TR, BR),
        (BR, BOT),
        (BOT, BL),
        (BL, TL),
        (TL, TOP),
        (TL, CTR),
        (TR, CTR),
        (CTR, BOT),
    ];
    let border = 0.036;
    for py_i in 0..n {
        for px_i in 0..n {
            let (px, py) = ((px_i as f32 + 0.5) / sz, (py_i as f32 + 0.5) / sz);
            let color = if near_edges(px, py, &edges, border) {
                Some(INK)
            } else if point_in_poly(px, py, &LATCH) || point_in_poly(px, py, &SEAL) {
                Some(INDIGO)
            } else if point_in_poly(px, py, &top_face) {
                Some(CARD)
            } else if point_in_poly(px, py, &left_face) {
                Some(PINK)
            } else if point_in_poly(px, py, &right_face) {
                Some(CYAN)
            } else {
                None
            };
            if let Some(c) = color {
                let idx = (py_i * n + px_i) * 4;
                buf[idx..idx + 4].copy_from_slice(&c);
            }
        }
    }
    buf
}

pub fn logo_icon_data(size: u32) -> IconData {
    let side = size.max(16);
    IconData {
        rgba: render_logo_rgba(side),
        width: side,
        height: side,
    }
}

pub fn encode_logo_png(size: u32) -> Option<Vec<u8>> {
    use image::{ImageFormat, RgbaImage};
    use std::io::Cursor;

    let side = size.max(16);
    let img = RgbaImage::from_raw(side, side, render_logo_rgba(side))?;
    let mut png = Vec::new();
    img.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .ok()?;
    Some(png)
}

pub fn encode_multi_size_ico(sizes: &[u32]) -> Option<Vec<u8>> {
    let count = u16::try_from(sizes.len()).ok()?;
    let mut payloads = Vec::with_capacity(sizes.len());
    for &size in sizes {
        let side = size.clamp(16, 256);
        payloads.push((side, encode_logo_png(side)?));
    }
    let header_len = 6usize + payloads.len() * 16;
    let total_png: usize = payloads.iter().map(|(_, p)| p.len()).sum();
    let mut ico = Vec::with_capacity(header_len + total_png);
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&count.to_le_bytes());

    let mut offset = header_len;
    for (side, png) in &payloads {
        let dim = if *side >= 256 { 0u8 } else { *side as u8 };
        let size_u32 = u32::try_from(png.len()).ok()?;
        let off_u32 = u32::try_from(offset).ok()?;
        ico.extend_from_slice(&[dim, dim, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&size_u32.to_le_bytes());
        ico.extend_from_slice(&off_u32.to_le_bytes());
        offset += png.len();
    }
    for (_, png) in &payloads {
        ico.extend_from_slice(png);
    }
    Some(ico)
}

pub fn ensure_logo_ico(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join("opendrive.ico");
    let ico = encode_multi_size_ico(&[64])?;
    std::fs::write(&path, ico).ok()?;
    Some(path)
}

pub fn paint_logo(painter: &egui::Painter, rect: Rect, pal: &ThemePalette) {
    let ink = pal.tactile_ink;
    let sw = (rect.width() * 0.065).clamp(1.3, 2.2);
    let to_pos = |pt: [f32; 2]| {
        Pos2::new(
            rect.left() + pt[0] * rect.width(),
            rect.top() + pt[1] * rect.height(),
        )
    };
    let poly = |pts: &[[f32; 2]], fill: Color32| {
        painter.add(Shape::convex_polygon(
            pts.iter().map(|&p| to_pos(p)).collect(),
            fill,
            Stroke::new(sw, ink),
        ));
    };
    poly(&[TOP, TR, CTR, TL], Color32::from_rgb(0xf8, 0xfa, 0xfc));
    poly(&[TL, CTR, BOT, BL], pal.pop_pink);
    poly(&[CTR, TR, BR, BOT], pal.pop_cyan);
    poly(&SEAL, pal.accent_blue);
    poly(&LATCH, pal.accent_blue);
}
