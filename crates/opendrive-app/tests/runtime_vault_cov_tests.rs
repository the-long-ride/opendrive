//! Coverage tests for bridge_vault and release_check.

mod common_auth_helpers;

use common_auth_helpers::FailingSecretStore;
use opendrive_app::runtime::bridge::Bridge;
use opendrive_app::runtime::release_check::{
    fetch_latest_release, is_newer_release, parse_semver, spawn_release_check,
};
use opendrive_app::runtime::ui::bridge_from_controller;
use opendrive_auth::{encrypt_vault, ExportedAccount, ExportedCredentialSet, VaultPayload};
use opendrive_core::{Account, AccountStatus, AppController, CredentialSet, Settings};
use opendrive_store::Store;
use opendrive_sync::SyncEngine;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

fn mk_acct(id: Uuid, cid: Uuid, name: &str) -> Account {
    Account {
        id,
        nickname: name.into(),
        email: format!("{name}@x.com"),
        credential_set_id: cid,
        folder_name: name.into(),
        read_only: false,
        enabled: true,
        status: AccountStatus::Connected,
        storage_used: 0,
        storage_quota: 0,
        avatar_url: None,
    }
}

#[test]
fn vault_and_rename_edge_and_error_branches() {
    let (br, _) = Bridge::new(Settings::default());
    assert!(br.rename_account("not-a-uuid", "N".into(), None).is_err());
    assert!(br
        .rename_account(&Uuid::new_v4().to_string(), "N".into(), None)
        .is_err());

    let snap = br
        .add_account("Alpha".into(), "a@x.com".into(), "Alpha".into(), false)
        .unwrap();
    let aid = snap.accounts[0].id.to_string();
    let (st1, chg1) = br.rename_account(&aid, "Alpha2".into(), None).unwrap();
    assert!(!chg1 && st1.accounts[0].nickname == "Alpha2");
    assert!(
        br.rename_account(&aid, "Alpha2".into(), Some("AlphaDir".into()))
            .unwrap()
            .1
    );

    br.controller_handle()
        .lock()
        .unwrap()
        .accounts
        .add_credential(CredentialSet {
            id: Uuid::new_v4(),
            label: "Empty".into(),
            client_id: "cid-empty".into(),
        });
    let bytes = br.export_encrypted_vault("VaultPass#2026!").unwrap();
    assert_eq!(
        br.import_encrypted_vault(&bytes, "VaultPass#2026!")
            .unwrap()
            .1,
        1
    );

    let payload = VaultPayload {
        exported_at: "unix:1".into(),
        credentials: vec![ExportedCredentialSet {
            label: "Skip".into(),
            client_id: "  ".into(),
            client_secret: "s".into(),
        }],
        accounts: vec![ExportedAccount {
            nickname: "Alpha2".into(),
            email: "a@x.com".into(),
            folder_name: "/".into(),
            read_only: false,
            enabled: false,
            credential_client_id: None,
            refresh_token: None,
            service_account_json: None,
            avatar_url: Some("https://example.com/av.png".into()),
        }],
    };
    let custom_bytes = encrypt_vault(&payload, "VaultPass#2026!").unwrap();
    let (st_imp, _) = br
        .import_encrypted_vault(&custom_bytes, "VaultPass#2026!")
        .unwrap();
    assert_eq!(st_imp.accounts[0].status, AccountStatus::Paused);
    assert_eq!(
        st_imp.accounts[0].avatar_url.as_deref(),
        Some("https://example.com/av.png")
    );

    let (fail_br, _) = Bridge::new_with_secrets(
        Settings::default(),
        Arc::new(SyncEngine::new(Store::open_in_memory().unwrap())),
        None,
        Arc::new(FailingSecretStore),
    );
    fail_br
        .controller_handle()
        .lock()
        .unwrap()
        .accounts
        .add_credential(CredentialSet {
            id: Uuid::new_v4(),
            label: "L".into(),
            client_id: "c".into(),
        });
    assert!(fail_br.export_encrypted_vault("VaultPass#2026!").is_err());

    let p2 = VaultPayload {
        exported_at: "unix:2".into(),
        credentials: vec![ExportedCredentialSet {
            label: "L".into(),
            client_id: "c1".into(),
            client_secret: "s1".into(),
        }],
        accounts: vec![ExportedAccount {
            nickname: "U".into(),
            email: "u@x.com".into(),
            folder_name: "U".into(),
            read_only: false,
            enabled: true,
            credential_client_id: Some("c1".into()),
            refresh_token: Some("rt".into()),
            service_account_json: Some("{}".into()),
            avatar_url: None,
        }],
    };
    let b2 = encrypt_vault(&p2, "VaultPass#2026!").unwrap();
    assert!(fail_br
        .import_encrypted_vault(&b2, "VaultPass#2026!")
        .is_err());

    let tmp_file = std::env::temp_dir().join(format!("od-vcfg-{}", Uuid::new_v4()));
    std::fs::write(&tmp_file, b"x").unwrap();
    let (fbr, _) = Bridge::new_with_engine(
        Settings::default(),
        Arc::new(SyncEngine::new(Store::open_in_memory().unwrap())),
        Some(tmp_file.clone()),
    );
    fbr.controller_handle()
        .lock()
        .unwrap()
        .accounts
        .add_account(mk_acct(Uuid::new_v4(), Uuid::nil(), "F"));
    let fid = fbr.state_snapshot().accounts[0].id.to_string();
    assert!(fbr.rename_account(&fid, "F2".into(), None).is_err());
    assert!(fbr.update_release_prefs(true, Some("v1".into())).is_err());
    assert!(fbr.import_encrypted_vault(&b2, "VaultPass#2026!").is_err());
    let _ = std::fs::remove_file(&tmp_file);
}

