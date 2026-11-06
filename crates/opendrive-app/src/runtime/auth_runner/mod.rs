//! Background OAuth loopback + PKCE and Service Account JWT authentication runners.

mod complete;

use crate::design::formatters::ConnectAuthMode;
use crate::runtime::bridge::{AppStateResponse, Bridge};
use crate::runtime::release_check::GITHUB_LATEST_RELEASE_URL;
use eframe::egui;
use opendrive_auth::{
    bind_loopback, generate_pkce, random_state, wait_for_redirect, AuthClient, ServiceAccountKey,
    AUTH_URL, DEFAULT_TOKEN_URI, TOKEN_URL,
};
use opendrive_drive::DriveClient;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use tokio::sync::oneshot;
use uuid::Uuid;

pub(crate) const GOOGLE_DRIVE_API_BASE: &str = "https://www.googleapis.com/drive/v3";
pub(crate) const GOOGLE_DRIVE_UPLOAD_BASE: &str = "https://www.googleapis.com/upload/drive/v3";

pub type BrowserOpener = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;
pub type VaultFilePicker = Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>;

#[derive(Clone)]
pub struct AuthEndpoints {
    pub auth_url: String,
    pub token_url: String,
    pub drive_api_base: String,
    pub drive_upload_base: String,
    pub redirect_timeout_secs: u64,
    pub browser_opener: BrowserOpener,
    pub folder_opener: BrowserOpener,
    pub vault_save_picker: VaultFilePicker,
    pub vault_open_picker: VaultFilePicker,
    pub release_api_url: String,
}

impl Default for AuthEndpoints {
    fn default() -> Self {
        Self {
            auth_url: AUTH_URL.to_string(),
            token_url: TOKEN_URL.to_string(),
            drive_api_base: GOOGLE_DRIVE_API_BASE.to_string(),
            drive_upload_base: GOOGLE_DRIVE_UPLOAD_BASE.to_string(),
            redirect_timeout_secs: 180,
            browser_opener: Arc::new(|url| {
                opendrive_platform::open_path(url).map_err(|e| e.to_string())
            }),
            folder_opener: Arc::new(|path| {
                opendrive_platform::open_path(path).map_err(|e| e.to_string())
            }),
            vault_save_picker: Arc::new(|| {
                rfd::FileDialog::new()
                    .add_filter("OpenDrive Encrypted Vault", &["odcred"])
                    .set_file_name("opendrive-credentials.odcred")
                    .save_file()
            }),
            vault_open_picker: Arc::new(|| {
                rfd::FileDialog::new()
                    .add_filter("OpenDrive Encrypted Vault", &["odcred"])
                    .pick_file()
            }),
            release_api_url: GITHUB_LATEST_RELEASE_URL.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OAuthStartParams {
    pub target_account_id: Option<Uuid>,
    pub client_id: String,
    pub client_secret: String,
    pub nickname: String,
    pub folder_name: String,
    pub read_only: bool,
    pub open_browser: bool,
    pub copy_link: bool,
}

#[derive(Debug, Clone)]
pub struct ServiceAccountParams {
    pub target_account_id: Option<Uuid>,
    pub json_or_path: String,
    pub nickname: String,
    pub folder_name: String,
    pub read_only: bool,
}

#[derive(Debug, Clone)]
pub enum AuthEvent {
    BrowserReady {
        auth_url: String,
        copy_to_clipboard: bool,
    },
    BrowserOpenWarning(String),
    Completed {
        state: Box<AppStateResponse>,
        message: String,
    },
    Failed(String),
}

pub struct AuthSession {
    pub mode: ConnectAuthMode,
    pub target_account: Option<Uuid>,
    pub auth_url: Option<String>,
    rx: mpsc::Receiver<AuthEvent>,
    cancel_tx: Option<oneshot::Sender<()>>,
}

impl AuthSession {
    pub fn try_recv(&mut self) -> Option<AuthEvent> {
        self.rx.try_recv().ok()
    }

    pub fn cancel(&mut self) {
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }
    }
}

fn notify(tx: &mpsc::Sender<AuthEvent>, event: AuthEvent, repaint_ctx: &Option<egui::Context>) {
    let _ = tx.send(event);
    if let Some(ctx) = repaint_ctx {
        ctx.request_repaint();
    }
}

fn spawn_auth_worker<F, Fut>(
    mode: ConnectAuthMode,
    target_account: Option<Uuid>,
    repaint_ctx: Option<egui::Context>,
    work: F,
) -> AuthSession
where
    F: FnOnce(mpsc::Sender<AuthEvent>, Option<egui::Context>, oneshot::Receiver<()>) -> Fut
        + Send
        + 'static,
    Fut: Future<Output = Result<Option<(AppStateResponse, String)>, String>>,
{
    let (tx, rx) = mpsc::channel();
    let (cancel_tx, cancel_rx) = oneshot::channel();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                notify(&tx, AuthEvent::Failed(e.to_string()), &repaint_ctx);
                return;
            }
        };
        match rt.block_on(work(tx.clone(), repaint_ctx.clone(), cancel_rx)) {
            Ok(Some((state, message))) => {
                notify(
                    &tx,
                    AuthEvent::Completed {
                        state: Box::new(state),
                        message,
                    },
                    &repaint_ctx,
                );
            }
            Ok(None) => {}
            Err(err) => notify(&tx, AuthEvent::Failed(err), &repaint_ctx),
        }
    });
    AuthSession {
        mode,
        target_account,
        auth_url: None,
        rx,
        cancel_tx: Some(cancel_tx),
    }
}

