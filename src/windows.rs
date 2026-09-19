//! Windows only: getting the overlay into the `ZBID_UIACCESS` window band.
//!
//! Since Windows 8, windows live in bands, and `SetWindowPos(HWND_TOPMOST)`
//! cannot lift a window out of `ZBID_DESKTOP` - the band an ordinary process's
//! windows are created in. Anything sitting higher therefore covers the overlay
//! no matter what: Task Manager with "Always on top" ticked
//! (`ZBID_SYSTEM_TOOLS`), the on-screen keyboard, the lock screen. The only
//! band above those that an application can reach is `ZBID_UIACCESS`, and the
//! only way in is for the process itself to hold UIAccess.
//!
//! UIAccess normally requires an Authenticode signature and installation under
//! `%ProgramFiles%`. Putting `uiAccess="true"` in the manifest of an unsigned
//! binary does not degrade gracefully - the process simply refuses to start -
//! so that is not an option here. This uses the other documented route: with
//! administrator rights, take a copy of a SYSTEM process's token, set
//! `TokenUIAccess` on the copy, and start a second instance with it. That
//! instance holds UIAccess from the moment it starts, which is when the band
//! gets decided.
//!
//! Everything here is best-effort by design. Every failure path leaves a
//! normally-working, normally-privileged overlay running: declining the UAC
//! prompt, or being unable to reach a SYSTEM token, costs the top window band
//! and nothing else.

/// Set on the process we start ourselves, so the bootstrap knows how far it got.
const MARKER_ELEVATED: &str = "--as-elevated";
/// Set on the final instance, the one that actually owns the overlay.
const MARKER_UIA: &str = "--as-uia";

/// What the caller should do once the bootstrap has had its say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Carry on and start the overlay in this process.
    Continue,
    /// A better-privileged instance was started; this one should exit quietly.
    HandedOver,
}

/// Internal flags the argument parser has to tolerate. The bootstrap sets them
/// when it re-launches; a person never types them.
pub fn is_internal_flag(arg: &str) -> bool {
    arg == MARKER_ELEVATED || arg == MARKER_UIA
}

// Only the Windows implementation calls these, but they are worth testing
// everywhere, so a test build keeps them on every platform.
#[cfg(any(windows, test))]
/// The command line for a re-launch: every argument we were given, minus any
/// marker already applied, plus the new one at the end.
///
/// This is the part that can go wrong in a way that loops forever - a marker
/// that does not survive the round trip re-launches forever - so it is kept
/// pure, and tested on every platform rather than only on Windows.
fn child_args(args: &[String], marker: &str) -> Vec<String> {
    let mut child: Vec<String> = args
        .iter()
        .filter(|arg| !is_internal_flag(arg))
        .cloned()
        .collect();
    child.push(marker.to_string());
    child
}

