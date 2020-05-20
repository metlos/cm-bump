use log;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use regex::Regex;
use std::fs;
use std::path::Path;
use std::str;
use std::str::FromStr;
use thiserror::Error;

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
pub enum ProcessDetection {
    Cmdline(Regex),
    Pid(i32),
}

#[derive(Debug, Clone)]
struct ProcessDetector {
    detection: ProcessDetection,
    pid: Option<Pid>,
    parent: Option<Box<ProcessDetector>>,
}

#[derive(Debug, Clone)]
pub struct Bumper {
    process_tree: ProcessDetector,
    signal: Signal,
}

impl ProcessDetector {
    pub fn pid(&mut self) -> Option<Pid> {
        log::trace!("Determining pid for {:?}", self);
        let ppid = match self.parent {
            Some(ref mut parent) => {
                log::trace!("PPID required, checking...");
                match parent.pid() {
                    Some(ppid) => {
                        log::trace!("Will check if PPID is {}", ppid);
                        Some(ppid)
                    }
                    // we require a parent process but it could not be found. no point in continuing.
                    None => {
                        log::trace!("PPID require yet none found. Bailing.");
                        return None;
                    }
                }
            }
            None => {
                // no parent pid
                None
            }
        };

        log::trace!("Checking whether the current PID {:?} is valid", self.pid);

        if !self.valid() {
            log::trace!(
                "Current PID {:?} determined not valid. Trying to rediscover.",
                self.pid
            );
            self.pid = match self.find_pid() {
                Some(new_pid) if ppid.is_some() => match is_parent(&ppid.unwrap(), &new_pid) {
                    Ok(yes) => {
                        if yes {
                            log::trace!("New PID found to be {}", new_pid);
                            Some(new_pid)
                        } else {
                            log::trace!(
                                "Forgetting the candidate PID {} because PPID doesn't match.",
                                new_pid
                            );
                            None
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to determine parent process of PID {}: {}",
                            new_pid,
                            e
                        );
                        None
                    }
                },
                Some(new_pid) => {
                    log::trace!("New PID found to be {}", new_pid);
                    Some(new_pid)
                }
                _ => {
                    log::trace!("Could not find PID matching the criteria.");
                    None
                }
            };
        }

        log::trace!("The PID is {:?}", self.pid);

        self.pid
    }

    fn find_pid(&self) -> Option<Pid> {
        match self.detection {
            ProcessDetection::Cmdline(ref regex) => match scan_proc(&regex) {
                Ok(res) => res,
                Err(e) => {
                    log::error!(
                        "Failed to scan the process list for process matching {}: {}",
                        regex,
                        e
                    );
                    None
                }
            },
            ProcessDetection::Pid(ref pid) => {
                if *pid == 0 {
                    // special case - PID 0 is mainly useful for specifying PPID of an init-like process
                    // e.g. the command of a docker container for example. For this case, we always match
                    // PID 0 successfully.
                    Some(Pid::from_raw(*pid))
                } else {
                    let pid = Pid::from_raw(*pid);
                    if ProcessDetector::pid_exists(&pid) {
                        log::trace!("The required PID {} found.", pid);
                        Some(pid)
                    } else {
                        log::trace!("The required PID {} NOT found.", pid);
                        None
                    }
                }
            }
        }
    }

    fn valid(&self) -> bool {
        log::trace!("Checking whether the current PID {:?} is valid.", self.pid);
        match self.pid {
            Some(pid) => match self.detection {
                ProcessDetection::Cmdline(ref regex) => {
                    match parse_cmdline(format!("/proc/{}/cmdline", pid)) {
                        Ok(cmdline) => {
                            log::trace!(
                                "Checking whether the cmdline `{}` matches regex `{:?}`",
                                cmdline,
                                regex
                            );
                            regex.is_match(&cmdline)
                        }
                        Err(e) => {
                            log::warn!("Failed to detect if process {} is still valid: {}", pid, e);
                            false
                        }
                    }
                }
                ProcessDetection::Pid(ref expected_pid) => {
                    if pid.as_raw() == *expected_pid {
                        log::trace!("Checking whether the required PID {} exists", expected_pid);
                        ProcessDetector::pid_exists(&pid)
                    } else {
                        log::trace!(
                            "Current PID {} is different from the required PID {}.",
                            pid,
                            expected_pid
                        );
                        false
                    }
                }
            },
            None => false,
        }
    }