async fn run_browser_oauth(
    bridge: Bridge,
    endpoints: AuthEndpoints,
    params: OAuthStartParams,
    tx: mpsc::Sender<AuthEvent>,
    repaint_ctx: Option<egui::Context>,
    mut cancel_rx: oneshot::Receiver<()>,
) -> Result<Option<(AppStateResponse, String)>, String> {
    let pkce = generate_pkce().map_err(|e| e.to_string())?;
    let state = random_state();
    let (listener, redirect_uri, _) = bind_loopback().await.map_err(|e| e.to_string())?;
    let auth = AuthClient::with_endpoints(
        params.client_id.trim().to_string(),
        params.client_secret.trim().to_string(),
        &endpoints.auth_url,
        &endpoints.token_url,
    );
    let auth_url = auth.authorization_url(&redirect_uri, &pkce.challenge, &state, params.read_only);
    notify(
        &tx,
        AuthEvent::BrowserReady {
            auth_url: auth_url.clone(),
            copy_to_clipboard: params.copy_link,
        },
        &repaint_ctx,
    );
    if params.open_browser {
        if let Err(err) = (endpoints.browser_opener)(&auth_url) {
            notify(&tx, AuthEvent::BrowserOpenWarning(err), &repaint_ctx);
        }
    }
    let redirect = tokio::select! {
        _ = &mut cancel_rx => return Ok(None),
        res = wait_for_redirect(listener, &state, endpoints.redirect_timeout_secs) => {
            res.map_err(|e| e.to_string())?
        }
    };
    let (access, refresh) = tokio::select! {
        _ = &mut cancel_rx => return Ok(None),
        res = auth.exchange_code(&redirect.code, &pkce.verifier, &redirect_uri) => {
            res.map_err(|e| e.to_string())?
        }
    };
    let drive = DriveClient::with_base_urls(
        access,
        &endpoints.drive_api_base,
        &endpoints.drive_upload_base,
    );
    let about = tokio::select! {
        _ = &mut cancel_rx => return Ok(None),
        res = drive.about() => res.map_err(|e| e.to_string())?,
    };
    let (acct_id, root_id, message) =
        bridge.complete_oauth_account(&params, &refresh, &about, drive)?;
    let _ = bridge.engine().hydrate_folder(acct_id, root_id).await;
    Ok(Some((bridge.state_snapshot(), message)))
}

async fn run_service_account(
    bridge: Bridge,
    endpoints: AuthEndpoints,
    params: ServiceAccountParams,
    mut cancel_rx: oneshot::Receiver<()>,
) -> Result<Option<(AppStateResponse, String)>, String> {
    let (mut key, raw_json) =
        ServiceAccountKey::from_json_or_path(&params.json_or_path).map_err(|e| e.to_string())?;
    if key.token_uri == DEFAULT_TOKEN_URI {
        key.token_uri = endpoints.token_url.clone();
    }
    let access = tokio::select! {
        _ = &mut cancel_rx => return Ok(None),
        res = key.fetch_access_token(params.read_only) => res.map_err(|e| e.to_string())?,
    };
    let drive = DriveClient::with_base_urls(
        access,
        &endpoints.drive_api_base,
        &endpoints.drive_upload_base,
    );
    let about = tokio::select! {
        _ = &mut cancel_rx => return Ok(None),
        res = drive.about() => res.map_err(|e| e.to_string())?,
    };
    let (acct_id, root_id, message) =
        bridge.complete_service_account(&params, &raw_json, &about, drive)?;
    let _ = bridge.engine().hydrate_folder(acct_id, root_id).await;
    Ok(Some((bridge.state_snapshot(), message)))
}

pub fn start_browser_oauth(
    bridge: Bridge,
    endpoints: AuthEndpoints,
    params: OAuthStartParams,
    repaint_ctx: Option<egui::Context>,
) -> AuthSession {
    spawn_auth_worker(
        ConnectAuthMode::BrowserOAuth,
        params.target_account_id,
        repaint_ctx,
        move |tx, ctx, cancel| run_browser_oauth(bridge, endpoints, params, tx, ctx, cancel),
    )
}

pub fn start_service_account_auth(
    bridge: Bridge,
    endpoints: AuthEndpoints,
    params: ServiceAccountParams,
    repaint_ctx: Option<egui::Context>,
) -> AuthSession {
    spawn_auth_worker(
        ConnectAuthMode::ServiceAccount,
        params.target_account_id,
        repaint_ctx,
        move |_, _, cancel| run_service_account(bridge, endpoints, params, cancel),
    )
}
