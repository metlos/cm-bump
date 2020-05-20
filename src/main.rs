use core::convert::TryFrom;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::{
    api::{Api, ListParams},
    config::Config,
    Client,
};
use log;
use pretty_env_logger::formatted_timed_builder;
use regex::Regex;
use std::env;
use std::str::FromStr;
use structopt::StructOpt;

mod bumper;
mod operator;
mod updater;

const LOG_ENV_VAR: &str = "CM_LOG";

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
struct Opts {
    /// The directory to which persist the files retrieved from config maps.
    #[structopt(short, long, env = "CM_DIR")]
    dir: String,

    /// The namespace in which to look for the config maps to persist.
    #[structopt(short, long, env = "CM_NAMESPACE")]
    namespace: String,

    /// Whether to require valid certificate chain. True by default.
    #[structopt(short, long, env = "CM_TLS_VERIFY")]
    tls_verify: Option<bool>,

    /// An expression to match the labels against. Consult the Kubernetes documentation for the
    /// syntax required.
    #[structopt(short, long, env = "CM_LABELS")]
    labels: String,

    /// The commandline by which to identify the process to send the signal to. This can be a regular expression.
    /// Ignored if process pid is specified.
    #[structopt(short = "c", long, env = "CM_PROC_CMD")]
    process_command: Option<String>,

    /// The PID of the process to send the signal to, if known. Otherwise process detection can be used.
    #[structopt(short = "p", long, env = "CM_PROC_PID")]
    process_pid: Option<i32>,

    /// The commandline by which to identify the parent process of the process to send signal to. This can be a regular expression.
    /// Ignored if parent process pid is specified.
    #[structopt(short = "a", long, env = "CMD_PROC_PARENT_CMD")]
    process_parent_command: Option<String>,

    /// The PID of the parent process of the process to send the signal to, if known. Otherwise process detection can be used.
    #[structopt(short = "i", long, env = "CMD_PROC_PARENT_PID")]
    process_parent_pid: Option<i32>,

    /// The name of the signal to send to the process on the configuration files change.
    /// Use `kill -l` to get a list of possible signals and prepend it with "SIG". E.g. "SIGHUP", "SIGKILL", etc.
    #[structopt(short, long, env = "CM_PROC_SIGNAL")]
    signal: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if env::var(LOG_ENV_VAR).is_err() {
        //std::env::set_var("LOG_ENV_VAR", "info,cm_bump=trace,kube=trace");
        env::set_var(LOG_ENV_VAR, "info,kube=warn");
    }

    formatted_timed_builder()
        .parse_filters(&env::var(LOG_ENV_VAR).unwrap_or("info,kube=warn".into()))
        .init();

    let opt: Opts = StructOpt::from_args();

    log::info!("cm-bump starting");

    let mut client_config = Config::infer().await?;
    client_config.accept_invalid_certs = !opt.tls_verify.unwrap_or(true);

    let client = Client::try_from(client_config)?;
    let cms: Api<ConfigMap> = Api::namespaced(client, &opt.namespace);
    let lp = ListParams::default().labels(&opt.labels);

    let bumper = match bumper_config(&opt) {
        Some((detection, signal)) => {
            log::info!("Bumper will look for processes matching hierarchy `{:?}` and send `{}` to it on config change.", detection, signal);
            Some(bumper::Bumper::new(detection, &signal)?)
        }
        None => {
            log::info!("Bumper not configured.");
            None
        }
    };

    let op = match updater::ConfigUpdater::new(&opt.dir, bumper) {
        Ok(cu) => cu,
        Err(e) => {
            log::error!("{}", e);
            anyhow::bail!("{}", e)
        }
    };

    operator::run(cms, op, lp).await?;

    Ok(())
}

fn bumper_config(opts: &Opts) -> Option<(Vec<bumper::ProcessDetection>, String)> {
    match opts.signal {
        Some(ref signal) => {
            let mut ret = vec![];
            let parent_process = process_detection_config(
                &opts.process_parent_command,
                &opts.process_parent_pid,
                "the parent",
            );
            let process = process_detection_config(&opts.process_command, &opts.process_pid, "the");

            if parent_process.is_some() {
                ret.push(parent_process.unwrap());
            }

            if process.is_some() {
                ret.push(process.unwrap());
            }

            Some((ret, signal.clone()))
        }
        None => None,
    }
}

fn process_detection_config(
    cmd: &Option<String>,
    pid: &Option<i32>,
    adjective: &str,
) -> Option<bumper::ProcessDetection> {
    match pid {
        Some(pid) => {
            if cmd.is_some() {
                log::warn!("Ignoring {} process command configuration `{}` because {} PID `{}` has been specified.", 
                    adjective, adjective, cmd.clone().unwrap(), pid);
            }
            Some(bumper::ProcessDetection::Pid(*pid))
        }
        None => match cmd {
            Some(cmd) => match Regex::from_str(&cmd) {
                Ok(regex) => Some(bumper::ProcessDetection::Cmdline(regex)),
                Err(e) => {
                    log::error!("Failed to parse {} as a regular expression. Exitting.", e);
                    std::process::exit(1);
                }
            },
            None => None,
        },
    }
}
