//! Starting Prism as its own app.
//!
//! macOS charges what a child process does to the app that started it. When Prism's updater
//! writes into `Prism Launcher.app`, macOS sees BeaconSync modifying another developer's app,
//! blocks it and shows "BeaconSync was prevented from modifying apps on your Mac". So on macOS
//! Prism is spawned with the responsibility disclaimed (as terminals and IDEs do), which makes
//! it answer for itself. Elsewhere this is a plain [`Command::spawn`].
//!
//! [`Process`] has the slice of [`std::process::Child`]'s API that BeaconSync uses.

#[cfg(not(target_os = "macos"))]
pub use std::process::Child as Process;

#[cfg(not(target_os = "macos"))]
pub fn spawn(cmd: &mut std::process::Command) -> std::io::Result<Process> {
    cmd.spawn()
}

#[cfg(target_os = "macos")]
pub use macos::{Process, spawn};

#[cfg(target_os = "macos")]
mod macos {
    use std::{
        collections::BTreeMap,
        ffi::{CString, OsStr, OsString},
        io, mem,
        os::unix::{ffi::OsStrExt, process::ExitStatusExt},
        process::{Command, ExitStatus},
        ptr,
    };

    unsafe extern "C" {
        // Private but stable since macOS 10.14; Chromium, LLDB and terminals rely on it.
        fn responsibility_spawnattrs_setdisclaim(attrs: *mut libc::posix_spawnattr_t, disclaim: libc::c_int) -> libc::c_int;
        fn posix_spawn_file_actions_addchdir_np(
            actions: *mut libc::posix_spawn_file_actions_t,
            path: *const libc::c_char,
        ) -> libc::c_int;
    }

    /// A process started by [`spawn`]. Dropping it leaves the process running.
    pub struct Process {
        pid: libc::pid_t,
        /// Set once reaped, so a later `kill` can't hit a reused pid.
        status: Option<ExitStatus>,
    }

    impl Process {
        pub fn wait(&mut self) -> io::Result<ExitStatus> {
            loop {
                if let Some(s) = self.try_wait_with(0)? {
                    return Ok(s);
                }
            }
        }

        pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            self.try_wait_with(libc::WNOHANG)
        }

        fn try_wait_with(&mut self, flags: libc::c_int) -> io::Result<Option<ExitStatus>> {
            if self.status.is_none() {
                let mut raw = 0;
                match unsafe { libc::waitpid(self.pid, &mut raw, flags) } {
                    0 => return Ok(None),
                    -1 => {
                        let e = io::Error::last_os_error();
                        return if e.kind() == io::ErrorKind::Interrupted { Ok(None) } else { Err(e) };
                    }
                    _ => self.status = Some(ExitStatus::from_raw(raw)),
                }
            }
            Ok(self.status)
        }

        pub fn kill(&mut self) -> io::Result<()> {
            if self.status.is_none() && unsafe { libc::kill(self.pid, libc::SIGKILL) } == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }

