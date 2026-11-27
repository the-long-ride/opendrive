//! Direct widget-click and edge-branch tests covering font_ui, font_import, and fonts closures.

use eframe::egui::{
    self, CentralPanel, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2,
};
use opendrive_app::design::theme::{palette, ThemeMode};
use opendrive_app::desktop::modals::font_ui::{render_font_section, FontPickerDraft};
use opendrive_app::desktop::UiAction;
use opendrive_app::runtime::font_import::{
    encode_google_font_query, import_google_font_family, import_local_font_file,
};
use opendrive_app::runtime::fonts::{discover_fonts_in_dirs, validate_font_bytes, DiscoveredFont};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use uuid::Uuid;

fn collect_pos(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<Pos2> {
    fn walk(s: &Shape, needle: &str) -> Option<Pos2> {
        match s {
            Shape::Text(t) if t.galley.text() == needle => Some(s.visual_bounding_rect().center()),
            Shape::Vec(v) => v.iter().find_map(|x| walk(x, needle)),
            _ => None,
        }
    }
    shapes.iter().rev().find_map(|cs| walk(&cs.shape, needle))
}

fn find_popup_rect(shapes: &[egui::epaint::ClippedShape], pt: Pos2) -> Rect {
    fn walk(s: &Shape, pt: Pos2, best: &mut Option<Rect>) {
        match s {
            Shape::Rect(rs)
                if rs.rect.contains(pt)
                    && rs.rect.height() >= 180.0
                    && rs.rect.width() >= 240.0 =>
            {
                if best.is_none_or(|b| rs.rect.area() <= b.area()) {
                    *best = Some(rs.rect);
                }
            }
            Shape::Vec(v) => v.iter().for_each(|x| walk(x, pt, best)),
            _ => {}
        }
    }
    let mut best = None;
    shapes.iter().for_each(|cs| walk(&cs.shape, pt, &mut best));
    best.expect("font dropdown popup rect")
}

fn draw_font_ui(
    ctx: &egui::Context,
    active: Option<&str>,
    fonts: &[DiscoveredFont],
    draft: &mut FontPickerDraft,
    events: Vec<Event>,
) -> (Vec<UiAction>, Vec<egui::epaint::ClippedShape>) {
    let mut actions = Vec::new();
    let pal = palette(ThemeMode::DarkStudio);
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(720.0, 420.0))),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| {
        CentralPanel::default().show(ui, |ui| {
            render_font_section(ui, &pal, active, fonts, draft, &mut actions);
        });
    });
    out.textures_delta.clear();
    (actions, out.shapes)
}

