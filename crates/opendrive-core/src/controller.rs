//! UI-agnostic app controller (channels in/out for GPUI or CLI).

use crate::{Account, AccountManager, AccountStatus, Settings};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum AppCommand {
    AddAccount { nickname: String },
    RemoveAccount { id: Uuid },
    PauseAccount { id: Uuid },
    ResumeAccount { id: Uuid },
    PauseAll,
    ResumeAll,
    OpenFolder,
    Quit,
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    AccountUpdated(Account),
    SyncProgress { transfers: u64, queue: usize },
    Error(String),
    Ready,
}

pub struct AppController {
    pub accounts: AccountManager,
    pub settings: Settings,
}

impl AppController {
    pub fn new(settings: Settings) -> Self {
        Self {
            accounts: AccountManager::new(),
            settings,
        }
    }

    pub fn handle(&mut self, cmd: AppCommand) -> Vec<AppEvent> {
        match cmd {
            AppCommand::PauseAccount { id } => {
                if let Ok(a) = self.accounts.get_account(id) {
                    let mut a = a.clone();
                    a.status = AccountStatus::Paused;
                    a.enabled = false;
                    // replace
                    let _ = self.accounts.remove_account(id);
                    self.accounts.add_account(a.clone());
                    vec![AppEvent::AccountUpdated(a)]
                } else {
                    vec![AppEvent::Error("account not found".into())]
                }
            }
            AppCommand::ResumeAccount { id } => {
                if let Ok(a) = self.accounts.get_account(id) {
                    let mut a = a.clone();
                    a.status = AccountStatus::Connected;
                    a.enabled = true;
                    let _ = self.accounts.remove_account(id);
                    self.accounts.add_account(a.clone());
                    vec![AppEvent::AccountUpdated(a)]
                } else {
                    vec![AppEvent::Error("account not found".into())]
                }
            }
            AppCommand::RemoveAccount { id } => {
                match self.accounts.remove_account(id) {
                    Ok(()) => vec![AppEvent::Ready],
                    Err(e) => vec![AppEvent::Error(e.to_string())],
                }
            }
            AppCommand::PauseAll => {
                let ids: Vec<_> = self.accounts.list_accounts().iter().map(|a| a.id).collect();
                let mut ev = Vec::new();
                for id in ids {
                    ev.extend(self.handle(AppCommand::PauseAccount { id }));
                }
                ev
            }
            AppCommand::ResumeAll => {
                let ids: Vec<_> = self.accounts.list_accounts().iter().map(|a| a.id).collect();
                let mut ev = Vec::new();
                for id in ids {
                    ev.extend(self.handle(AppCommand::ResumeAccount { id }));
                }
                ev
            }
            AppCommand::AddAccount { nickname } => {
                let a = Account {
                    id: Uuid::new_v4(),
                    nickname: nickname.clone(),
                    email: String::new(),
                    credential_set_id: Uuid::nil(),
                    folder_name: nickname,
                    read_only: false,
                    enabled: true,
                    status: AccountStatus::NeedsReauth,
                };
                self.accounts.add_account(a.clone());
                vec![AppEvent::AccountUpdated(a)]
            }
            AppCommand::OpenFolder | AppCommand::Quit => vec![AppEvent::Ready],
        }
    }
}
