//! State actions for adding, editing, validating, and removing Open With Rules.

use super::open_rules_ui::OpenRuleDraft;
use crate::desktop::{ControlCenterApp, UiAction};
use opendrive_core::validate_open_with_rule;

impl ControlCenterApp {
    fn reset_open_rule_editor(&mut self) {
        let search = self.open_rule_draft.search_query.clone();
        self.open_rule_draft = OpenRuleDraft {
            search_query: search,
            ..Default::default()
        };
    }

    fn save_open_with_rule(&mut self, pat: &str, cmd: &str, edit_idx: Option<usize>) {
        let mut rules = self.state.settings.open_with_rules.clone();
        let valid = match validate_open_with_rule(pat, cmd, &rules, edit_idx) {
            Ok(rule) => rule,
            Err(err) => {
                self.open_rule_draft.error = Some(err.clone());
                self.set_err(err);
                return;
            }
        };
        let msg = if let Some(idx) = edit_idx.filter(|i| *i < rules.len()) {
            rules[idx] = valid;
            "Saved Open With rule"
        } else {
            rules.push(valid);
            "Added Open With rule"
        };
        let res = self.bridge.update_open_with_rules(rules);
        if res.is_ok() {
            self.reset_open_rule_editor();
        }
        self.apply_result(res, msg);
    }

    pub(crate) fn handle_open_rule_action(&mut self, action: UiAction) {
        match action {
            UiAction::StartAddOpenWithRule => {
                self.reset_open_rule_editor();
                self.open_rule_draft.editor_open = true;
            }
            UiAction::StartEditOpenWithRule(idx) => {
                if let Some(rule) = self.state.settings.open_with_rules.get(idx).cloned() {
                    self.open_rule_draft.editor_open = true;
                    self.open_rule_draft.editing_index = Some(idx);
                    self.open_rule_draft.pattern = rule.pattern;
                    self.open_rule_draft.app_command = rule.app_command;
                    self.open_rule_draft.error = None;
                }
            }
            UiAction::CancelOpenWithRuleEdit => self.reset_open_rule_editor(),
            UiAction::BrowseOpenWithRuleApp => {
                if let Some(path) = (self.auth_endpoints.app_exe_picker)() {
                    self.open_rule_draft.editor_open = true;
                    self.open_rule_draft.app_command = path.display().to_string();
                    self.open_rule_draft.error = None;
                }
            }
            UiAction::SaveOpenWithRule => {
                let (pat, cmd, edit_idx) = (
                    self.open_rule_draft.pattern.clone(),
                    self.open_rule_draft.app_command.clone(),
                    self.open_rule_draft.editing_index,
                );
                self.save_open_with_rule(&pat, &cmd, edit_idx);
            }
            UiAction::RemoveOpenWithRule(idx) => {
                if idx >= self.state.settings.open_with_rules.len() {
                    return;
                }
                if self.open_rule_draft.editing_index == Some(idx) {
                    self.reset_open_rule_editor();
                } else if let Some(ed) = self.open_rule_draft.editing_index.as_mut() {
                    if *ed > idx {
                        *ed -= 1;
                    }
                }
                let res = self.bridge.remove_open_with_rule(idx);
                self.apply_result(res, "Removed Open With rule");
            }
            _ => {}
        }
    }
}
