//! Credential, token, private-key, and password masking helpers for OpenDrive logs.

pub fn mask_sensitive_text(input: &str) -> String {
    let mut out = redact_pem_blocks(input);
    for prefix in ["GOCSPX-", "ya29.", "1//", "Bearer ", "bearer ", "BEARER "] {
        out = redact_prefixed_tokens(&out, prefix);
    }
    out = redact_google_client_id(&out);
    for key in [
        "client_secret",
        "refresh_token",
        "access_token",
        "private_key_id",
        "private_key",
        "password",
        "passphrase",
        "secret",
        "token",
        "authorization",
        "code",
    ] {
        out = redact_kv_key(&out, key);
    }
    out
}

fn redact_pem_blocks(input: &str) -> String {
    let mut cur = input.to_string();
    loop {
        let lower = cur.to_ascii_lowercase();
        let Some(start) = lower.find("-----begin ") else {
            break;
        };
        let end = lower[start..]
            .find("-----end ")
            .and_then(|rel| {
                let tail = &lower[start + rel + 9..];
                tail.find("-----").map(|k| start + rel + 9 + k + 5)
            })
            .unwrap_or(cur.len());
        cur.replace_range(start..end, "[REDACTED PRIVATE KEY]");
    }
    cur
}

fn redact_prefixed_tokens(input: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(pos) = rest.find(prefix) {
        out.push_str(&rest[..pos]);
        out.push_str(if prefix.eq_ignore_ascii_case("bearer ") {
            "Bearer "
        } else {
            prefix
        });
        out.push_str("***");
        let after = &rest[pos + prefix.len()..];
        let skip = after
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || b"-._~+/=".contains(b))
            .count();
        rest = &after[skip..];
    }
    out.push_str(rest);
    out
}

fn redact_google_client_id(input: &str) -> String {
    let suffix = ".apps.googleusercontent.com";
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(pos) = rest.find(suffix) {
        let head = &rest[..pos];
        let id_len = head
            .bytes()
            .rev()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_')
            .count();
        let keep = head.len() - id_len;
        out.push_str(&head[..keep]);
        out.push_str("***");
        out.push_str(suffix);
        rest = &rest[pos + suffix.len()..];
    }
    out.push_str(rest);
    out
}

fn redact_kv_key(input: &str, key: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut idx = 0usize;
    let bytes = input.as_bytes();
    while idx < input.len() {
        let lower_rest = input[idx..].to_ascii_lowercase();
        let Some(rel) = lower_rest.find(key) else {
            out.push_str(&input[idx..]);
            break;
        };
        let abs = idx + rel;
        let prev_ok = abs == 0 || !is_ident_byte(bytes[abs - 1]);
        let after_key = abs + key.len();
        if !prev_ok || after_key >= input.len() {
            out.push_str(&input[idx..after_key]);
            idx = after_key;
            continue;
        }
        let mut sep = after_key;
        if bytes[sep] == b'"' || bytes[sep] == b'\'' {
            sep += 1;
        }
        while sep < input.len() && bytes[sep].is_ascii_whitespace() {
            sep += 1;
        }
        if sep >= input.len() || (bytes[sep] != b'=' && bytes[sep] != b':') {
            out.push_str(&input[idx..after_key]);
            idx = after_key;
            continue;
        }
        sep += 1;
        while sep < input.len() && bytes[sep].is_ascii_whitespace() {
            sep += 1;
        }
        let quoted = sep < input.len() && (bytes[sep] == b'"' || bytes[sep] == b'\'');
        let quote_ch = if quoted { bytes[sep] } else { 0 };
        let val_start = if quoted { sep + 1 } else { sep };
        let val_end = if quoted {
            let mut p = val_start;
            while p < bytes.len() {
                if bytes[p] == b'\\' && p + 1 < bytes.len() {
                    p += 2;
                    continue;
                }
                if bytes[p] == quote_ch {
                    break;
                }
                p += 1;
            }
            p
        } else {
            val_start
                + input[val_start..]
                    .bytes()
                    .take_while(|b| !b.is_ascii_whitespace() && !b"&,;)}]\"".contains(b))
                    .count()
        };
        let val = &input[val_start..val_end];
        let keep_http = key == "code" && val.len() == 3 && val.bytes().all(|b| b.is_ascii_digit());
        if val.starts_with("[REDACTED") || keep_http {
            out.push_str(&input[idx..val_end]);
        } else {
            out.push_str(&input[idx..val_start]);
            out.push_str("***");
        }
        idx = val_end;
    }
    out
}

const fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
