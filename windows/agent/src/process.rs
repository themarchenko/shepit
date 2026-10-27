//! One agent child process: spawning, a writer thread for stdin, a reader thread for stdout, the
//! Job object on Windows, and the deadline-aware wait the workers build on.

use crate::env::{open_stderr_log, STDERR_LOG_LIMIT};
use serde_json::Value;
use shepit_core::polish::claude_wire::LineBuffer;
use shepit_core::polish::pipeline::PolishError;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// How often a wait looks at the cancel flag and the clock.
const SLICE: Duration = Duration::from_millis(100);

pub struct SpawnSpec<'a> {
    pub executable: &'a Path,
    pub args: &'a [String],
    pub cwd: &'a Path,
    pub env: Vec<(String, String)>,
    pub stderr_log: &'a Path,
    pub stderr_label: &'a str,
}

pub enum WriterMsg {
    Data(Vec<u8>),
    /// Drop stdin so the child sees end of input.
    Close,
}

pub struct AgentProcess {
    child: Child,
    writer: Sender<WriterMsg>,
    #[cfg(windows)]
    job: Option<crate::job_win::Job>,
    killed: bool,
}

impl AgentProcess {
    /// Starts the child. `on_line` runs on the reader thread for every JSON object line of stdout
    /// (split on `\n`, trailing `\r` dropped, anything else skipped); `on_eof` runs once when
    /// stdout closes.
    pub fn spawn(
        spec: &SpawnSpec,
        mut on_line: impl FnMut(Value) + Send + 'static,
        on_eof: impl FnOnce() + Send + 'static,
    ) -> std::io::Result<AgentProcess> {
        let mut cmd = Command::new(spec.executable);
        // No user text ever goes into argv; std quotes `.cmd` targets itself.
        cmd.args(spec.args)
            .current_dir(spec.cwd)
            .env_clear()
            .envs(spec.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        match open_stderr_log(spec.stderr_log, STDERR_LOG_LIMIT, spec.stderr_label) {
            Some(file) => cmd.stderr(Stdio::from(file)),
            None => cmd.stderr(Stdio::null()),
        };
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(crate::creation_flags());
        }
        let mut child = cmd.spawn()?;
        #[cfg(windows)]
        let job = match crate::job_win::Job::with_child(&child) {
            Ok(job) => Some(job),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };

        let mut stdin = child.stdin.take().expect("stdin is piped");
        let (writer, wrx) = std::sync::mpsc::channel::<WriterMsg>();
        std::thread::Builder::new()
            .name("shepit.agent.stdin".into())
            .spawn(move || {
                while let Ok(WriterMsg::Data(bytes)) = wrx.recv() {
                    if stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_err() {
                        break;
                    }
                }
                // `stdin` drops here, which closes the pipe.
            })?;

        let mut stdout = child.stdout.take().expect("stdout is piped");
        std::thread::Builder::new()
            .name("shepit.agent.stdout".into())
            .spawn(move || {
                let mut lines = LineBuffer::new();
                let mut chunk = [0u8; 8192];
                loop {
                    match stdout.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => lines.push(&chunk[..n]).into_iter().for_each(&mut on_line),
                    }
                }
                lines.finish().into_iter().for_each(&mut on_line);
                on_eof();
            })?;

        Ok(AgentProcess {
            child,
            writer,
            #[cfg(windows)]
            job,
            killed: false,
        })
    }

    /// A handle other threads can write through.
    pub fn writer(&self) -> Sender<WriterMsg> {
        self.writer.clone()
    }

    /// Queues one line for stdin. False when the writer thread has already given up.
    pub fn send(&self, line: String) -> bool {
        self.writer.send(WriterMsg::Data(line.into_bytes())).is_ok()
    }

    pub fn is_alive(&mut self) -> bool {
        !self.killed && matches!(self.child.try_wait(), Ok(None))
    }

    /// The exit code once the child is gone, `-1` when it has none or does not finish quickly.
    pub fn exit_code(&mut self) -> i32 {
        for _ in 0..100 {
            match self.child.try_wait() {
                Ok(Some(status)) => return status.code().unwrap_or(-1),
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(_) => break,
            }
        }
        -1
    }

    /// Close stdin, kill the child, drop the job (which kills anything it started). Safe to repeat.
    pub fn kill(&mut self) {
        if self.killed {
            return;
        }
        self.killed = true;
        let _ = self.writer.send(WriterMsg::Close);
        let _ = self.child.kill();
        #[cfg(windows)]
        drop(self.job.take());
        let _ = self.child.wait();
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Waits for a message that `on_msg` turns into a result. Wakes every 100 ms to check the cancel
/// flag and the deadline, so a hung agent cannot hold the caller past `deadline`. A closed channel
/// means the reader ended without a result: the process is gone.
pub fn wait_for<M, T>(
    rx: &Receiver<M>,
    deadline: Instant,
    cancelled: &dyn Fn() -> bool,
    mut on_msg: impl FnMut(M) -> Option<Result<T, PolishError>>,
) -> Result<T, PolishError> {
    loop {
        if cancelled() {
            return Err(PolishError::Cancelled);
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(PolishError::Timeout);
        }
        match rx.recv_timeout(SLICE.min(deadline - now)) {
            Ok(msg) => {
                if let Some(result) = on_msg(msg) {
                    return result;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err(PolishError::Exited(-1)),
        }
    }
}
