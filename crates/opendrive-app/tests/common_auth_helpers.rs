#![allow(dead_code)]
//! Shared mock HTTP servers, FakeDrive setup, ephemeral PKCS8 key, and egui test helpers.

use eframe::egui::{self, Pos2, RawInput, Rect, Vec2};
use opendrive_app::desktop::ControlCenterApp;
use opendrive_app::runtime::auth_runner::AuthEndpoints;
use opendrive_app::runtime::mount_manager::MountBackend;
use opendrive_auth::{AuthError, Result as AuthResult, SecretStore};
use opendrive_core::Account;
use opendrive_fake_drive::{FakeAbout, FakeDrive, FakeDriveServer};
use opendrive_sync::VfsEngine;
use std::any::Any;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use url::Url;
use uuid::Uuid;

#[derive(Default)]
pub struct RecordingBackend {
    pub prepared: Mutex<Vec<PathBuf>>,
    pub connected: Mutex<Vec<(Uuid, PathBuf, u64)>>,
    pub unregistered: Mutex<Vec<PathBuf>>,
    pub virtual_drives: Mutex<Vec<(bool, String, PathBuf)>>,
}

impl MountBackend for RecordingBackend {
    fn prepare_root(&self, path: &Path) -> Result<(), String> {
        self.prepared.lock().unwrap().push(path.to_path_buf());
        Ok(())
    }

    fn connect_account(
        &self,
        acct: &Account,
        path: &Path,
        root_node: u64,
        _engine: Arc<dyn VfsEngine>,
        _rt: &tokio::runtime::Handle,
    ) -> Result<Box<dyn Any + Send + Sync>, String> {
        self.connected
            .lock()
            .unwrap()
            .push((acct.id, path.to_path_buf(), root_node));
        Ok(Box::new(()))
    }

    fn unregister_account(&self, path: &Path) {
        self.unregistered.lock().unwrap().push(path.to_path_buf());
    }

    fn set_virtual_drive(&self, enabled: bool, letter: &str, root: &Path) -> Result<(), String> {
        self.virtual_drives
            .lock()
            .unwrap()
            .push((enabled, letter.to_string(), root.to_path_buf()));
        opendrive_platform::normalize_drive_letter(letter)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

const TEST_PKCS8_B64: &str = concat!(
    "MIIEvwIBADANBgkqhkiG9w0BAQEFAASCBKkwggSlAgEAAoIBAQC4b7NqObarTaXeuE995eVK+sWHSZmmeZR/",
    "Wx1KxMCRno8rFVlebtu6+V9tDlyQkxrmOHpqV+hx5hd/0jDVz5EwN4YEowZrHDZcXHEsx99mypGLnpRH7OPg",
    "QXZmz6kjCHuWzacp+I5sfU+SVS/Y4uPyTfFaDpyvBmj8u+3zA7ARGmtlmviaFch2KiPsPZtTzbGgAGlU8Bt1",
    "Ol9sKcWgNZ5LLMbOObDvKDytzzmzHpNL6AWWghL5eBREXVBRIH/uBxc7uIX1gDmQGmFvhaB9XXoQccpipc+B",
    "/OHpk3//fKsLlUwmUetbD7a0EDL9UkO4Y4oztdHrLPTQVyfEv+InE6JBAgMBAAECggEAGz75GLSs7RJ78fJ1",
    "X8ph/JkeLBWt2p90VfoOUjjNn9V2He259Y7X6/+h3rjz6LGqlB78yo/w+T598zaIW1WeA6r4I+A3dwREbZGE",
    "cJKYSJWdOax0CEoRAdR52sdLm67JnyYDT1p2I8XBRwNJnbcq6GUmReEJuXtvcAKNLMy7KlNlnd5auUh1iFSi",
    "cjCefrSVRuxGzFWiezoiT1Dkuf0WKd88IJ0IRBn8GnRE8L9wRf4gQAX3/6W6384FlklpjwzU4GRJQpEERk1M1",
    "EzeJvcsyoNbvPFTLN1/GToe7b1+duvhZYQufN6TwitCX6BXoZrvvEpWI8UFFtpWna9CtUZHSQKBgQD2KjGwVW",
    "47MZW6NJPZGtZsPtuDWVz8kvEE4O6HjdVTs/6Vv8k0DAVHV2oC/mVO44R4CTdzvdq6OXdXSMOHNfukCdsw4H",
    "QzBOYKGgayIhaHxtb51sE0Img5p7GFNLLoh3V/SJM0J7QlLU2ZUatrWvfVTVP00fCKxGCwad56pgcjBQKBgQ",
    "C/ziPDa0S6opBmT5/ncc0ix5suf1ng0IcOOEbjnb6erYN3uiaVytD7bxvkbj7JVZ4aTLokLlxvg0CvgIQ3QX",
    "RVuBSvX3lykJwi76YG8GA+rIwlU0q11kMLXrZOZ++iAfd3r3gWpG+btp669tj8aDGyWwDtsDCw3L0qtjOrNV",
    "hfDQKBgQCra/qqNvPKWN9RiiejOmMzghpPP73XTWR150V0NDroTSHuml281toeMitq5kkaRKoNBS4OdZmJsV",
    "b/+3XeBKO2Of7f7I4+sRyY0EL1bXOPHdK6rOhiK8u0yqh+gJJOB3a1Ko8H9NYoRYISUKM04WZQrcbXaBNALz",
    "y93RvHATNbGQKBgQCdQYGI4xXCfy4s8/OCaodpkIXmpXsnG1Dfk7xrOhXjOxeivfiMr3CP9RTFQK4xnAS4Q1",
    "gJ+gjHVLFgMto0A3JCJV6wOnL0PyQEcL+KyOHgANAWeiey1iy50blep1CNX1dpxW+MAN2QqTWSzHgy2aqFNJ",
    "+aQO5q01A1CJVsNZxysQKBgQDLluomtOX0NlYXU96V0xE0a8e4WaeNoOjLfU/MBIIGzIiGSNjnM1X+CrDvWw",
    "fQhQaYqCiOqI6OvRm2sM0RqWa9579yOrtVMJxh7pVPzSaIZVaUBWDFEECzg+OUaB+UAiMujYedi01ZL9L3ci",
    "NIAT3Bfn/gIPBCddFfVTXu0ZZAwA=="
);

pub fn sample_service_account_json(token_uri: &str) -> String {
    let pem = format!(
        "-----{}-----\n{TEST_PKCS8_B64}\n-----{}-----\n",
        "BEGIN PRIVATE KEY", "END PRIVATE KEY"
    );
    serde_json::json!({
        "type": "service_account",
        "client_email": "studio-bot@proj.iam.gserviceaccount.com",
        "private_key": pem,
        "token_uri": token_uri,
        "project_id": "proj"
    })
    .to_string()
}

pub async fn mock_token_server(json_body: &'static str) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        while let Ok((mut s, _)) = listener.accept().await {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf).await;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                json_body.len(),
                json_body
            );
            let _ = s.write_all(resp.as_bytes()).await;
        }
    });
    (format!("http://{addr}/token"), handle)
}

