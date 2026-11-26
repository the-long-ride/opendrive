//! OS Explorer refresh, detached watchdog spawning, and process-exit cleanup helpers.

use std::path::Path;
use std::process::{Child, Command, Stdio};
#[cfg(windows)]
use std::sync::Once;
use std::sync::{Arc, Mutex};

#[cfg(windows)]
#[link(name = "shell32")]
extern "system" {
    fn SHChangeNotify(
        w_event_id: i32,
        u_flags: u32,
        dw_item1: *const core::ffi::c_void,
        dw_item2: *const core::ffi::c_void,
    );
    fn SHGetSpecialFolderLocation(
        hwnd_owner: isize,
        n_folder: i32,
        ppidl: *mut *mut core::ffi::c_void,
    ) -> i32;
    fn SHParseDisplayName(
        psz_name: *const u16,
        pbc: *mut core::ffi::c_void,
        ppidl: *mut *mut core::ffi::c_void,
        sfgao_in: u32,
        psfgao_out: *mut u32,
    ) -> i32;
}

#[cfg(windows)]
#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(pv_reserved: *const core::ffi::c_void, dw_coinit: u32) -> i32;
    fn CoUninitialize();
    fn CoTaskMemFree(pv: *mut core::ffi::c_void);
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn SendMessageTimeoutW(
        hwnd: isize,
        msg: u32,
        w_param: usize,
        l_param: isize,
        fu_flags: u32,
        u_timeout: u32,
        lpdw_result: *mut usize,
    ) -> isize;
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(
        dw_desired_access: u32,
        b_inherit_handle: i32,
        dw_process_id: u32,
    ) -> *mut core::ffi::c_void;
    fn WaitForSingleObject(h_handle: *mut core::ffi::c_void, dw_milliseconds: u32) -> u32;
    fn CloseHandle(h_object: *mut core::ffi::c_void) -> i32;
    fn SetConsoleCtrlHandler(
        handler_routine: Option<unsafe extern "system" fn(u32) -> i32>,
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
pub(crate) fn remove_namespace_with_shell_notify<T>(
    clsid: &str,
    remove_op: impl FnOnce() -> T,
) -> T {
    #[cfg(windows)]
    {
        struct ComGuard(bool);
        impl Drop for ComGuard {
            fn drop(&mut self) {
                if self.0 {
                    unsafe { CoUninitialize() };
                }
            }
        }
        let hr = unsafe { CoInitializeEx(core::ptr::null(), 0x2) };
        let _com = ComGuard(hr == 0 || hr == 1);
        let mut pidls = Vec::with_capacity(2);
        for spec in [
            format!("::{{20D04FE0-3AEA-1069-A2D8-08002B30309D}}\\::{clsid}"),
            format!("::{clsid}"),
        ] {
            let wide: Vec<u16> = spec.encode_utf16().chain(Some(0)).collect();
            let mut pidl: *mut core::ffi::c_void = core::ptr::null_mut();
            let parsed = unsafe {
                SHParseDisplayName(
                    wide.as_ptr(),
                    core::ptr::null_mut(),
                    &mut pidl,
                    0,
                    core::ptr::null_mut(),
                )
            };
            if parsed >= 0 && !pidl.is_null() {
                pidls.push(pidl);
            }
        }
        let out = remove_op();
        for pidl in pidls {
            unsafe {
                SHChangeNotify(0x0000_0010, 0x1000, pidl, core::ptr::null());
                SHChangeNotify(0x0000_0004, 0x1000, pidl, core::ptr::null());
                CoTaskMemFree(pidl);
            }
        }
        out
    }
    #[cfg(not(windows))]
    {
        let _ = clsid;
        remove_op()
    }
}

/// Notifies Windows Explorer that virtual drives or `This PC` namespace entries changed without restarting `explorer.exe`.
pub fn notify_shell_mounts_changed() {
    #[cfg(windows)]
    unsafe {
        for csidl in [0x0011_i32, 0x0000_i32] {
            let mut pidl: *mut core::ffi::c_void = core::ptr::null_mut();
            if SHGetSpecialFolderLocation(0, csidl, &mut pidl) >= 0 && !pidl.is_null() {
                SHChangeNotify(0x0000_1000, 0x1000, pidl, core::ptr::null());
                CoTaskMemFree(pidl);
            }
        }
        SHChangeNotify(0x0800_0000, 0x1000, core::ptr::null(), core::ptr::null());
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
        pid <= i32::MAX as u32 && unsafe { kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(any(windows, unix)))]
    {
        false
    }
}

/// Blocks until process `pid` exits (returns immediately if `pid` is invalid or already gone).
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
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn shell_notify_and_zero_or_exited_pid_wait_are_safe() {
        notify_shell_mounts_changed();
        assert!(is_process_alive(std::process::id()));
        assert!(!is_process_alive(0) && !is_process_alive(u32::MAX));
        wait_for_process_exit(0);
        wait_for_process_exit(u32::MAX);
        assert_eq!(
            remove_namespace_with_shell_notify("{00000000-0000-0000-0000-000000000000}", || 42),
            42
        );
        #[cfg(windows)]
        {
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
}