#[test]
fn release_check_semver_and_fetch_paths() {
    assert_eq!(parse_semver("v1.2.3+build.9"), Some((1, 2, 3)));
    for bad in ["", "1.2", "1.2.3.4", "1.x.3", "1.2.y"] {
        assert_eq!(parse_semver(bad), None);
    }
    assert!(!is_newer_release("bad", "1.0.0"));
    assert!(!is_newer_release("1.0.0", "bad"));

    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(TcpListener::bind("127.0.0.1:0")).unwrap();
    let addr = listener.local_addr().unwrap();
    let h = rt.spawn(async move {
        let responses = [
            ("200 OK", r#"{"tag_name":"v9.0.0","name":"v9","html_url":"https://x/9","body":"hi"}"#),
            ("200 OK", r#"{"tag_name":"v0.1.0","name":"v0.1.0","html_url":"https://x/1","body":""}"#),
            ("500 Internal Server Error", "{}"),
            ("200 OK", "not-json"),
        ];
        for (st, body) in responses {
            if let Ok((mut s, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = s.read(&mut buf).await;
                let msg = format!("HTTP/1.1 {st}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = s.write_all(msg.as_bytes()).await;
            }
        }
    });
    let url = format!("http://{addr}/rel");
    assert_eq!(
        fetch_latest_release(&url, "0.1.0")
            .unwrap()
            .unwrap()
            .tag_name,
        "v9.0.0"
    );
    assert!(fetch_latest_release(&url, "0.1.0").unwrap().is_none());
    assert!(fetch_latest_release(&url, "0.1.0")
        .unwrap_err()
        .contains("HTTP 500"));
    assert!(fetch_latest_release(&url, "0.1.0").is_err());
    h.abort();
    assert!(fetch_latest_release(&url, "0.1.0").is_err());
    let handle = spawn_release_check("http://127.0.0.1:1/bad".into(), "0.1.0".into(), true, None);
    assert!(handle.manual);
    assert!(handle.rx.recv().unwrap().is_err());
}

#[test]
fn credentials_lookup_sibling_sa_and_error_fallbacks() {
    let (c1, c2) = (Uuid::new_v4(), Uuid::new_v4());
    let (sa_id, oauth_id) = (Uuid::new_v4(), Uuid::new_v4());
    let mut ctrl = AppController::new(Settings::default());
    ctrl.accounts.add_credential(CredentialSet {
        id: c1,
        label: "A".into(),
        client_id: "cid-shared".into(),
    });
    ctrl.accounts.add_credential(CredentialSet {
        id: c2,
        label: "B".into(),
        client_id: "cid-shared".into(),
    });
    ctrl.accounts.add_account(mk_acct(sa_id, Uuid::nil(), "SA"));
    ctrl.accounts.add_account(mk_acct(oauth_id, c2, "OA"));

    let br = bridge_from_controller(ctrl, None, Store::open_in_memory().unwrap()).unwrap();
    br.secrets().store_client_secret(c1, "sib-sec").unwrap();
    assert_eq!(
        br.credentials_for_account(Some(sa_id)),
        (String::new(), String::new())
    );
    assert_eq!(
        br.credentials_for_account(Some(oauth_id)),
        ("cid-shared".into(), "sib-sec".into())
    );
    assert_eq!(
        br.credentials_for_account(Some(Uuid::new_v4())),
        ("cid-shared".into(), "sib-sec".into())
    );
    assert_eq!(br.load_service_account_json(sa_id), None);

    let (fail_br, _) = Bridge::new_with_secrets(
        Settings::default(),
        Arc::new(SyncEngine::new(Store::open_in_memory().unwrap())),
        None,
        Arc::new(FailingSecretStore),
    );
    fail_br.save_oauth_client_credentials("x", "y").unwrap_err();
    assert_eq!(
        fail_br.saved_oauth_credentials(),
        ("x".into(), String::new())
    );
    assert_eq!(fail_br.load_service_account_json(sa_id), None);

    let tmp = std::env::temp_dir().join(format!("od-cfgfile-{}", Uuid::new_v4()));
    std::fs::write(&tmp, b"not-a-dir").unwrap();
    let (file_br, _) = Bridge::new_with_engine(
        Settings::default(),
        Arc::new(SyncEngine::new(Store::open_in_memory().unwrap())),
        Some(tmp.clone()),
    );
    assert!(file_br.save_oauth_client_credentials("cid", "sec").is_err());
    let _ = std::fs::remove_file(&tmp);
}
