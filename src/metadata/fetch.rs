use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use std::{io, thread};

use anyhow::{Result, bail, ensure};
use rustix::process::{Pid, Signal, kill_process_group};
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

use super::MAX_BYTES;
use super::sources::Source;

pub struct Cancellation {
    flag: Arc<AtomicBool>,
    registrations: Vec<signal_hook::SigId>,
}

impl Cancellation {
    pub fn new() -> Result<Self> {
        let flag = Arc::new(AtomicBool::new(false));
        let mut result = Self {
            flag,
            registrations: Vec::new(),
        };
        for signal in [SIGINT, SIGTERM, SIGHUP] {
            result.registrations.push(
                signal_hook::flag::register(signal, Arc::clone(&result.flag))
                    .map_err(|_| anyhow::anyhow!("cannot register metadata cancellation"))?,
            );
        }
        Ok(result)
    }

    pub fn check(&self) -> Result<()> {
        ensure!(
            !self.flag.load(Ordering::SeqCst),
            "metadata operation cancelled"
        );
        Ok(())
    }
}

impl Drop for Cancellation {
    fn drop(&mut self) {
        for registration in self.registrations.drain(..) {
            signal_hook::low_level::unregister(registration);
        }
    }
}

struct OwnedChild {
    child: Child,
    reaped: bool,
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = kill_process_group(Pid::from_child(&self.child), Signal::KILL);
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn reader(
    mut pipe: impl Read + Send + 'static,
    limit: usize,
) -> mpsc::Receiver<io::Result<Vec<u8>>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut contents = Vec::new();
        let result = pipe
            .by_ref()
            .take(limit as u64 + 1)
            .read_to_end(&mut contents)
            .and_then(|_| {
                if contents.len() > limit {
                    Err(io::Error::other("metadata output exceeded its limit"))
                } else {
                    Ok(contents)
                }
            });
        let _ = sender.send(result);
    });
    receiver
}

fn receive(
    receiver: &mpsc::Receiver<io::Result<Vec<u8>>>,
    value: &mut Option<Vec<u8>>,
) -> Result<()> {
    if value.is_none() {
        match receiver.try_recv() {
            Ok(Ok(contents)) => *value = Some(contents),
            Ok(Err(_)) | Err(mpsc::TryRecvError::Disconnected) => {
                bail!("1Password output is invalid or oversized")
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
    Ok(())
}

pub fn document(
    executable: &Path,
    source: &Source,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<Vec<u8>> {
    cancellation.check()?;
    ensure!(
        Instant::now() < deadline,
        "metadata retrieval deadline expired"
    );
    let mut command = Command::new(executable);
    command.env_clear();
    let context = [
        "HOME",
        "PATH",
        "USER",
        "LOGNAME",
        "LANG",
        "TERM",
        "XDG_CONFIG_HOME",
        "XDG_RUNTIME_DIR",
        "XDG_SESSION_TYPE",
        "DBUS_SESSION_BUS_ADDRESS",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "OP_CONFIG_DIR",
    ];
    for (name, value) in std::env::vars_os() {
        let name_text = name.to_string_lossy();
        if context.contains(&name_text.as_ref()) || name_text.starts_with("LC_") {
            command.env(name, value);
        }
    }
    command.env("OP_BIOMETRIC_UNLOCK_ENABLED", "true");
    command
        .args([
            "--account",
            &source.account,
            "--no-color",
            "document",
            "get",
            &source.document,
            "--vault",
            &source.vault,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let child = command
        .spawn()
        .map_err(|_| anyhow::anyhow!("cannot start the configured 1Password CLI"))?;
    let mut owned = OwnedChild {
        child,
        reaped: false,
    };
    let stdout = reader(owned.child.stdout.take().expect("piped stdout"), MAX_BYTES);
    let stderr = reader(owned.child.stderr.take().expect("piped stderr"), 32768);
    let mut output = None;
    let mut diagnostic = None;
    loop {
        cancellation.check()?;
        ensure!(
            Instant::now() < deadline,
            "1Password did not finish before the deadline; authenticate with the desktop app and retry"
        );
        receive(&stdout, &mut output)?;
        receive(&stderr, &mut diagnostic)?;
        if output.is_some() && diagnostic.is_some() {
            if let Some(status) = owned
                .child
                .try_wait()
                .map_err(|_| anyhow::anyhow!("cannot observe the 1Password process"))?
            {
                owned.reaped = true;
                ensure!(
                    status.success(),
                    "1Password metadata retrieval failed; check the explicit account, vault, Document and desktop authorization"
                );
                return Ok(output.take().expect("received output"));
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
}

pub fn default_executable() -> std::path::PathBuf {
    std::env::var_os("AWS_PROFILE_SELECTOR_OP_EXECUTABLE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "op".into())
}

pub fn overall_deadline() -> Instant {
    Instant::now() + Duration::from_secs(300)
}
