#![allow(dead_code)]
//! Shared headless UI & FakeDrive test helpers for Cloud Explorer integration tests.

use crate::common_auth_helpers::run_frame;
use eframe::egui::{self, Event, Key, Modifiers, Pos2, RawInput, Rect, Shape, Vec2};
use opendrive_app::design::theme::{palette, ThemeMode};
use opendrive_app::desktop::modals::open_rules_ui::render_open_rules_section;
use opendrive_app::desktop::ControlCenterApp;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_drive::DriveClient;
use opendrive_fake_drive::{FakeAbout, FakeDrive, FakeFile};
use opendrive_store::NodeState;
use opendrive_sync::SyncEngine;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

pub type LaunchLog = Arc<Mutex<Vec<(Option<String>, String)>>>;

pub fn collect(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
    fn walk(s: &Shape, out: &mut Vec<String>) {
        match s {
            Shape::Text(ts) => out.push(ts.galley.text().to_owned()),
            Shape::Vec(v) => v.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes.iter().for_each(|cs| walk(&cs.shape, &mut out));
    out
}

pub fn texts(ctx: &egui::Context, app: &mut ControlCenterApp) -> Vec<String> {
    let _ = run_frame(ctx, app, Vec::new());
    collect(&run_frame(ctx, app, Vec::new()).shapes)
}

pub fn has(v: &[String], s: &str) -> bool {
    v.iter().any(|x| x == s)
}

pub fn has_sub(v: &[String], s: &str) -> bool {
    v.iter().any(|x| x.contains(s))
}

pub fn wait_until(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    p: impl Fn(&ControlCenterApp) -> bool,
) -> Vec<String> {
    let dl = Instant::now() + Duration::from_secs(5);
    loop {
        let t = collect(&run_frame(ctx, app, Vec::new()).shapes);
        if p(app) {
            return t;
        }
        assert!(Instant::now() < dl, "timeout: {:?}", app.notice);
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub fn wait_entry(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    a: Uuid,
    n: &str,
) -> Vec<String> {
    wait_until(ctx, app, |x| {
        !x.explorer.is_busy(a)
            && x.explorer
                .sessions
                .get(&a)
                .is_some_and(|s| s.loaded && s.entries.iter().any(|e| e.name == n))
    })
}

pub fn wait_state(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    eng: &SyncEngine,
    a: Uuid,
    id: u64,
    st: NodeState,
) -> Vec<String> {
    wait_until(ctx, app, |x| {
        !x.explorer.is_busy(a) && state_of(eng, id) == st
    })
}

pub fn seed_drive(
    items: &[(&str, &str, &str, &str, &[u8])],
    used: u64,
    quota: u64,
) -> Arc<FakeDrive> {
    let d = Arc::new(FakeDrive::new());
    d.set_about(FakeAbout {
        storage_used: used,
        storage_quota: quota,
        ..Default::default()
    });
    for &(id, name, mime, parent, data) in items {
        d.add_file(FakeFile {
            id: id.into(),
            name: name.into(),
            mime_type: mime.into(),
            size: data.len() as u64,
            parents: vec![parent.into()],
            content: data.to_vec(),
            head_revision_id: Some("rev-1".into()),
            ..Default::default()
        });
    }
    d
}

pub fn add_acct(
    br: &Bridge,
    eng: &SyncEngine,
    name: &str,
    addr: std::net::SocketAddr,
    used: u64,
    quota: u64,
) -> Uuid {
    let res = br
        .add_account(name.into(), format!("{name}@x.com"), name.into(), false)
        .unwrap();
    let id = res.accounts.last().unwrap().id;
    let ctrl = br.controller_handle();
    let mut g = ctrl.lock().unwrap();
    let a = g.accounts.get_account_mut(id).unwrap();
    (a.storage_used, a.storage_quota) = (used, quota);
    let (api, up) = (
        format!("http://{addr}/drive/v3"),
        format!("http://{addr}/upload/drive/v3"),
    );
    eng.set_drive_client(id, DriveClient::with_base_urls("tok".into(), api, up));
    id
}

pub fn eid(app: &ControlCenterApp, acct: Uuid, name: &str) -> u64 {
    app.explorer.sessions[&acct]
        .entries
        .iter()
        .find(|e| e.name == name)
        .unwrap()
        .id
}

pub fn state_of(eng: &SyncEngine, id: u64) -> NodeState {
    eng.store()
        .lock()
        .get_node(id as i64)
        .unwrap()
        .unwrap()
        .state
}

pub fn dirty_cached(eng: &SyncEngine, id: u64, data: &[u8]) -> String {
    let p = eng
        .store()
        .lock()
        .get_cache_path(id as i64)
        .unwrap()
        .unwrap();
    std::fs::write(&p, data).unwrap();
    eng.store()
        .lock()
        .update_node_state(id as i64, NodeState::Dirty, Some(true))
        .unwrap();
    p
}

pub fn render_rules_and_close(ctx: &egui::Context, app: &mut ControlCenterApp) -> Vec<String> {
    let mut acts = Vec::new();
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(620.0, 400.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| {
        let rules = &app.state.settings.open_with_rules;
        render_open_rules_section(
            ui,
            &palette(ThemeMode::DarkStudio),
            rules,
            &mut app.open_rule_draft,
            &mut acts,
        );
    });
    out.textures_delta.clear();
    let res = collect(&out.shapes);
    let esc = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 760.0))),
        events: vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        ..Default::default()
    };
    ctx.run_ui(esc, |ui| app.render_root(ui))
        .textures_delta
        .clear();
    res
}

pub fn send_mods(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    modifiers: Modifiers,
    events: Vec<Event>,
) -> egui::FullOutput {
    let mut evs = vec![Event::ModifiersChanged(modifiers)];
    evs.extend(events);
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 620.0))),
        events: evs,
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    out
}

pub fn send_frame(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    events: Vec<Event>,
) -> egui::FullOutput {
    send_mods(ctx, app, Modifiers::NONE, events)
}

pub fn text_pos(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Pos2 {
    fn walk(s: &Shape, needle: &str) -> Option<Pos2> {
        match s {
            Shape::Text(ts) if ts.galley.text() == needle => {
                Some(ts.galley.rect.translate(ts.pos.to_vec2()).center())
            }
            Shape::Vec(v) => v.iter().find_map(|x| walk(x, needle)),
            _ => None,
        }
    }
    if let Some(p) = shapes.iter().find_map(|cs| walk(&cs.shape, needle)) {
        p
    } else {
        panic!("missing text {needle}")
    }
}

fn send_tap(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    modifiers: Modifiers,
    ev: impl Fn(bool) -> Vec<Event>,
) {
    for pressed in [true, false] {
        send_mods(ctx, app, modifiers, ev(pressed));
    }
    send_frame(ctx, app, Vec::new());
}

pub fn click_at(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    pos: Pos2,
    button: egui::PointerButton,
    mods: Modifiers,
) {
    send_tap(ctx, app, mods, |pressed| {
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: mods,
            },
        ]
    });
}

pub fn tap_key(ctx: &egui::Context, app: &mut ControlCenterApp, key: Key, mods: Modifiers) {
    send_tap(ctx, app, mods, |pressed| {
        vec![Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: mods,
        }]
    });
}
