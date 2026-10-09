//! Platform sleep locks.
//!
//! - Linux: a systemd-logind inhibitor held by `systemd-inhibit` (no D-Bus
//!   crate in the hub). Its child is `cat` reading a pipe from the hub: when
//!   the hub exits, even on SIGKILL, the pipe closes, `cat` ends and
//!   `systemd-inhibit` with it, so no lock outlives the hub. On a GNOME
//!   desktop the session's own suspend inhibitor is taken too
//!   (`gnome-session-inhibit`): GNOME's automatic suspend of an idle session
//!   looks at the session's inhibitors, not at logind's.
//! - Windows: `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)` on
//!   a thread that holds it until released (Windows drops it with the thread).
//! - macOS: `caffeinate -i -w <hub pid>` (ends with the hub on its own).

use super::{Acquired, Backend, InhibitError, InhibitRequest, LostNotifier, ERR_UNSUPPORTED};

pub(super) fn backend() -> Box<dyn Backend> {
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::SystemdInhibit)
    }
    #[cfg(windows)]
    {
        Box::new(windows::ExecutionState)
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::Caffeinate)
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        Box::new(Unsupported)
    }
}

/// Platforms without a lock (Android never runs a hub; BSDs).
#[allow(dead_code)]
struct Unsupported;

impl Backend for Unsupported {
    fn method(&self) -> Option<&'static str> {
        None
    }
    fn supports_lid(&self) -> bool {
        false
    }
    fn acquire(&self, _: InhibitRequest<'_>, _: LostNotifier) -> Result<Acquired, InhibitError> {
        Err(InhibitError::new(
            ERR_UNSUPPORTED,
            "no sleep lock on this platform",
        ))
    }
}

