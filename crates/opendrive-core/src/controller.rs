//! UI-agnostic app controller (channels in/out for GPUI or CLI).

use crate::{
    default_account_alias, sanitize_folder_segment, Account, AccountManager, AccountStatus,
    Settings,
};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum AppCommand {
    AddAccount {
        nickname: String,
    },
    RenameAccount {
        id: Uuid,
        nickname: String,
        folder_name: Option<String>,
    },
    RemoveAccount {
        id: Uuid,
    },
    PauseAccount {
        id: Uuid,
    },
    ResumeAccount {
        id: Uuid,
    },
    SetReadOnly {
        id: Uuid,
        read_only: bool,
    },
    MoveAccount {
        id: Uuid,
        delta: isize,
    },
    PauseAll,
    ResumeAll,
    UpdateSettings(Settings),
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
            AppCommand::PauseAccount { id } => self.update_acct(id, |a| {
                a.status = AccountStatus::Paused;
                a.enabled = false;
            }),
            AppCommand::ResumeAccount { id } => self.update_acct(id, |a| {
                a.status = AccountStatus::Connected;
                a.enabled = true;
            }),
            AppCommand::SetReadOnly { id, read_only } => self.update_acct(id, |a| {
                a.read_only = read_only;
            }),
            AppCommand::MoveAccount { id, delta } => match self.accounts.move_account(id, delta) {
                Ok(_) => vec![AppEvent::Ready],
                Err(e) => vec![AppEvent::Error(e.to_string())],
            },
            AppCommand::RenameAccount {
                id,
                nickname,
                folder_name,
            } => self.update_acct(id, |a| {
                a.nickname = default_account_alias(&nickname, &a.email);
                if let Some(raw_folder) = folder_name {
                    if let Some(clean) = sanitize_folder_segment(&raw_folder) {
                        a.folder_name = clean;
                    }
                }
            }),
            AppCommand::RemoveAccount { id } => match self.accounts.remove_account(id) {
                Ok(()) => vec![AppEvent::Ready],
                Err(e) => vec![AppEvent::Error(e.to_string())],
            },
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
                    storage_used: 0,
                    storage_quota: 0,
                    avatar_url: None,
                };
                self.accounts.add_account(a.clone());
                vec![AppEvent::AccountUpdated(a)]
            }
            AppCommand::UpdateSettings(settings) => {
                self.settings = settings;
                vec![AppEvent::Ready]
            }
            AppCommand::OpenFolder | AppCommand::Quit => vec![AppEvent::Ready],
        }
    }

    fn update_acct(&mut self, id: Uuid, f: impl FnOnce(&mut Account)) -> Vec<AppEvent> {
        match self.accounts.get_account_mut(id) {
            Ok(a) => {
                f(a);
                vec![AppEvent::AccountUpdated(a.clone())]
            }
            Err(_) => vec![AppEvent::Error("account not found".into())],
        }
    }
}
