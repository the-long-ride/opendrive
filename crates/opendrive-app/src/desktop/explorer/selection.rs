//! Multi-selection session state helpers, visible sorting, toolbar summary, and keyboard shortcuts.

use super::file_ops_ui::{is_mutable_entry, paste_label_for};
use super::items::activate_entry_action;
use super::list_header::{compare_entries, ExplorerSortColumn};
use super::{AccountExplorerSession, ExplorerState};
use crate::desktop::UiAction;
use crate::runtime::explorer_ops::ExplorerEntry;
use eframe::egui;
use opendrive_core::MediaKind;
use opendrive_sync::NodeId;
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub(super) struct SelectionRef<'a> {
    pub entries: &'a [ExplorerEntry],
    pub selected_ids: &'a [NodeId],
    pub sort_col: ExplorerSortColumn,
    pub sort_desc: bool,
}

impl<'a> SelectionRef<'a> {
    pub fn is_selected(&self, id: NodeId) -> bool {
        self.selected_ids.contains(&id)
    }

    pub fn mutable_ids(&self, ids: &[NodeId]) -> Vec<NodeId> {
        filter_mutable_ids(self.entries, ids)
    }
}

pub(crate) fn filter_mutable_ids(entries: &[ExplorerEntry], ids: &[NodeId]) -> Vec<NodeId> {
    ids.iter()
        .copied()
        .filter(|id| {
            entries
                .iter()
                .find(|e| e.id == *id)
                .is_some_and(is_mutable_entry)
        })
        .collect()
}

pub(crate) fn visible_sorted_entries(s: &AccountExplorerSession) -> Vec<&ExplorerEntry> {
    let q = s.search_query.trim().to_ascii_lowercase();
    let mut filtered: Vec<&ExplorerEntry> = s
        .entries
        .iter()
        .filter(|e| q.is_empty() || e.name.to_ascii_lowercase().contains(&q))
        .collect();
    filtered.sort_by(|a, b| compare_entries(a, b, s.sort_col, s.sort_desc));
    filtered
}

fn visible_ids(s: &AccountExplorerSession) -> Vec<NodeId> {
    visible_sorted_entries(s)
        .into_iter()
        .map(|e| e.id)
        .collect()
}

impl AccountExplorerSession {
    pub fn toggle_sort(&mut self, col: ExplorerSortColumn) {
        if self.sort_col == col {
            self.sort_desc = !self.sort_desc;
        } else {
            self.sort_col = col;
            self.sort_desc = false;
        }
    }
    fn retain_visible(&self, vis: &HashSet<NodeId>) -> Vec<NodeId> {
        let mut seen = HashSet::new();
        let raw = if !self.selected_ids.is_empty() {
            self.selected_ids.clone()
        } else {
            self.selected_id.into_iter().collect()
        };
        raw.into_iter()
            .filter(|id| vis.contains(id) && seen.insert(*id))
            .collect()
    }

    pub fn effective_selected_ids(&self) -> Vec<NodeId> {
        let visible: HashSet<NodeId> = visible_ids(self).into_iter().collect();
        self.retain_visible(&visible)
    }

    pub fn clear_selection(&mut self) {
        self.selected_id = None;
        self.anchor_id = None;
        self.selected_ids.clear();
    }

    pub fn select_only(&mut self, id: Option<NodeId>) {
        self.selected_id = id;
        self.anchor_id = id;
        self.selected_ids = id.into_iter().collect();
    }

    pub fn toggle_select(&mut self, id: NodeId) {
        let mut ids = self.effective_selected_ids();
        if let Some(pos) = ids.iter().position(|&x| x == id) {
            ids.remove(pos);
        } else {
            ids.push(id);
        }
        self.anchor_id = Some(id);
        self.selected_id = ids.last().copied();
        self.selected_ids = ids;
    }