/// A helper process that holds a lock while it runs (Unix).
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod helper {
    use crate::power::{InhibitError, Lock, LockPart, LostNotifier, ERR_FAILED, ERR_UNSUPPORTED};
    use std::process::Stdio;
    use tokio::io::AsyncReadExt;
    use tokio::process::{ChildStdin, Command};
    use tokio::sync::oneshot;

    /// Maps the exit of a helper that stopped on its own to an error.
    pub type Classify = fn(&str) -> InhibitError;

    pub struct Helper {
        /// Closing it ends `cat`, and with it the helper.
        stdin: Option<ChildStdin>,
        stop: Option<oneshot::Sender<()>>,
    }

    impl Drop for Helper {
        fn drop(&mut self) {
            drop(self.stdin.take());
            if let Some(stop) = self.stop.take() {
                let _ = stop.send(());
            }
        }
    }

    /// Several helpers released together.
    pub struct Helpers(#[allow(dead_code)] pub Vec<Helper>);

    impl Lock for Helpers {}

    /// Start `program args…`, keep its stdin open, and report through `lost`
    /// if it exits before it is released. Needs a tokio runtime context.
    pub fn spawn(
        program: &str,
        args: &[&str],
        lost: Option<(LockPart, LostNotifier)>,
        classify: Classify,
    ) -> Result<Helper, InhibitError> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            // Its own process group: Ctrl-C in the hub's terminal reaches the
            // hub, which then releases the lock in order.
            .process_group(0)
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                InhibitError::new(ERR_UNSUPPORTED, format!("{program} not found"))
            } else {
                InhibitError::new(ERR_FAILED, format!("{program}: {e}"))
            }
        })?;
        let stdin = child.stdin.take();
        let mut stderr = child.stderr.take();
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let program = program.to_string();
        tokio::spawn(async move {
            let exited = tokio::select! {
                _ = stop_rx => None,
                status = child.wait() => Some(status),
            };
            match exited {
                None => {
                    // Released: stdin is closed, so it is already ending.
                    let wait =
                        tokio::time::timeout(std::time::Duration::from_secs(2), child.wait());
                    if wait.await.is_err() {
                        let _ = child.start_kill();
                        let _ = child.wait().await;
                    }
                }
                Some(status) => {
                    let mut text = String::new();
                    if let Some(err) = stderr.as_mut() {
                        let mut buf = Vec::new();
                        let _ = err.take(4096).read_to_end(&mut buf).await;
                        text = String::from_utf8_lossy(&buf).trim().to_string();
                    }
                    let detail = match status {
                        Ok(s) if text.is_empty() => format!("{program} exited ({s})"),
                        Ok(_) => text,
                        Err(e) => format!("{program}: {e}"),
                    };
                    match lost {
                        Some((part, notifier)) => notifier.lost(part, classify(&detail)),
                        None => tracing::debug!(%program, %detail, "sleep prevention helper ended"),
                    }
                }
            }
        });
        Ok(Helper {
            stdin,
            stop: Some(stop_tx),
        })
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::helper::{self, Helpers};
    use crate::power::{
        Acquired, Backend, InhibitError, InhibitRequest, LockPart, LostNotifier, ERR_FAILED,
        ERR_REFUSED, ERR_UNSUPPORTED,
    };

    pub struct SystemdInhibit;

    /// `systemd-inhibit` stderr → stable code.
    fn classify(detail: &str) -> InhibitError {
        let d = detail.to_ascii_lowercase();
        let code = if d.contains("access denied")
            || d.contains("permission denied")
            || d.contains("not authorized")
            || d.contains("interactive authentication required")
        {
            ERR_REFUSED
        } else if d.contains("connect to bus")
            || d.contains("no such file")
            || d.contains("org.freedesktop.login1")
        {
            ERR_UNSUPPORTED
        } else {
            ERR_FAILED
        };
        InhibitError::new(code, detail.to_string())
    }

    fn inhibit(
        what: &str,
        why: &str,
        lost: (LockPart, LostNotifier),
    ) -> Result<helper::Helper, InhibitError> {
        let what = format!("--what={what}");
        let why = format!("--why={why}");
        helper::spawn(
            "systemd-inhibit",
            &[&what, "--who=RE-KORD", &why, "--mode=block", "cat"],
            Some(lost),
            classify,
        )
    }

    /// A GNOME session with its session bus in reach (not a system service).
    fn gnome_session() -> bool {
        std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
            && std::env::var("XDG_CURRENT_DESKTOP")
                .map(|d| d.to_ascii_uppercase().contains("GNOME"))
                .unwrap_or(false)
    }

    impl Backend for SystemdInhibit {
        fn method(&self) -> Option<&'static str> {
            Some("systemd-inhibit")
        }

        fn supports_lid(&self) -> bool {
            true
        }

        fn acquire(
            &self,
            req: InhibitRequest<'_>,
            lost: LostNotifier,
        ) -> Result<Acquired, InhibitError> {
            // Same test as sd_booted(): no systemd, no logind inhibitors
            // (containers, other init systems).
            if !std::path::Path::new("/run/systemd/system").is_dir() {
                return Err(InhibitError::new(ERR_UNSUPPORTED, "systemd is not running"));
            }
            let mut parts = vec![inhibit(
                "sleep:idle",
                req.why,
                (LockPart::Sleep, lost.clone()),
            )?];
            let mut lid_error = None;
            if req.lid {
                // Separate lock: polkit may refuse the lid and still allow sleep.
                match inhibit("handle-lid-switch", req.why, (LockPart::Lid, lost.clone())) {
                    Ok(h) => parts.push(h),
                    Err(e) => lid_error = Some(e),
                }
            }
            let mut extras = Vec::new();
            if gnome_session() {
                // Best effort: suspend only (not "idle"), so the screen still
                // blanks and locks.
                let reason = req.why;
                match helper::spawn(
                    "gnome-session-inhibit",
                    &[
                        "--inhibit",
                        "suspend",
                        "--app-id",
                        "RE-KORD",
                        "--reason",
                        reason,
                        "cat",
                    ],
                    None,
                    classify,
                ) {
                    Ok(h) => {
                        parts.push(h);
                        extras.push("gnome-session-inhibit");
                    }
                    Err(e) => {
                        tracing::debug!(detail = %e.detail, "GNOME session inhibitor not taken")
                    }
                }
            }
            Ok(Acquired {
                lock: Box::new(Helpers(parts)),
                lid_error,
                extras,
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn classifies_systemd_inhibit_errors() {
            assert_eq!(
                classify("Failed to inhibit: Access denied").code,
                ERR_REFUSED
            );
            assert_eq!(
                classify("Failed to connect to bus: No such file or directory").code,
                ERR_UNSUPPORTED
            );
            assert_eq!(
                classify("systemd-inhibit exited (exit status: 1)").code,
                ERR_FAILED
            );
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::helper::{self, Helpers};
    use crate::power::{
        Acquired, Backend, InhibitError, InhibitRequest, LockPart, LostNotifier, ERR_FAILED,
        ERR_UNSUPPORTED,
    };

    pub struct Caffeinate;

    fn classify(detail: &str) -> InhibitError {
        InhibitError::new(ERR_FAILED, detail.to_string())
    }

    impl Backend for Caffeinate {
        fn method(&self) -> Option<&'static str> {
            Some("caffeinate")
        }

        fn supports_lid(&self) -> bool {
            false
        }

        fn acquire(
            &self,
            _req: InhibitRequest<'_>,
            lost: LostNotifier,
        ) -> Result<Acquired, InhibitError> {
            // `-i`: no idle system sleep; `-w`: ends with the hub.
            let pid = std::process::id().to_string();
            let h = helper::spawn(
                "/usr/bin/caffeinate",
                &["-i", "-w", &pid],
                Some((LockPart::Sleep, lost)),
                classify,
            )
            .map_err(|e| InhibitError::new(ERR_UNSUPPORTED, e.detail))?;
            Ok(Acquired {
                lock: Box::new(Helpers(vec![h])),
                lid_error: None,
                extras: Vec::new(),
            })
        }
    }
}

#[cfg(windows)]
mod windows {
    use crate::power::{
        Acquired, Backend, InhibitError, InhibitRequest, Lock, LostNotifier, ERR_FAILED,
    };
    use std::sync::mpsc;

    const ES_CONTINUOUS: u32 = 0x8000_0000;
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetThreadExecutionState(flags: u32) -> u32;
    }

    pub struct ExecutionState;

    /// Dropping the sender wakes the thread, which clears the state and ends.
    struct Held(#[allow(dead_code)] mpsc::Sender<()>);

    impl Lock for Held {}

    impl Backend for ExecutionState {
        fn method(&self) -> Option<&'static str> {
            Some("SetThreadExecutionState")
        }

        fn supports_lid(&self) -> bool {
            false
        }

        fn acquire(
            &self,
            _req: InhibitRequest<'_>,
            _lost: LostNotifier,
        ) -> Result<Acquired, InhibitError> {
            let (release_tx, release_rx) = mpsc::channel::<()>();
            let (ready_tx, ready_rx) = mpsc::channel::<u32>();
            std::thread::Builder::new()
                .name("rekord-power".into())
                .spawn(move || {
                    // SAFETY: plain Win32 call with documented flag values.
                    let previous =
                        unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
                    let _ = ready_tx.send(previous);
                    if previous == 0 {
                        return;
                    }
                    // Blocks until the lock is dropped (sender gone).
                    let _ = release_rx.recv();
                    // SAFETY: as above; the state also ends with the thread.
                    unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
                })
                .map_err(|e| InhibitError::new(ERR_FAILED, format!("power thread: {e}")))?;
            match ready_rx.recv() {
                Ok(0) | Err(_) => Err(InhibitError::new(
                    ERR_FAILED,
                    "SetThreadExecutionState failed",
                )),
                Ok(_) => Ok(Acquired {
                    lock: Box::new(Held(release_tx)),
                    lid_error: None,
                    extras: Vec::new(),
                }),
            }
        }
    }
}
