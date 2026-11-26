//! List view column geometry, sorting, theme-styled checkboxes, and empty-area click handling.

use crate::design::theme::ThemePalette;
use crate::design::tooltip::attach_smart_tooltip;
use crate::design::widgets::{paint_icon, IconKind};
use crate::desktop::UiAction;
use crate::runtime::explorer_ops::ExplorerEntry;
use eframe::egui::{
    self, Align, CornerRadius, CursorIcon, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind,
    Vec2,
};
use opendrive_store::NodeState;
use opendrive_sync::SHARED_WITH_ME_DRIVE_ID;
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExplorerSortColumn {
    #[default]
    Name,
    SyncState,
    Modified,
    Size,
}

fn state_rank(state: &NodeState) -> u8 {
    match state {
        NodeState::Conflict => 0,
        NodeState::Dirty => 1,
        NodeState::Hydrating => 2,
        NodeState::Cached => 3,
        NodeState::Placeholder => 4,
    }
}

pub(super) fn compare_entries(
    a: &ExplorerEntry,
    b: &ExplorerEntry,
    sort_col: ExplorerSortColumn,
    sort_desc: bool,
) -> Ordering {
    let a_swm = a.drive_id == SHARED_WITH_ME_DRIVE_ID;
    let b_swm = b.drive_id == SHARED_WITH_ME_DRIVE_ID;
    b_swm
        .cmp(&a_swm)
        .then_with(|| b.is_folder.cmp(&a.is_folder))
        .then_with(|| {
            let ord = match sort_col {
                ExplorerSortColumn::Name => a
                    .name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase()),
                ExplorerSortColumn::SyncState => state_rank(&a.state).cmp(&state_rank(&b.state)),
                ExplorerSortColumn::Modified => a.modified_ms.cmp(&b.modified_ms),
                ExplorerSortColumn::Size => a.size.cmp(&b.size),
            };
            if sort_desc {
                ord.reverse()
            } else {
                ord
            }
        })
        .then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        })
        .then_with(|| a.id.cmp(&b.id))
}

#[derive(Clone, Copy)]
pub(super) enum CheckboxState {
    Unchecked,
    Indeterminate,
    Checked,
}

pub(super) struct ListCols {
    pub check_x: f32,
    pub name: (f32, f32),
    pub state: (f32, f32),
    pub modified: Option<(f32, f32)>,
    pub size: (f32, f32),
}

impl ListCols {
    pub(super) fn compute(left: f32, width: f32) -> Self {
        let (pad_x, gap) = (12.0, 16.0);
        let right = left + width - pad_x;
        let size = (right - 76.0, right);
        let modified = (width >= 540.0).then_some((size.0 - gap - 138.0, size.0 - gap));
        let prev = modified.map_or(size.0, |m| m.0);
        let state = (prev - gap - 84.0, prev - gap);
        let name_start = left + pad_x + 26.0;
        let name = (name_start, (state.0 - gap).max(name_start + 72.0));
        Self {
            check_x: left + pad_x + 8.0,
            name,
            state,
            modified,
            size,
        }
    }
}

pub(super) fn row_checkbox_rect(cols: &ListCols, center_y: f32) -> Rect {
    Rect::from_center_size(Pos2::new(cols.check_x, center_y), Vec2::splat(16.0))
}