    /// Spawns `cmd` (program, args, env and working dir; stdio inherited) as its own
    /// responsible process.
    pub fn spawn(cmd: &mut Command) -> io::Result<Process> {
        let program = cstr(cmd.get_program())?;
        let args = std::iter::once(Ok(program.clone())).chain(cmd.get_args().map(cstr)).collect::<io::Result<Vec<_>>>()?;
        let mut vars: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
        for (k, v) in cmd.get_envs() {
            match v {
                Some(v) => vars.insert(k.into(), v.into()),
                None => vars.remove(k),
            };
        }
        let env = vars
            .into_iter()
            .map(|(mut kv, v)| {
                kv.push("=");
                kv.push(v);
                cstr(&kv)
            })
            .collect::<io::Result<Vec<_>>>()?;
        let dir = cmd.get_current_dir().map(|d| cstr(d.as_os_str())).transpose()?;
        let (argv, envp) = (nul_terminated(&args), nul_terminated(&env));

        let mut attrs = Attrs::new()?;
        let mut actions = Actions::new()?;
        unsafe {
            check(responsibility_spawnattrs_setdisclaim(&mut attrs.0, 1))?;
            // Like std: the child starts with no blocked signals and SIGPIPE back to default
            // (Rust ignores it in this process, and ignored signals survive exec).
            let mut none: libc::sigset_t = mem::zeroed();
            libc::sigemptyset(&mut none);
            let mut pipe: libc::sigset_t = mem::zeroed();
            libc::sigemptyset(&mut pipe);
            libc::sigaddset(&mut pipe, libc::SIGPIPE);
            check(libc::posix_spawnattr_setsigmask(&mut attrs.0, &none))?;
            check(libc::posix_spawnattr_setsigdefault(&mut attrs.0, &pipe))?;
            check(libc::posix_spawnattr_setflags(
                &mut attrs.0,
                (libc::POSIX_SPAWN_SETSIGMASK | libc::POSIX_SPAWN_SETSIGDEF) as libc::c_short,
            ))?;
            if let Some(dir) = &dir {
                check(posix_spawn_file_actions_addchdir_np(&mut actions.0, dir.as_ptr()))?;
            }
            let mut pid = 0;
            check(libc::posix_spawnp(&mut pid, program.as_ptr(), &actions.0, &attrs.0, argv.as_ptr(), envp.as_ptr()))?;
            Ok(Process { pid, status: None })
        }
    }

    fn cstr(s: &OsStr) -> io::Result<CString> {
        CString::new(s.as_bytes()).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
    }

    fn nul_terminated(strings: &[CString]) -> Vec<*mut libc::c_char> {
        strings.iter().map(|s| s.as_ptr().cast_mut()).chain([ptr::null_mut()]).collect()
    }

    /// posix_spawn functions return the error number instead of setting errno.
    fn check(code: libc::c_int) -> io::Result<()> {
        if code == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(code)) }
    }

    struct Attrs(libc::posix_spawnattr_t);

    impl Attrs {
        fn new() -> io::Result<Self> {
            let mut a = ptr::null_mut();
            check(unsafe { libc::posix_spawnattr_init(&mut a) })?;
            Ok(Self(a))
        }
    }

    impl Drop for Attrs {
        fn drop(&mut self) {
            unsafe { libc::posix_spawnattr_destroy(&mut self.0) };
        }
    }

    struct Actions(libc::posix_spawn_file_actions_t);

    impl Actions {
        fn new() -> io::Result<Self> {
            let mut a = ptr::null_mut();
            check(unsafe { libc::posix_spawn_file_actions_init(&mut a) })?;
            Ok(Self(a))
        }
    }

    impl Drop for Actions {
        fn drop(&mut self) {
            unsafe { libc::posix_spawn_file_actions_destroy(&mut self.0) };
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn runs_with_args_env_and_dir() {
            let dir = std::env::temp_dir().canonicalize().unwrap();
            let mut cmd = Command::new("sh");
            cmd.args(["-c", r#"[ "$PWD" = "$1" ] && [ "$BEACON_TEST" = yes ] && exit 7"#, "sh"])
                .arg(&dir)
                .env("BEACON_TEST", "yes")
                .current_dir(&dir);
            assert_eq!(spawn(&mut cmd).unwrap().wait().unwrap().code(), Some(7));
        }

        unsafe extern "C" {
            fn responsibility_get_pid_responsible_for_pid(pid: libc::pid_t) -> libc::pid_t;
        }

        #[test]
        fn answers_for_itself() {
            let mut p = spawn(Command::new("sleep").arg("30")).unwrap();
            assert_eq!(unsafe { responsibility_get_pid_responsible_for_pid(p.pid) }, p.pid);
            p.kill().unwrap();
            p.wait().unwrap();
        }

        #[test]
        fn kill_stops_it() {
            let mut p = spawn(Command::new("sleep").arg("30")).unwrap();
            assert_eq!(p.try_wait().unwrap(), None);
            p.kill().unwrap();
            assert_eq!(p.wait().unwrap().signal(), Some(libc::SIGKILL));
        }
    }
}
