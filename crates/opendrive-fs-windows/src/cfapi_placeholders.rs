//! Windows filename sanitization, deduplication, and CFAPI placeholder reconciliation helpers.

use opendrive_sync::NodeId;
use std::collections::HashSet;
#[cfg(windows)]
use std::path::{Path, PathBuf};

fn is_windows_reserved_stem(upper: &str) -> bool {
    matches!(upper, "CON" | "PRN" | "AUX" | "NUL")
        || ((upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.len() == 4
            && upper.as_bytes()[3].is_ascii_digit()
            && upper.as_bytes()[3] != b'0')
}

pub fn sanitize_windows_entry_name(raw: &str) -> String {
    let replaced: String = raw
        .chars()
        .map(|c| {
            if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = replaced.trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        return "unnamed".to_string();
    }
    let stem = trimmed.split('.').next().unwrap_or(trimmed);
    if is_windows_reserved_stem(&stem.to_ascii_uppercase()) {
        format!("_{trimmed}")
    } else {
        trimmed.to_string()
    }
}

fn with_collision_suffix(base: &str, is_dir: bool, n: usize) -> String {
    if !is_dir {
        if let Some((stem, ext)) = base.rsplit_once('.') {
            if !stem.is_empty() && !ext.is_empty() {
                return format!("{stem} ({n}).{ext}");
            }
        }
    }
    format!("{base} ({n})")
}

pub fn deduplicate_windows_names(
    entries: Vec<(String, NodeId, bool, u64)>,
) -> Vec<(String, NodeId, bool, u64)> {
    let mut seen = HashSet::with_capacity(entries.len());
    let mut out = Vec::with_capacity(entries.len());
    for (raw_name, id, is_dir, size) in entries {
        let base = sanitize_windows_entry_name(&raw_name);
        let mut candidate = base.clone();
        let mut n = 2usize;
        while !seen.insert(candidate.to_ascii_lowercase()) {
            candidate = with_collision_suffix(&base, is_dir, n);
            n += 1;
        }
        out.push((candidate, id, is_dir, size));
    }
    out
}

#[cfg(windows)]
fn open_writable_placeholder(path: &Path) -> Option<cloud_filter::placeholder::Placeholder> {
    use cloud_filter::placeholder::Placeholder;
    Placeholder::options()
        .write_access()
        .exclusive()
        .open(path)
        .or_else(|_| Placeholder::options().write_access().open(path))
        .ok()
}

#[cfg(windows)]
pub fn ensure_existing_placeholder(
    path: &Path,
    node_id: NodeId,
    is_dir: bool,
    enable_on_demand: bool,
) {
    use cloud_filter::placeholder::{ConvertOptions, UpdateOptions};
    let Some(mut ph) = open_writable_placeholder(path) else {
        return;
    };
    let blob = node_id.to_le_bytes();
    if matches!(ph.info(), Ok(Some(_))) {
        let mut opts = UpdateOptions::default().blob(&blob).mark_in_sync();
        if is_dir && enable_on_demand {
            opts = opts.has_children();
        }
        if let Err(err) = ph.update(opts, None) {
            tracing::warn!(?err, path = %path.display(), "cfapi placeholder update failed");
        }
    } else {
        let mut opts = ConvertOptions::default().blob(blob.to_vec()).mark_in_sync();
        if is_dir && enable_on_demand {
            opts = opts.has_children();
        }
        if let Err(err) = ph.convert_to_placeholder(opts, None) {
            tracing::warn!(?err, path = %path.display(), "cfapi convert_to_placeholder failed");
        }
    }
}

#[cfg(windows)]
pub fn reconcile_placeholder_dirs(root_path: &Path, rel_dirs: &[PathBuf]) {
    use cloud_filter::placeholder::UpdateOptions;
    let opts = UpdateOptions::default().has_children().mark_in_sync();
    for dir in
        std::iter::once(root_path.to_path_buf()).chain(rel_dirs.iter().map(|r| root_path.join(r)))
    {
        if dir.is_dir() {
            if let Some(mut ph) = open_writable_placeholder(&dir) {
                if matches!(ph.info(), Ok(Some(_))) {
                    if let Err(err) = ph.update(opts.clone(), None) {
                        tracing::warn!(?err, path = %dir.display(), "cfapi placeholder dir update failed");
                    }
                }
            }
        }
    }
}

#[cfg(windows)]
pub fn reconcile_placeholder_entries(root_path: &Path, entries: &[(PathBuf, NodeId, bool)]) {
    use cloud_filter::placeholder::UpdateOptions;
    if root_path.is_dir() {
        if let Some(mut ph) = open_writable_placeholder(root_path) {
            if matches!(ph.info(), Ok(Some(_))) {
                if let Err(err) =
                    ph.update(UpdateOptions::default().has_children().mark_in_sync(), None)
                {
                    tracing::warn!(?err, path = %root_path.display(), "cfapi sync root update failed");
                }
            }
        }
    }
    for (rel, node_id, is_dir) in entries {
        let full = root_path.join(rel);
        if full.exists() {
            ensure_existing_placeholder(&full, *node_id, *is_dir, *is_dir);
        } else {
            let sanitized_rel: PathBuf = rel
                .components()
                .map(|c| match c {
                    std::path::Component::Normal(s) => {
                        PathBuf::from(sanitize_windows_entry_name(&s.to_string_lossy()))
                    }
                    other => PathBuf::from(other.as_os_str()),
                })
                .collect();
            let alt = root_path.join(sanitized_rel);
            if alt.exists() {
                ensure_existing_placeholder(&alt, *node_id, *is_dir, *is_dir);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_and_deduplicate_windows_names_cover_edge_cases() {
        assert_eq!(
            sanitize_windows_entry_name("a/b:c*d?e\"f<g>h|i"),
            "a_b_c_d_e_f_g_h_i"
        );
        assert_eq!(sanitize_windows_entry_name("report.txt . "), "report.txt");
        assert_eq!(sanitize_windows_entry_name("...   "), "unnamed");
        assert_eq!(sanitize_windows_entry_name("con.txt"), "_con.txt");
        assert_eq!(sanitize_windows_entry_name("LPT1"), "_LPT1");
        assert_eq!(sanitize_windows_entry_name("LPT0"), "LPT0");

        let dedup = deduplicate_windows_names(vec![
            ("Notes.txt".into(), 1, false, 10),
            ("notes.txt".into(), 2, false, 20),
            ("Folder".into(), 3, true, 0),
            ("folder".into(), 4, true, 0),
        ]);
        let names: Vec<&str> = dedup.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(
            names,
            ["Notes.txt", "notes (2).txt", "Folder", "folder (2)"]
        );
    }
}