pub(super) fn paint_theme_checkbox(
    p: &egui::Painter,
    rect: Rect,
    state: CheckboxState,
    pal: &ThemePalette,
    hovered: bool,
) {
    let rad = CornerRadius::same(4);
    match state {
        CheckboxState::Checked => {
            p.rect_filled(rect.translate(Vec2::new(1.5, 1.5)), rad, pal.tactile_ink);
            p.rect(
                rect,
                rad,
                pal.accent_blue,
                Stroke::new(1.5, pal.tactile_ink),
                StrokeKind::Inside,
            );
            paint_icon(p, rect.shrink(2.0), IconKind::Check, egui::Color32::WHITE);
        }
        CheckboxState::Indeterminate => {
            p.rect(
                rect,
                rad,
                pal.accent_blue_tint,
                Stroke::new(1.5, pal.accent_blue),
                StrokeKind::Inside,
            );
            let y = rect.center().y;
            p.line_segment(
                [
                    Pos2::new(rect.left() + 3.5, y),
                    Pos2::new(rect.right() - 3.5, y),
                ],
                Stroke::new(2.0, pal.accent_blue),
            );
        }
        CheckboxState::Unchecked => {
            let stroke = if hovered {
                pal.pop_cyan
            } else {
                pal.card_border
            };
            p.rect(
                rect,
                rad,
                pal.card_bg,
                Stroke::new(1.5, stroke),
                StrokeKind::Inside,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_sort_cell(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    x: (f32, f32),
    row: Rect,
    right_align: bool,
    label: &str,
    col: ExplorerSortColumn,
    (active_col, sort_desc): (ExplorerSortColumn, bool),
    actions: &mut Vec<UiAction>,
) {
    let rect = Rect::from_min_max(
        Pos2::new(x.0, row.top() + 2.0),
        Pos2::new(x.1, row.bottom() - 2.0),
    );
    if !ui.clip_rect().intersect(rect).is_positive() {
        return;
    }
    let resp = ui.interact(rect, egui::Id::unique(("exp_sort", label)), Sense::click());
    let hovered = resp.hovered();
    let active = active_col == col;
    let color = if active {
        pal.accent_blue
    } else if hovered {
        pal.text_primary
    } else {
        pal.text_muted
    };
    let layout = if right_align {
        Layout::right_to_left(Align::Center)
    } else {
        Layout::left_to_right(Align::Center)
    };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
    child.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
    let paint_chev = |ui: &mut egui::Ui| {
        if active {
            let (ir, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
            let icon = if sort_desc {
                IconKind::ChevronDown
            } else {
                IconKind::ChevronUp
            };
            paint_icon(ui.painter(), ir, icon, pal.accent_blue);
        }
    };
    let lbl = RichText::new(label).size(11.5).strong().color(color);
    if right_align {
        paint_chev(&mut child);
        child.add(egui::Label::new(lbl).selectable(false));
    } else {
        child.add(egui::Label::new(lbl).selectable(false));
        paint_chev(&mut child);
    }
    let next_dir = if active && !sort_desc {
        "descending"
    } else {
        "ascending"
    };
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    attach_smart_tooltip(&resp, ui, pal, &format!("Sort by {label} ({next_dir})"));
    if resp.clicked() {
        actions.push(UiAction::ExplorerToggleSort(col));
    }
}

pub(super) fn render_list_header(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    row_w: f32,
    total_visible: usize,
    selected_count: usize,
    sort: (ExplorerSortColumn, bool),
    actions: &mut Vec<UiAction>,
) {
    ui.style_mut().interaction.selectable_labels = false;
    let (r, _) = ui.allocate_exact_size(Vec2::new(row_w, 29.0), Sense::hover());
    ui.painter().rect(
        r,
        CornerRadius::same(8),
        pal.pill_neutral,
        Stroke::new(1.5, pal.card_border),
        StrokeKind::Inside,
    );
    let cols = ListCols::compute(r.left(), row_w);
    let cb_rect = row_checkbox_rect(&cols, r.center().y);
    let cb_resp = ui
        .interact(
            cb_rect.expand(5.0),
            egui::Id::unique("exp_hdr_cb"),
            Sense::click(),
        )
        .on_hover_cursor(CursorIcon::PointingHand);
    let all_sel = total_visible > 0 && selected_count == total_visible;
    let cb_state = if all_sel {
        CheckboxState::Checked
    } else if selected_count > 0 {
        CheckboxState::Indeterminate
    } else {
        CheckboxState::Unchecked
    };
    paint_theme_checkbox(ui.painter(), cb_rect, cb_state, pal, cb_resp.hovered());
    let tip = if all_sel {
        "Clear selection"
    } else {
        "Select all visible items"
    };
    attach_smart_tooltip(&cb_resp, ui, pal, tip);
    if cb_resp.clicked() && total_visible > 0 {
        actions.push(if all_sel {
            UiAction::ExplorerSelectEntry(None)
        } else {
            UiAction::ExplorerSelectAll
        });
    }
    use ExplorerSortColumn as C;
    render_sort_cell(ui, pal, cols.name, r, false, "Name", C::Name, sort, actions);
    render_sort_cell(
        ui,
        pal,
        cols.state,
        r,
        false,
        "Sync State",
        C::SyncState,
        sort,
        actions,
    );
    if let Some(m) = cols.modified {
        render_sort_cell(ui, pal, m, r, false, "Modified", C::Modified, sort, actions);
    }
    render_sort_cell(ui, pal, cols.size, r, true, "Size", C::Size, sort, actions);
}