#[test]
fn font_ui_widgets_and_closures_cover_all_branches() {
    let ctx = egui::Context::default();
    let fonts = vec![
        DiscoveredFont {
            family: "Inter".into(),
            path: PathBuf::from("inter.ttf"),
            imported: true,
        },
        DiscoveredFont {
            family: "Roboto".into(),
            path: PathBuf::from("roboto.ttf"),
            imported: false,
        },
    ];
    let mut draft = FontPickerDraft {
        query: String::new(),
        import_input: "Lato".into(),
        feedback: Some("Invalid font".into()),
        feedback_is_err: true,
        importing: false,
    };
    let (_, shapes0) = draw_font_ui(&ctx, Some("Inter"), &fonts, &mut draft, Vec::new());

    let click = |active: Option<&str>, draft: &mut FontPickerDraft, pos: Pos2| -> Vec<UiAction> {
        let mut acts = Vec::new();
        for pressed in [true, false] {
            let btn = Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            acts.extend(
                draw_font_ui(
                    &ctx,
                    active,
                    &fonts,
                    draft,
                    vec![Event::PointerMoved(pos), btn],
                )
                .0,
            );
        }
        acts
    };

    let mut got = Vec::new();
    for label in ["Reset Font", "Import", "Browse"] {
        let pos = collect_pos(&shapes0, label).unwrap();
        got.extend(click(Some("Inter"), &mut draft, pos));
    }
    assert!(got.iter().any(|a| matches!(a, UiAction::ResetUiFont)));
    assert!(got.iter().any(|a| matches!(a, UiAction::ImportUiFont)));
    assert!(got.iter().any(|a| matches!(a, UiAction::BrowseUiFont)));

    let combo_pos = collect_pos(&shapes0, "Inter").unwrap();
    let _ = click(Some("Inter"), &mut draft, combo_pos);
    let (_, popup_shapes) = draw_font_ui(&ctx, Some("Inter"), &fonts, &mut draft, Vec::new());
    assert!(egui::Popup::is_any_open(&ctx));
    let search_pos = collect_pos(&popup_shapes, "Search installed fonts...").expect("search box");
    let popup_full = find_popup_rect(&popup_shapes, search_pos);
    let _ = click(Some("Inter"), &mut draft, search_pos);
    assert!(
        egui::Popup::is_any_open(&ctx),
        "clicking font search input inside dropdown closed popup prematurely"
    );
    draft.query = "zzznomatch".into();
    let (_, empty_shapes) = draw_font_ui(&ctx, Some("Inter"), &fonts, &mut draft, Vec::new());
    assert!(collect_pos(&empty_shapes, "No matching fonts").is_some());
    let popup_empty = find_popup_rect(&empty_shapes, search_pos);
    assert!(
        (popup_full.width() - popup_empty.width()).abs() <= 2.0
            && (popup_full.height() - popup_empty.height()).abs() <= 2.0,
        "dropdown size shrank on empty filter: {popup_full:?} vs {popup_empty:?}"
    );
    draft.query = "rob".into();
    let (_, filtered_shapes) = draw_font_ui(&ctx, Some("Inter"), &fonts, &mut draft, Vec::new());
    let popup_filtered = find_popup_rect(&filtered_shapes, search_pos);
    assert!(
        (popup_full.width() - popup_filtered.width()).abs() <= 2.0
            && (popup_full.height() - popup_filtered.height()).abs() <= 2.0,
        "dropdown size changed on filter match: {popup_full:?} vs {popup_filtered:?}"
    );
    assert!(collect_pos(&filtered_shapes, "Inter [Imported]").is_none());
    if let Some(p) = collect_pos(&filtered_shapes, "Roboto") {
        got.extend(click(Some("Inter"), &mut draft, p));
    }
    assert!(got
        .iter()
        .any(|a| matches!(a, UiAction::SelectUiFont(f) if f == "Roboto")));
    assert!(!egui::Popup::is_any_open(&ctx));

    // Focus input + Enter + click away (lost_focus && Enter)
    let (_, shapes_idle) = draw_font_ui(&ctx, None, &fonts, &mut draft, Vec::new());
    let in_pos = Pos2::new(120.0, 128.0);
    for pressed in [true, false] {
        let evs = vec![
            Event::PointerMoved(in_pos),
            Event::PointerButton {
                pos: in_pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ];
        let _ = draw_font_ui(&ctx, None, &fonts, &mut draft, evs);
    }
    let _ = collect_pos(&shapes_idle, "UI Font");
    let evs = vec![
        Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
        Event::Key {
            key: Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
    ];
    let _ = draw_font_ui(&ctx, None, &fonts, &mut draft, evs);
}

#[test]
fn font_validation_and_import_fs_error_branches_work() {
    let mut bad_ttc_off = Vec::from(*b"ttcf\x00\x01\x00\x00\x00\x00\x00\x01\x00\x00\x00\x99");
    assert!(validate_font_bytes(&bad_ttc_off).is_err());
    bad_ttc_off[15] = 0x10;
    bad_ttc_off.extend_from_slice(b"OTTO");
    assert!(validate_font_bytes(&bad_ttc_off).is_err());

    let mut no_name = vec![0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    no_name.extend_from_slice(b"head");
    no_name.extend_from_slice(&[0u8; 12]);
    let tmp = std::env::temp_dir().join(format!("od-fui3-{}", Uuid::new_v4()));
    let (sys_d, imp_d) = (tmp.join("sys"), tmp.join("imp"));
    std::fs::create_dir_all(&sys_d).unwrap();
    let weird = tmp.join("___.bin");
    std::fs::write(&weird, &no_name).unwrap();
    let (fam, saved) = import_local_font_file(&weird, &imp_d).unwrap();
    assert_eq!(fam, "___");
    assert!(saved.ends_with("custom_font.ttf"));

    // Pre-create collision directory for fs::write error + invalid ttf in discovery
    std::fs::remove_file(&saved).unwrap();
    std::fs::create_dir_all(&saved).unwrap();
    assert!(import_local_font_file(&weird, &imp_d).is_err());
    std::fs::write(sys_d.join("FallbackStem.ttf"), b"not-valid-font").unwrap();
    std::fs::write(sys_d.join("NoName.ttf"), &no_name).unwrap();
    let found = discover_fonts_in_dirs(std::slice::from_ref(&sys_d), Some(&imp_d));
    assert!(found.iter().any(|f| f.family == "FallbackStem"));

    // Google Fonts mock serving no_name bytes + fs error
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let nn = no_name.clone();
    let _srv = std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 512];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let (hdr, body) = if req.contains("GET /ok.ttf") {
                (
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        nn.len()
                    ),
                    nn.clone(),
                )
            } else {
                let css = format!("src: url(http://{addr}/ok.ttf) format('truetype');");
                (
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        css.len()
                    ),
                    css.into_bytes(),
                )
            };
            let _ = s.write_all(hdr.as_bytes());
            let _ = s.write_all(&body);
        }
    });
    let file_dir = tmp.join("fdir");
    std::fs::write(&file_dir, b"x").unwrap();
    assert!(
        import_google_font_family("NoName", &format!("http://{addr}/css2"), &file_dir).is_err()
    );
    let g_dir = tmp.join("gcol");
    std::fs::create_dir_all(g_dir.join("NoName.ttf")).unwrap();
    assert!(import_google_font_family("NoName", &format!("http://{addr}/css2"), &g_dir).is_err());

    // Query encoding & Regular-over-Bold prioritization
    assert_eq!(
        encode_google_font_query("Open Sans&x=1#a"),
        "Open+Sans%26x%3D1%23a"
    );
    let rank_dir = tmp.join("ranks");
    std::fs::create_dir_all(&rank_dir).unwrap();
    let b_font = rank_dir.join("MyFaceBold.ttf");
    let l_font = rank_dir.join("MyFaceLight.ttf");
    let r_font = rank_dir.join("MyFace.ttf");
    let mut named_font = Vec::from(*b"OTTO\x00\x01\x00\x00\x00\x00\x00\x00name\x00\x00\x00\x00");
    named_font.extend_from_slice(&28u32.to_be_bytes());
    named_font.extend_from_slice(&22u32.to_be_bytes());
    named_font.extend_from_slice(&[0, 0, 0, 1, 0, 18, 0, 1, 0, 0, 0, 0, 0, 1, 0, 4, 0, 0]);
    named_font.extend_from_slice(b"Face");
    std::fs::write(&b_font, &named_font).unwrap();
    std::fs::write(&l_font, &named_font).unwrap();
    std::fs::write(&r_font, &named_font).unwrap();
    let ranked = discover_fonts_in_dirs(std::slice::from_ref(&rank_dir), None);
    let picked = ranked.iter().find(|f| f.family == "Face").unwrap();
    assert_eq!(picked.path, r_font);
    let _ = std::fs::remove_dir_all(&tmp);
}
