use std::fs;
use std::path::Path;
use thiserror::Error;
use nix::unistd::Pid;
use nix::sys::signal::{self, Signal};
use std::str::FromStr;
use log;

#[derive(Debug, Clone, Error)]
pub enum Error {
    #[error("Initialization error: {0}")]
    InitError(String),

    #[error("Process reading error: {0}")]
    ProcError(String),

    #[error("Process signalling error: {0}")]
    SignalError(String),
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct Bumper {
    proc_comm: String,
    signal: Signal,
    pid: Option<Pid>,
}

impl Bumper {
    pub fn new(proc_comm: &str, signal: &str) -> Result<Self> {
        Ok(Bumper {
            proc_comm: proc_comm.to_owned(),
            signal: Signal::from_str(signal).map_err(|e| Error::InitError(format!("{}", e)))?,
            pid: None,
        })
    }

    fn ensure_pid(&mut self) -> Result<()> {
        match self.pid {
            Some(pid) => {
                // make sure the pid is still for the process we want
                let comm = trimmed_contents(format!("/proc/{}/comm", pid))?;
                if comm != self.proc_comm {
                    self.pid = scan_proc(&self.proc_comm)?;
                    log::debug!("Process restarted. New PID is {:?}", self.pid);
                }
            }
            None => {
                self.pid = scan_proc(&self.proc_comm)?;
                log::debug!("Process PID determined to be {:?}", self.pid);
            }
        }

        Ok(())
    }

    pub fn bump(&mut self) -> Result<()> {
        self.ensure_pid()?;

        match self.pid {
            Some(pid) => {
                log::debug!("Sending signal {:?} to process {:?}", self.signal, self.pid);
                signal::kill(pid, self.signal).map_err(|e| Error::SignalError(format!("{}", e)))
            },
            _ => {
                log::info!("No process of the configured name found running. Bump has no effect.");
                Ok(())
            }
        }
    }
}

fn scan_proc(proc_comm: &str) -> Result<Option<Pid>> {
    std::fs::read_dir("/proc")
        .map_err(|e| proc_error(&e))?
        .map(|e| {
            log::trace!("Inspecting {:?}", e);
            e
        })
        .filter_map(|r| r.ok())
        .filter(|e| {
            // check if the directory can be parsed as a number - that would be a pid of a process
            e.path()
                .file_name()
                .map(|f| f.to_str().map(|f| f.to_string()))
                .flatten()
                .filter(|f| f.parse::<u16>().is_ok())
                .is_some()
        })
        .map(|e| {
            // now see if the comm of the process is what we're looking for
            let mut comm_path = e.path();
            comm_path.push("comm");
            let comm = trimmed_contents(comm_path)?;
            
            log::trace!("Checking `{}` with command `{}`", e.file_name().to_string_lossy(), comm);

            if &comm == proc_comm {
                log::trace!("Matched {}.", comm);
                e.file_name()
                    .to_str()
                    .map(|f| {
                        f.parse::<i32>()
                            .map(|pid| Some(Pid::from_raw(pid)))
                            .map_err(|e| proc_error(&e))
                    })
                    .unwrap_or(Ok(None))
            } else {
                log::trace!("{} doesn't match.", comm);
                Ok(None)
            }
        })
        .filter(|r| {
            if let Ok(Some(_)) = r {
                true
            } else {
                false
            }
        })
        .next()
        .unwrap_or(Ok(None))
}

fn proc_error(e: &dyn ToString) -> Error {
    Error::ProcError(e.to_string())
}

fn trimmed_contents<P: AsRef<Path>>(e: P) -> Result<String> {
    let ret = fs::read_to_string(e).map_err(|e| proc_error(&e))?.trim().to_string();
    Ok(ret)
}
