//! Dynamic UI font loading with Unicode Latin & CJK fallback preservation.

use eframe::egui;
use std::sync::{Arc, OnceLock};

fn unicode_fallback_fonts() -> &'static Vec<(&'static str, Arc<egui::FontData>)> {
    static FONTS: OnceLock<Vec<(&'static str, Arc<egui::FontData>)>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut out = Vec::new();
        let groups = [
            (
                "od_lat",
                "C:\\Windows\\Fonts\\segoeui.ttf|/System/Library/Fonts/Supplemental/Arial Unicode.ttf|/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            ),
            (
                "od_cjk",
                "C:\\Windows\\Fonts\\msyh.ttc|C:\\Windows\\Fonts\\malgun.ttf|/System/Library/Fonts/PingFang.ttc|/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            ),
        ];
        for (k, ps) in groups {
            if let Some(b) = ps.split('|').find_map(|p| std::fs::read(p).ok()) {
                out.push((k, Arc::new(egui::FontData::from_owned(b))));
            }
        }
        out
    })
}

pub fn apply_ui_font(
    ctx: &egui::Context,
    active_family: Option<&str>,
    custom_bytes: Option<Vec<u8>>,
) {
    let applied_key = active_family
        .filter(|_| custom_bytes.is_some())
        .unwrap_or("__default__")
        .to_string();
    let state_id = egui::Id::unique("od_applied_font_key");
    let already_applied = ctx.data_mut(|d| {
        let prev = d.get_temp::<String>(state_id);
        if prev.as_deref() == Some(applied_key.as_str()) {
            true
        } else {
            d.insert_temp(state_id, applied_key.clone());
            false
        }
    });
    if already_applied {
        return;
    }

    let mut defs = egui::FontDefinitions::default();
    if let Some(bytes) = custom_bytes {
        let name = "od_user_font".to_string();
        defs.font_data
            .insert(name.clone(), Arc::new(egui::FontData::from_owned(bytes)));
        defs.families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, name);
    }
    for (k, data) in unicode_fallback_fonts() {
        defs.font_data.insert((*k).to_string(), Arc::clone(data));
        defs.families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .push((*k).to_string());
        defs.families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push((*k).to_string());
    }
    ctx.set_fonts(defs);
}
