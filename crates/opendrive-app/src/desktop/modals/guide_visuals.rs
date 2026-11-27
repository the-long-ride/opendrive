//! Vector flow diagrams for the Google Cloud Console setup guide modal.

use crate::design::theme::ThemePalette;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2,
};

struct FlowNode {
    step: &'static str,
    title: &'static str,
    detail: &'static str,
}

const OAUTH_FLOW: &[FlowNode] = &[
    FlowNode {
        step: "1",
        title: "GCP Project",
        detail: "Enable Drive API",
    },
    FlowNode {
        step: "2",
        title: "Auth Screen",
        detail: "Personal / External",
    },
    FlowNode {
        step: "3",
        title: "Desktop Client",
        detail: "Copy ID & Secret",
    },
    FlowNode {
        step: "4",
        title: "Browser PKCE",
        detail: "Sign in -> Sync",
    },
];

const SA_FLOW: &[FlowNode] = &[
    FlowNode {
        step: "1",
        title: "Drive API",
        detail: "Enable in project",
    },
    FlowNode {
        step: "2",
        title: "Service Acct",
        detail: "Create JSON key",
    },
    FlowNode {
        step: "3",
        title: "Drive Access",
        detail: "Add client_email",
    },
    FlowNode {
        step: "4",
        title: "Drop JSON",
        detail: "Zero browser login",
    },
];

fn paint_flow_diagram(ui: &mut egui::Ui, pal: &ThemePalette, nodes: &[FlowNode], accent: Color32) {
    let w = ui.available_width().max(260.0);
    let h = 76.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let p = ui.painter();
    let count = nodes.len() as f32;
    let gap = 14.0;
    let card_w = ((w - gap * (count - 1.0)) / count).max(52.0);
    let card_h = h - 6.0;

    for (idx, node) in nodes.iter().enumerate() {
        let x = rect.left() + (idx as f32) * (card_w + gap);
        let card = Rect::from_min_size(Pos2::new(x, rect.top() + 3.0), Vec2::new(card_w, card_h));
        let rad = CornerRadius::same(8);
        p.rect_filled(card.translate(Vec2::splat(2.0)), rad, pal.tactile_ink);
        p.rect(
            card,
            rad,
            pal.card_bg,
            Stroke::new(1.5, accent),
            StrokeKind::Inside,
        );
        let badge_c = Pos2::new(card.left() + 14.0, card.top() + 15.0);
        p.circle_filled(badge_c, 8.5, accent);
        p.text(
            badge_c,
            Align2::CENTER_CENTER,
            node.step,
            FontId::proportional(10.5),
            Color32::WHITE,
        );
        p.text(
            Pos2::new(card.left() + 27.0, card.top() + 15.0),
            Align2::LEFT_CENTER,
            node.title,
            FontId::proportional(11.5),
            pal.text_primary,
        );
        p.text(
            Pos2::new(card.left() + 9.0, card.top() + 44.0),
            Align2::LEFT_CENTER,
            node.detail,
            FontId::proportional(10.5),
            pal.text_muted,
        );
        if idx + 1 < nodes.len() {
            let ax1 = card.right() + 2.0;
            let ax2 = card.right() + gap - 2.0;
            let ay = card.center().y;
            let stroke = Stroke::new(1.8, accent);
            p.line_segment([Pos2::new(ax1, ay), Pos2::new(ax2, ay)], stroke);
            p.line_segment([Pos2::new(ax2 - 4.0, ay - 3.5), Pos2::new(ax2, ay)], stroke);
            p.line_segment([Pos2::new(ax2 - 4.0, ay + 3.5), Pos2::new(ax2, ay)], stroke);
        }
    }
}

pub fn render_oauth_flow_diagram(ui: &mut egui::Ui, pal: &ThemePalette) {
    paint_flow_diagram(ui, pal, OAUTH_FLOW, pal.accent_blue);
}

pub fn render_service_account_flow_diagram(ui: &mut egui::Ui, pal: &ThemePalette) {
    paint_flow_diagram(ui, pal, SA_FLOW, pal.pop_pink);
}
