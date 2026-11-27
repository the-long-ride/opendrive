//! Embeds the multi-resolution Windows icon resource into `opendrive.exe` without external crates or `rc.exe`.

use std::path::PathBuf;

fn write_res_entry(out: &mut Vec<u8>, res_type: u16, res_id: u16, data: &[u8]) {
    let data_len = u32::try_from(data.len()).expect("resource too large");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    out.extend_from_slice(&0xFFFF_u16.to_le_bytes());
    out.extend_from_slice(&res_type.to_le_bytes());
    out.extend_from_slice(&0xFFFF_u16.to_le_bytes());
    out.extend_from_slice(&res_id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0x1030_u16.to_le_bytes());
    out.extend_from_slice(&0x0409_u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(data);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

fn build_win32_icon_res(ico: &[u8]) -> Option<Vec<u8>> {
    if ico.len() < 6 || ico[0..4] != [0, 0, 1, 0] {
        return None;
    }
    let count = u16::from_le_bytes([ico[4], ico[5]]);
    let dir_end = 6usize.checked_add(usize::from(count) * 16)?;
    if ico.len() < dir_end || count == 0 {
        return None;
    }

    let mut res = Vec::with_capacity(ico.len() + 256);
    // Win32 32-bit .res null header (32 bytes).
    res.extend_from_slice(&0u32.to_le_bytes());
    res.extend_from_slice(&32u32.to_le_bytes());
    res.extend_from_slice(&0xFFFF_u16.to_le_bytes());
    res.extend_from_slice(&0u16.to_le_bytes());
    res.extend_from_slice(&0xFFFF_u16.to_le_bytes());
    res.extend_from_slice(&0u16.to_le_bytes());
    res.extend_from_slice(&[0u8; 16]);

    let mut group = Vec::with_capacity(6 + usize::from(count) * 14);
    group.extend_from_slice(&0u16.to_le_bytes());
    group.extend_from_slice(&1u16.to_le_bytes());
    group.extend_from_slice(&count.to_le_bytes());

    for idx in 0..usize::from(count) {
        let entry = &ico[6 + idx * 16..6 + (idx + 1) * 16];
        let img_size = u32::from_le_bytes(entry[8..12].try_into().ok()?) as usize;
        let img_off = u32::from_le_bytes(entry[12..16].try_into().ok()?) as usize;
        let img_end = img_off.checked_add(img_size)?;
        let img = ico.get(img_off..img_end)?;
        let id = u16::try_from(idx + 1).ok()?;
        write_res_entry(&mut res, 3, id, img);
        group.extend_from_slice(&entry[0..12]);
        group.extend_from_slice(&id.to_le_bytes());
    }
    write_res_entry(&mut res, 14, 1, &group);
    Some(res)
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let ico_path = manifest_dir
        .join("..")
        .join("..")
        .join("docs")
        .join("assets")
        .join("opendrive.ico");
    println!("cargo:rerun-if-changed={}", ico_path.display());

    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os == "windows" && env == "msvc" {
        if let Ok(ico) = std::fs::read(&ico_path) {
            if let Some(res) = build_win32_icon_res(&ico) {
                let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
                let res_path = out_dir.join("opendrive.res");
                if std::fs::write(&res_path, res).is_ok() {
                    println!("cargo:rustc-link-arg-bin=opendrive={}", res_path.display());
                }
            }
        }
    }
}
