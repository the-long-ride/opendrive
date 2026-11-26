//! OS Explorer refresh, detached watchdog spawning, and process-exit cleanup helpers.

#[cfg(windows)]
use core::ffi::c_void;
#[cfg(windows)]
use core::ptr::{null, null_mut};
use std::path::Path;
use std::process::{Child, Command, Stdio};
#[cfg(windows)]
use std::sync::Once;
use std::sync::{Arc, Mutex};

#[cfg(windows)]
#[link(name = "shell32")]
extern "system" {
    fn SHChangeNotify(event: i32, flags: u32, item1: *const c_void, item2: *const c_void);
    fn SHGetSpecialFolderLocation(owner: isize, folder: i32, ppidl: *mut *mut c_void) -> i32;
    fn SHParseDisplayName(
        name: *const u16,
        pbc: *mut c_void,
        ppidl: *mut *mut c_void,
        in_flags: u32,
        out_flags: *mut u32,
    ) -> i32;
}

#[cfg(windows)]
#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(reserved: *const c_void, coinit: u32) -> i32;
    fn CoUninitialize();
    fn CoTaskMemFree(pv: *mut c_void);
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn SendMessageTimeoutW(
        hwnd: isize,
        msg: u32,
        w: usize,
        l: isize,
        flags: u32,
        timeout: u32,
        res: *mut usize,
    ) -> isize;
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
    fn WaitForSingleObject(handle: *mut c_void, ms: u32) -> u32;
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn SetConsoleCtrlHandler(
        routine: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
}

static CONSOLE_CLEANUP_HOOK: Mutex<Option<Arc<dyn Fn() + Send + Sync>>> = Mutex::new(None);
#[cfg(windows)]
static CONSOLE_HANDLER_ONCE: Once = Once::new();

#[cfg(windows)]
unsafe extern "system" fn console_ctrl_handler(ctrl_type: u32) -> i32 {
    if matches!(ctrl_type, 0 | 1 | 2 | 5 | 6) {
        let hook = CONSOLE_CLEANUP_HOOK
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(Arc::clone));
        if let Some(cb) = hook {
            cb();
        }
    }
    0
}

/// Registers a callback invoked on console interrupt/close/logoff/shutdown events (Windows).
pub fn install_console_cleanup_handler(hook: impl Fn() + Send + Sync + 'static) {
    if let Ok(mut guard) = CONSOLE_CLEANUP_HOOK.lock() {
        *guard = Some(Arc::new(hook));
    }
    #[cfg(windows)]
    CONSOLE_HANDLER_ONCE.call_once(|| unsafe {
        let _ = SetConsoleCtrlHandler(Some(console_ctrl_handler), 1);
    });
}

/// Spawns a detached, windowless background child process (surviving `CREATE_NO_WINDOW` rules on Windows).
pub fn spawn_detached_background(program: &Path, args: &[&str]) -> std::io::Result<Child> {
    let build = || {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

        let mut primary = build();
        primary.creation_flags(
            CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB,
        );
        match primary.spawn() {
            Ok(child) => Ok(child),
            Err(_) => {
                // Fallback when a parent Job Object forbids breakaway (never combine DETACHED_PROCESS + CREATE_NO_WINDOW).
                let mut fallback = build();
                fallback.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
                fallback.spawn()
            }
        }
    }
    #[cfg(not(windows))]
    {
        build().spawn()
    }
}

/// Captures Shell PIDLs for a removed namespace CLSID before deletion and notifies Explorer immediately afterward.
#[cfg(windows)]
pub(crate) fn remove_namespace_with_shell_notify<T>(
    clsid: &str,
    remove_op: impl FnOnce() -> T,
) -> T {
    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }
    let hr = unsafe { CoInitializeEx(null(), 0x2) };
    let _com = ComGuard(hr == 0 || hr == 1);
    let mut pidls = Vec::with_capacity(2);
    for spec in [
        format!("::{{20D04FE0-3AEA-1069-A2D8-08002B30309D}}\\::{clsid}"),
        format!("::{clsid}"),
    ] {
        let wide: Vec<u16> = spec.encode_utf16().chain(Some(0)).collect();
        let mut pidl: *mut c_void = null_mut();
        let parsed =
            unsafe { SHParseDisplayName(wide.as_ptr(), null_mut(), &mut pidl, 0, null_mut()) };
        if parsed >= 0 && !pidl.is_null() {
            pidls.push(pidl);
        }
    }
    let out = remove_op();
    for pidl in pidls {
        unsafe {
            SHChangeNotify(0x0000_0010, 0x1000, pidl, null());
            SHChangeNotify(0x0000_0004, 0x1000, pidl, null());
            CoTaskMemFree(pidl);
        }
    }
    out
}