    pub fn select_range(&mut self, target_id: NodeId, additive: bool) {
        let vis = visible_ids(self);
        let anchor = self
            .anchor_id
            .or(self.selected_id)
            .filter(|a| vis.contains(a))
            .unwrap_or(target_id);
        let (Some(ia), Some(it)) = (
            vis.iter().position(|&x| x == anchor),
            vis.iter().position(|&x| x == target_id),
        ) else {
            return self.select_only(Some(target_id));
        };
        let range = &vis[ia.min(it)..=ia.max(it)];
        if additive {
            let mut ids = self.effective_selected_ids();
            for &id in range {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            self.selected_ids = ids;
        } else {
            self.selected_ids = range.to_vec();
        }
        self.anchor_id = Some(anchor);
        self.selected_id = Some(target_id);
    }

    pub fn select_all_visible(&mut self) {
        let vis = visible_ids(self);
        self.anchor_id = vis.first().copied();
        self.selected_id = vis.first().copied();
        self.selected_ids = vis;
    }

    pub fn prune_to_visible(&mut self) {
        let vis: HashSet<NodeId> = visible_ids(self).into_iter().collect();
        let kept = self.retain_visible(&vis);
        if self.anchor_id.is_some_and(|a| !vis.contains(&a)) {
            self.anchor_id = None;
        }
        self.selected_id = kept.last().copied();
        self.selected_ids = kept;
    }

    pub fn prune_to_entries(&mut self) {
        let mut seen = HashSet::new();
        self.selected_ids
            .retain(|id| seen.insert(*id) && self.entries.iter().any(|e| e.id == *id));
        if self
            .anchor_id
            .is_some_and(|a| !self.entries.iter().any(|e| e.id == a))
        {
            self.anchor_id = None;
        }
        self.selected_id = self.selected_ids.last().copied();
    }

    pub fn remove_selected(&mut self, removed: &[NodeId]) {
        self.selected_ids.retain(|id| !removed.contains(id));
        if self.anchor_id.is_some_and(|a| removed.contains(&a)) {
            self.anchor_id = None;
        }
        self.selected_id = self.selected_ids.last().copied();
    }
}

pub(super) struct ToolbarSelection {
    pub selected_ids: Vec<NodeId>,
    pub mutable_ids: Vec<NodeId>,
    pub single_entry: Option<ExplorerEntry>,
    pub can_rename: bool,
    pub is_single_video: bool,
}

impl ToolbarSelection {
    pub(super) fn from_session(session: Option<&AccountExplorerSession>) -> Self {
        let Some(s) = session else {
            return Self {
                selected_ids: Vec::new(),
                mutable_ids: Vec::new(),
                single_entry: None,
                can_rename: false,
                is_single_video: false,
            };
        };
        let selected_ids = s.effective_selected_ids();
        let mutable_ids = filter_mutable_ids(&s.entries, &selected_ids);
        let single_entry = (selected_ids.len() == 1)
            .then(|| s.entries.iter().find(|e| e.id == selected_ids[0]).cloned())
            .flatten();
        let can_rename = selected_ids.len() == 1 && mutable_ids.len() == 1;
        let is_single_video = single_entry
            .as_ref()
            .is_some_and(|e| e.media_kind == MediaKind::Video);
        Self {
            selected_ids,
            mutable_ids,
            single_entry,
            can_rename,
            is_single_video,
        }
    }
}

pub(crate) fn handle_explorer_shortcuts(
    ctx: &egui::Context,
    explorer: &ExplorerState,
    can_write: bool,
    overlay_open: bool,
    actions: &mut Vec<UiAction>,
) {
    if !explorer.open || overlay_open || ctx.egui_wants_keyboard_input() {
        return;
    }
    let tsel = ToolbarSelection::from_session(explorer.active_session());
    let has_paste = paste_label_for(
        explorer.clipboard.as_ref(),
        explorer.active_account_id,
        can_write,
    )
    .is_some();
    let popup_open = egui::Popup::is_any_open(ctx);
    ctx.input(|i| {
        use egui::{Event as E, Key as K};
        let mut pasted = false;
        for ev in &i.events {
            let key_hit = |target: K, need_cmd: bool| {
                matches!(
                    ev,
                    E::Key { key, pressed: true, repeat: false, modifiers, .. }
                        if *key == target && (!need_cmd || modifiers.command || modifiers.ctrl)
                )
            };
            if key_hit(K::A, true) {
                actions.push(UiAction::ExplorerSelectAll);
                continue;
            }
            if key_hit(K::Escape, false) && !tsel.selected_ids.is_empty() && !popup_open {
                actions.push(UiAction::ExplorerSelectEntry(None));
                continue;
            }
            if key_hit(K::Enter, false) && !popup_open {
                if let Some(entry) = &tsel.single_entry {
                    actions.push(activate_entry_action(entry));
                    continue;
                }
            }
            if !can_write {
                continue;
            }
            if !pasted && has_paste && (matches!(ev, E::Paste(_)) || key_hit(K::V, true)) {
                pasted = true;
                actions.push(UiAction::ExplorerPasteClipboard(None));
            } else if !tsel.mutable_ids.is_empty() {
                let ids = tsel.mutable_ids.clone();
                let clip = |cut| UiAction::ExplorerSetClipboard {
                    node_ids: ids.clone(),
                    cut,
                };
                if key_hit(K::F2, false) && tsel.can_rename {
                    actions.push(UiAction::ExplorerStartRename(ids[0]));
                } else if key_hit(K::Delete, false) {
                    actions.push(UiAction::ExplorerRequestDelete(ids));
                } else if matches!(ev, E::Copy) || key_hit(K::C, true) {
                    actions.push(clip(false));
                } else if matches!(ev, E::Cut) || key_hit(K::X, true) {
                    actions.push(clip(true));
                }
            }
        }
    });
}