// Only the Windows implementation calls these, but they are worth testing
// everywhere, so a test build keeps them on every platform.
#[cfg(any(windows, test))]
/// Quote one argument the way `CommandLineToArgvW` expects to read it back.
fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    let mut backslashes = 0;
    for ch in arg.chars() {
        match ch {
            '\\' => {
                backslashes += 1;
                out.push('\\');
            }
            '"' => {
                // Backslashes in front of a quote have to be doubled first.
                for _ in 0..backslashes {
                    out.push('\\');
                }
                out.push_str("\\\"");
                backslashes = 0;
            }
            _ => {
                backslashes = 0;
                out.push(ch);
            }
        }
    }
    // A trailing run of backslashes would otherwise escape the closing quote.
    for _ in 0..backslashes {
        out.push('\\');
    }
    out.push('"');
    out
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, LUID};
    use windows_sys::Win32::Security::{
        AdjustTokenPrivileges, DuplicateTokenEx, GetTokenInformation, LookupPrivilegeValueW,
        SecurityImpersonation, SetTokenInformation, TokenElevation, TokenPrimary, TokenUIAccess,
        LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_DEFAULT, TOKEN_ADJUST_PRIVILEGES,
        TOKEN_ADJUST_SESSIONID, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_PRIVILEGES,
        TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::SystemServices::MAXIMUM_ALLOWED;
    use windows_sys::Win32::System::Threading::{
        CreateProcessAsUserW, GetCurrentProcess, GetCurrentProcessId, OpenProcess,
        OpenProcessToken, CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION,
        PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW,
    };
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    use super::{child_args, quote, Outcome, MARKER_ELEVATED, MARKER_UIA};

    /// Climb towards UIAccess, re-launching the process at most twice.
    ///
    /// `wanted` comes from the configuration, so the whole feature can be
    /// turned off without a rebuild.
    pub fn bootstrap(wanted: bool) -> Outcome {
        if !wanted {
            return Outcome::Continue;
        }

        let args: Vec<String> = std::env::args().collect();
        let already_elevated = args.iter().any(|a| a == MARKER_ELEVATED);

        // The instance started with the UIAccess token is the finished article.
        if args.iter().any(|a| a == MARKER_UIA) {
            return Outcome::Continue;
        }
        // Someone launched us holding UIAccess already - a debugger, an
        // installer, a parent that did the work. Nothing left to do.
        if has_ui_access() {
            return Outcome::Continue;
        }

        if !already_elevated && !is_elevated() {
            match escalate(&args, MARKER_ELEVATED) {
                Ok(()) => return Outcome::HandedOver,
                Err(reason) => {
                    // Declining the prompt is a perfectly reasonable answer:
                    // the overlay works, it just cannot reach the top band.
                    eprintln!("[float-clock] not running as administrator: {reason}");
                    eprintln!("[float-clock] the overlay will stay below windows in a higher band");
                    return Outcome::Continue;
                }
            }
        }

        match relaunch_with_ui_access(&args) {
            Ok(()) => Outcome::HandedOver,
            Err(reason) => {
                eprintln!("[float-clock] could not obtain UIAccess: {reason}");
                eprintln!("[float-clock] the overlay will stay below windows in a higher band");
                Outcome::Continue
            }
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        OsString::from(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// Owns a Win32 handle so that every early return closes it.
    struct OwnedHandle(HANDLE);

    impl OwnedHandle {
        fn new(raw: HANDLE) -> Option<Self> {
            if raw.is_null() || raw as isize == -1 {
                None
            } else {
                Some(Self(raw))
            }
        }
    }

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    fn last_error(context: &str) -> String {
        format!("{context} failed (error {})", unsafe { GetLastError() })
    }

    /// Read a `u32`-sized token information class.
    fn token_u32(token: HANDLE, class: i32) -> Option<u32> {
        let mut value: u32 = 0;
        let mut returned: u32 = 0;
        let ok = unsafe {
            GetTokenInformation(
                token,
                class,
                &mut value as *mut u32 as *mut core::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
                &mut returned,
            )
        };
        (ok != 0).then_some(value)
    }

    fn own_token(desired: u32) -> Option<OwnedHandle> {
        let mut token: HANDLE = std::ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), desired, &mut token) } == 0 {
            return None;
        }
        OwnedHandle::new(token)
    }

    /// Does this process hold UIAccess already?
    fn has_ui_access() -> bool {
        own_token(TOKEN_QUERY)
            .and_then(|token| token_u32(token.0, TokenUIAccess))
            .is_some_and(|value| value != 0)
    }

    /// Is this process running with administrator rights?
    fn is_elevated() -> bool {
        own_token(TOKEN_QUERY)
            .and_then(|token| token_u32(token.0, TokenElevation))
            .is_some_and(|value| value != 0)
    }

    fn current_exe_wide() -> Result<Vec<u16>, String> {
        let exe =
            std::env::current_exe().map_err(|e| format!("cannot locate this executable: {e}"))?;
        Ok(wide(&exe.to_string_lossy()))
    }

    /// Re-launch through the shell, asking for elevation.
    ///
    /// `ShellExecuteW` returns a value above 32 on success; anything at or
    /// below that - `ERROR_CANCELLED` when the user says no - is a refusal.
    fn escalate(args: &[String], marker: &str) -> Result<(), String> {
        let exe = current_exe_wide()?;
        let parameters: Vec<String> = child_args(args, marker).iter().map(|a| quote(a)).collect();
        let parameters = wide(&parameters.join(" "));
        let verb = wide("runas");

        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                exe.as_ptr(),
                parameters.as_ptr(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        } as isize;

        if result > 32 {
            Ok(())
        } else {
            Err(format!("the elevation request returned {result}"))
        }
    }

    /// Enable one of our own privileges. An administrator's token holds
    /// `SeDebugPrivilege` but starts with it disabled, and opening a SYSTEM
    /// process's token needs it.
    fn enable_privilege(name: &str) -> Result<(), String> {
        let token = own_token(TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY)
            .ok_or_else(|| last_error("OpenProcessToken"))?;

        let name = wide(name);
        let mut luid = LUID::default();
        if unsafe { LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) } == 0 {
            return Err(last_error("LookupPrivilegeValueW"));
        }

        let privileges = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        let ok = unsafe {
            AdjustTokenPrivileges(
                token.0,
                0,
                &privileges,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        // AdjustTokenPrivileges reports success even when it changed nothing,
        // so the error code is the real answer here.
        if ok == 0 {
            return Err(last_error("AdjustTokenPrivileges"));
        }
        let error = unsafe { GetLastError() };
        if error != 0 {
            return Err(format!("SeDebugPrivilege was not granted (error {error})"));
        }
        Ok(())
    }

    /// Find a SYSTEM process in our own session and take a copy of its token
    /// with `TokenUIAccess` set.
    fn ui_access_token() -> Result<OwnedHandle, String> {
        let mut session: u32 = 0;
        unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) };

        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        let snapshot =
            OwnedHandle::new(snapshot).ok_or_else(|| last_error("CreateToolhelp32Snapshot"))?;

        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        let mut pid = None;
        let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
        while more {
            let end = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
            if name.eq_ignore_ascii_case("winlogon.exe") {
                let mut candidate: u32 = 0;
                unsafe { ProcessIdToSessionId(entry.th32ProcessID, &mut candidate) };
                if candidate == session {
                    pid = Some(entry.th32ProcessID);
                    break;
                }
            }
            more = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
        }

        let pid = pid.ok_or("no winlogon.exe in this session")?;
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        let process =
            OwnedHandle::new(process).ok_or_else(|| last_error("OpenProcess(winlogon.exe)"))?;

        let mut source: HANDLE = std::ptr::null_mut();
        if unsafe { OpenProcessToken(process.0, TOKEN_DUPLICATE | TOKEN_QUERY, &mut source) } == 0 {
            return Err(last_error("OpenProcessToken(winlogon.exe)"));
        }
        let source = OwnedHandle::new(source).ok_or("no token from winlogon.exe")?;

        let mut duplicate: HANDLE = std::ptr::null_mut();
        let ok = unsafe {
            DuplicateTokenEx(
                source.0,
                MAXIMUM_ALLOWED
                    | TOKEN_ADJUST_DEFAULT
                    | TOKEN_ADJUST_SESSIONID
                    | TOKEN_QUERY
                    | TOKEN_ASSIGN_PRIMARY
                    | TOKEN_DUPLICATE,
                std::ptr::null(),
                SecurityImpersonation,
                TokenPrimary,
                &mut duplicate,
            )
        };
        if ok == 0 {
            return Err(last_error("DuplicateTokenEx"));
        }
        let duplicate = OwnedHandle::new(duplicate).ok_or("DuplicateTokenEx returned no token")?;

        let enabled: u32 = 1;
        let ok = unsafe {
            SetTokenInformation(
                duplicate.0,
                TokenUIAccess,
                &enabled as *const u32 as *const core::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            )
        };
        if ok == 0 {
            return Err(last_error("SetTokenInformation(TokenUIAccess)"));
        }

        Ok(duplicate)
    }

    /// Start the final instance with a UIAccess token.
    fn relaunch_with_ui_access(args: &[String]) -> Result<(), String> {
        enable_privilege("SeDebugPrivilege")?;
        let token = ui_access_token()?;

        let exe = current_exe_wide()?;
        let command: Vec<String> = child_args(args, MARKER_UIA)
            .iter()
            .map(|a| quote(a))
            .collect();
        let mut command = wide(&command.join(" "));

        let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
        startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        // A UIAccess process has to be created on the default desktop.
        let desktop = wide("winsta0\\default");
        startup.lpDesktop = desktop.as_ptr() as *mut u16;

        let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            CreateProcessAsUserW(
                token.0,
                exe.as_ptr(),
                command.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                CREATE_UNICODE_ENVIRONMENT,
                std::ptr::null(),
                std::ptr::null(),
                &startup,
                &mut info,
            )
        };
        if ok == 0 {
            return Err(last_error("CreateProcessAsUserW"));
        }
        unsafe {
            CloseHandle(info.hProcess);
            CloseHandle(info.hThread);
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Outcome;

    /// Nothing to climb towards: window bands are a Windows concept, and so is
    /// the UIAccess requirement behind them.
    pub fn bootstrap(_wanted: bool) -> Outcome {
        Outcome::Continue
    }
}

