//! Account avatar download, local file cache, and egui image decoding.

use crate::design::theme::ThemePalette;
use eframe::egui::{
    self, Color32, ColorImage, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind,
    TextureHandle, TextureOptions, Vec2,
};
use image::imageops::FilterType;
use opendrive_core::Account;
use opendrive_drive::DriveClient;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub fn sync_avatar_textures(
    ctx: &egui::Context,
    data_dir: Option<&Path>,
    accounts: &[Account],
    textures: &mut HashMap<Uuid, TextureHandle>,
    attempted: &mut HashSet<Uuid>,
) {
    let Some(dir) = data_dir else {
        return;
    };
    for acct in accounts {
        if textures.contains_key(&acct.id) || attempted.contains(&acct.id) {
            continue;
        }
        attempted.insert(acct.id);
        if let Some(img) = load_cached_avatar_bytes(dir, acct.id)
            .and_then(|bytes| decode_avatar_color_image(&bytes))
        {
            let handle =
                ctx.load_texture(format!("avatar-{}", acct.id), img, TextureOptions::LINEAR);
            textures.insert(acct.id, handle);
        }
    }
}

pub fn avatar_file_path(data_dir: &Path, account_id: Uuid) -> PathBuf {
    data_dir.join("avatars").join(format!("{account_id}.img"))
}

pub fn load_cached_avatar_bytes(data_dir: &Path, account_id: Uuid) -> Option<Vec<u8>> {
    std::fs::read(avatar_file_path(data_dir, account_id)).ok()
}

pub fn decode_avatar_color_image(bytes: &[u8]) -> Option<ColorImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let resized = img.resize(72, 72, FilterType::Triangle).into_rgba8();
    let mut canvas = image::RgbaImage::new(72, 72);
    let x = i64::from(72_u32.saturating_sub(resized.width())) / 2;
    let y = i64::from(72_u32.saturating_sub(resized.height())) / 2;
    image::imageops::overlay(&mut canvas, &resized, x, y);
    Some(ColorImage::from_rgba_unmultiplied(
        [72, 72],
        canvas.as_raw(),
    ))
}

pub async fn cache_account_avatar(
    drive: &DriveClient,
    data_dir: &Path,
    account_id: Uuid,
    photo_url: Option<&str>,
) -> bool {
    let Some(url) = photo_url.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    let Ok(bytes) = drive.download_bytes(url).await else {
        return false;
    };
    if decode_avatar_color_image(&bytes).is_none() {
        return false;
    }
    let path = avatar_file_path(data_dir, account_id);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, bytes).is_ok()
}

#[derive(Debug, Clone, PartialEq)]
pub struct TotalStorageSummary {
    pub used: u64,
    pub quota: u64,
    pub account_count: usize,
    pub badge_text: String,
    pub detail_text: String,
    pub ratio: f32,
    pub has_exact_total: bool,
}

pub fn summarize_total_storage(accounts: &[Account]) -> TotalStorageSummary {
    let used: u64 = accounts.iter().map(|a| a.storage_used).sum();
    let account_count = accounts.len();
    if account_count == 0 {
        return TotalStorageSummary {
            used: 0,
            quota: 0,
            account_count: 0,
            badge_text: "0 B used".into(),
            detail_text: "No accounts connected".into(),
            ratio: 0.0,
            has_exact_total: false,
        };
    }
    let has_exact_total = !accounts.is_empty()
        && accounts
            .iter()
            .all(|a| a.storage_quota > 0 && a.storage_quota >= a.storage_used);
    let quota: u64 = if has_exact_total {
        accounts.iter().map(|a| a.storage_quota).sum()
    } else {
        0
    };
    let ratio = if has_exact_total && quota > 0 {
        (used as f32 / quota as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let unit = if account_count == 1 {
        "account"
    } else {
        "accounts"
    };
    let u = crate::design::formatters::format_bytes(used);
    let (badge_text, detail_text) = if has_exact_total {
        let q = crate::design::formatters::format_bytes(quota);
        (
            format!("{u} / {q}"),
            format!("{u} of {q} across {account_count} {unit}"),
        )
    } else {
        (
            format!("{u} used"),
            format!("{u} across {account_count} {unit}"),
        )
    };
    TotalStorageSummary {
        used,
        quota,
        account_count,
        badge_text,
        detail_text,
        ratio,
        has_exact_total,
    }
}

pub fn account_avatar(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    texture: Option<&egui::TextureHandle>,
    monogram: &str,
    ring_color: Color32,
    size: f32,
) {
    let small = size <= 32.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = ui.painter();
    let (rad, shrink, irad, font_sz) = if small {
        (7, 2.5, 5, 11.0)
    } else {
        (9, 3.0, 7, 13.5)
    };
    p.rect(
        rect,
        CornerRadius::same(rad),
        ring_color,
        Stroke::new(2.0, pal.tactile_ink),
        StrokeKind::Inside,
    );
    let inner = rect.shrink(shrink);
    if let Some(tex) = texture {
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        let shape =
            egui::epaint::RectShape::filled(inner, CornerRadius::same(irad), Color32::WHITE)
                .with_texture(tex.id(), uv);
        p.add(egui::Shape::Rect(shape));
        p.rect_stroke(
            inner,
            CornerRadius::same(irad),
            Stroke::new(1.0, pal.tactile_ink),
            StrokeKind::Inside,
        );
    } else {
        p.rect_filled(inner, CornerRadius::same(irad), pal.card_bg);
        let g = p.layout_no_wrap(
            monogram.to_owned(),
            FontId::proportional(font_sz),
            pal.text_primary,
        );
        p.galley(inner.center() - g.size() * 0.5, g, pal.text_primary);
    }
}