pub async fn hanging_token_server() -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((s, _)) = listener.accept().await {
            held.push(s);
        }
    });
    (format!("http://{addr}/token"), handle)
}

pub async fn start_fake_studio_drive() -> (String, String, JoinHandle<()>) {
    let drive = Arc::new(FakeDrive::new());
    drive.set_about(FakeAbout {
        email: "studio@example.com".into(),
        display_name: "Studio Drive".into(),
        ..Default::default()
    });
    let (addr, handle) = FakeDriveServer::start(drive).await;
    (
        format!("http://{addr}/drive/v3"),
        format!("http://{addr}/upload/drive/v3"),
        handle,
    )
}

pub fn capturing_endpoints(
    token_url: String,
    api_base: String,
    upload_base: String,
    opened: Arc<Mutex<Vec<String>>>,
) -> AuthEndpoints {
    AuthEndpoints {
        token_url,
        drive_api_base: api_base,
        drive_upload_base: upload_base,
        browser_opener: Arc::new(move |url| {
            opened.lock().unwrap().push(url.to_string());
            Ok(())
        }),
        ..Default::default()
    }
}

pub fn send_oauth_callback(auth_url: &str) {
    let parsed = Url::parse(auth_url).unwrap();
    let query_val = |key: &str| {
        parsed
            .query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
            .unwrap()
    };
    let redir = Url::parse(&query_val("redirect_uri")).unwrap();
    let state = query_val("state");
    let addr = format!("{}:{}", redir.host_str().unwrap(), redir.port().unwrap());
    let mut stream = TcpStream::connect(addr).unwrap();
    let req = format!(
        "GET {}?code=ok_code&state={state} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        redir.path()
    );
    stream.write_all(req.as_bytes()).unwrap();
    let mut resp = String::new();
    let _ = stream.read_to_string(&mut resp);
}

pub fn poll_until(app: &mut ControlCenterApp, pred: impl Fn(&ControlCenterApp) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        app.poll_auth();
        if pred(app) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "timed out polling auth (form_error={:?}, notice={:?})",
        app.form_error, app.notice
    );
}

#[derive(Debug)]
pub struct MockDroppedFile {
    pub path: PathBuf,
    pub bytes: Result<Vec<u8>, String>,
}

impl egui::DroppedFile for MockDroppedFile {
    fn path(&self) -> &Path {
        &self.path
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        self.bytes.clone()
    }
}

pub fn run_frame(
    ctx: &egui::Context,
    app: &mut ControlCenterApp,
    dropped_files: Vec<egui::DroppedFileHandle>,
) -> egui::FullOutput {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 760.0))),
        dropped_files,
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| app.render_root(ui));
    out.textures_delta.clear();
    out
}

pub fn has_copy_text(out: &egui::FullOutput, expected: &str) -> bool {
    out.platform_output
        .commands
        .iter()
        .any(|cmd| matches!(cmd, egui::OutputCommand::CopyText(text) if text == expected))
}

pub struct FailingSecretStore;

impl SecretStore for FailingSecretStore {
    fn store_refresh_token(&self, _: Uuid, _: &str) -> AuthResult<()> {
        Err(AuthError::Keychain("locked".into()))
    }
    fn load_refresh_token(&self, _: Uuid) -> AuthResult<Option<String>> {
        Err(AuthError::Keychain("locked".into()))
    }
    fn delete_refresh_token(&self, _: Uuid) -> AuthResult<()> {
        Ok(())
    }
    fn store_client_secret(&self, _: Uuid, _: &str) -> AuthResult<()> {
        Err(AuthError::Keychain("keyring locked".into()))
    }
    fn load_client_secret(&self, _: Uuid) -> AuthResult<Option<String>> {
        Err(AuthError::Keychain("keyring read failed".into()))
    }
    fn store_service_account_json(&self, _: Uuid, _: &str) -> AuthResult<()> {
        Err(AuthError::Keychain("locked".into()))
    }
    fn load_service_account_json(&self, _: Uuid) -> AuthResult<Option<String>> {
        Ok(None)
    }
    fn delete_service_account_json(&self, _: Uuid) -> AuthResult<()> {
        Ok(())
    }
}