pub use imp::bootstrap;

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_re_launch_keeps_the_arguments_and_replaces_the_marker() {
        let input = args(&[
            "float-clock.exe",
            "--config",
            "C:\\my data\\c.toml",
            MARKER_ELEVATED,
        ]);
        let child = child_args(&input, MARKER_UIA);
        assert_eq!(
            child,
            args(&[
                "float-clock.exe",
                "--config",
                "C:\\my data\\c.toml",
                MARKER_UIA
            ])
        );
        // Exactly one marker, so a re-launch can never chain into a loop.
        assert_eq!(child.iter().filter(|a| is_internal_flag(a)).count(), 1);
    }

    #[test]
    fn quoting_survives_the_awkward_cases() {
        assert_eq!(quote("plain"), "plain");
        assert_eq!(quote("has space"), "\"has space\"");
        assert_eq!(quote(""), "\"\"");
        assert_eq!(quote("say \"hi\""), "\"say \\\"hi\\\"\"");
        // A trailing backslash must not escape the closing quote.
        assert_eq!(quote("ends with \\"), "\"ends with \\\\\"");
    }

    #[test]
    fn the_markers_are_the_only_internal_flags() {
        assert!(is_internal_flag(MARKER_ELEVATED));
        assert!(is_internal_flag(MARKER_UIA));
        assert!(!is_internal_flag("--config"));
    }
}