    fn pid_exists(pid: &Pid) -> bool {
        Path::new(&format!("/proc/{}", pid)).exists()
    }
}

impl Bumper {
    pub fn new(process_tree: Vec<ProcessDetection>, signal: &str) -> Result<Self> {
        if process_tree.is_empty() {
            return Err(Error::InitError(
                "At least 1 process detection needs to be defined.".into(),
            ));
        }

        let first = ProcessDetector {
            detection: process_tree.get(0).unwrap().clone(),
            pid: None,
            parent: None,
        };

        let process_tree = process_tree
            .iter()
            .skip(1)
            .fold(first, |detector, detection| ProcessDetector {
                detection: detection.clone(),
                pid: None,
                parent: Some(Box::from(detector)),
            });

        Ok(Bumper {
            process_tree: process_tree,
            signal: Signal::from_str(signal).map_err(|e| Error::InitError(format!("{}", e)))?,
        })
    }

    pub fn bump(&mut self) -> Result<()> {
        match self.process_tree.pid() {
            Some(pid) => {
                log::debug!("Sending signal {:?} to process {:?}", self.signal, pid);
                signal::kill(pid, self.signal).map_err(|e| Error::SignalError(format!("{}", e)))
            }
            _ => {
                log::info!("No process of the configured name found running. Bump has no effect.");
                Ok(())
            }
        }
    }
}

fn scan_proc(proc_cmd: &Regex) -> Result<Option<Pid>> {
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
            comm_path.push("cmdline");
            let comm = parse_cmdline(comm_path)?;

            log::trace!(
                "Checking `{}` with cmdline `{}`",
                e.file_name().to_string_lossy(),
                comm
            );

            if proc_cmd.is_match(&comm) {
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
        .filter(|r| if let Ok(Some(_)) = r { true } else { false })
        .next()
        .unwrap_or(Ok(None))
}

fn proc_error(e: &dyn ToString) -> Error {
    Error::ProcError(e.to_string())
}

fn parse_cmdline<P: AsRef<Path>>(e: P) -> Result<String> {
    let bytes = fs::read(e).map_err(|e| proc_error(&e))?;
    // the cmdline is \0 separated, so we need to convert
    let cmdline =
        bytes
            .split(|b| *b == 0)
            .map(|a| str::from_utf8(a))
            .fold(String::new(), |mut acc, s| {
                if !acc.is_empty() {
                    acc.push_str(" ");
                }
                acc.push_str(s.unwrap_or(""));
                acc
            });
    Ok(cmdline.trim().to_string())
}

fn is_parent(ppid: &Pid, new_pid: &Pid) -> Result<bool> {
    let stat =
        fs::read_to_string(format!("/proc/{}/stat", *new_pid)).map_err(|e| proc_error(&e))?;
    match stat.rfind(") ") {
        Some(last_paren) => {
            let mut splits = stat.split_at(last_paren + 2).1.split(" ");
            splits.next();
            let found_ppid = splits.next();
            match found_ppid {
                Some(found_ppid) => match found_ppid.parse::<i32>() {
                    Ok(found_ppid) => Ok(ppid.as_raw() == found_ppid),
                    Err(e) => {
                        log::error!(
                            "Could not parse ppid {} as a number, weird: {}",
                            found_ppid,
                            e
                        );
                        Ok(false)
                    }
                },
                None => Ok(false),
            }
        }
        None => Ok(false),
    }
}

mod test {
    #[test]
    fn test_stat_parsing() {
        // an executable with a ')' in its name... yuck!
        let stat = "327321 (cm)bump) S 135114 327321 135114 34824 327321 1077936128 3274 0 0 0 21 0 0 0 20 0 9 0 3568036 657534976 6792 18446744073709551615 94252542341120 94252554575081 140734720655776 0 0 0 0 4096 1088 0 0 0 17 6 0 0 0 0 0 94252558218784 94252559146553 94252566843392 140734720662251 140734720662323 140734720662323 140734720667630 0";
        match stat.rfind(") ") {
            Some(last_paren) => {
                let splits: Vec<&str> = stat.split_at(last_paren + 2).1.split(" ").collect();
                assert_eq!("135114", splits[1])
            }
            None => {
                println!("rfind failed.");
            }
        }
    }
}