/// Notifies Windows Explorer that virtual drives or `This PC` namespace entries changed without restarting `explorer.exe`.
pub fn notify_shell_mounts_changed() {
    #[cfg(windows)]
    unsafe {
        for csidl in [0x0011_i32, 0x0000_i32] {
            let mut pidl: *mut c_void = null_mut();
            if SHGetSpecialFolderLocation(0, csidl, &mut pidl) >= 0 && !pidl.is_null() {
                SHChangeNotify(0x0000_1000, 0x1000, pidl, null());
                CoTaskMemFree(pidl);
            }
        }
        SHChangeNotify(0x0800_0000, 0x1000, null(), null());
        let shell_state: Vec<u16> = "ShellState\0".encode_utf16().collect();
        let mut msg_res = 0_usize;
        let _ = SendMessageTimeoutW(
            0xFFFF_u16 as isize,
            0x001A,
            0,
            shell_state.as_ptr() as isize,
            0x0002,
            200,
            &mut msg_res,
        );
    }
}

#[cfg(target_os = "linux")]
fn is_zombie_unix(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| {
            s.rsplit_once(')')
                .map(|(_, r)| r.trim_start().starts_with(['Z', 'X', 'x']))
        })
        .unwrap_or(true)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn is_zombie_unix(pid: u32) -> bool {
    Command::new("ps")
        .args(["-o", "state=", "-p", &pid.to_string()])
        .output()
        .ok()
        .is_some_and(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            let st = s.trim_start();
            st.is_empty() || st.starts_with(['Z', 'X'])
        })
}

/// Returns whether process `pid` is currently alive (cross-platform without requiring `/proc`).
pub fn is_process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    unsafe {
        const SYNCHRONIZE: u32 = 0x0010_0000;
        const WAIT_TIMEOUT: u32 = 0x0000_0102;
        let handle = OpenProcess(SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return false;
        }
        let alive = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
        let _ = CloseHandle(handle);
        alive
    }
    #[cfg(unix)]
    {
        extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        pid <= i32::MAX as u32 && unsafe { kill(pid as i32, 0) == 0 } && !is_zombie_unix(pid)
    }
    #[cfg(not(any(windows, unix)))]
    {
        false
    }
}

/// Blocks until process `pid` exits (returns immediately if `pid` is invalid, exited, or already gone).
pub fn wait_for_process_exit(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(windows)]
    unsafe {
        const SYNCHRONIZE: u32 = 0x0010_0000;
        const INFINITE: u32 = 0xFFFF_FFFF;
        let handle = OpenProcess(SYNCHRONIZE, 0, pid);
        if !handle.is_null() {
            let _ = WaitForSingleObject(handle, INFINITE);
            let _ = CloseHandle(handle);
        }
    }
    #[cfg(not(windows))]
    {
        while is_process_alive(pid) {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn windows_internal_namespace_notify_and_ctrl_handler_work() {
        assert_eq!(
            remove_namespace_with_shell_notify("{00000000-0000-0000-0000-000000000000}", || 42),
            42
        );
        let hits = Arc::new(AtomicUsize::new(0));
        let cb = Arc::clone(&hits);
        install_console_cleanup_handler(move || {
            cb.fetch_add(1, Ordering::SeqCst);
        });
        unsafe {
            assert_eq!((console_ctrl_handler(0), console_ctrl_handler(99)), (0, 0));
        }
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        *CONSOLE_CLEANUP_HOOK.lock().unwrap() = None;
    }
}
