use k8s_openapi::api::core::v1::ConfigMap;
use kube::{
    api::{Api, ListParams},
    Client,
    config::Config,
};
use log;
use pretty_env_logger::formatted_timed_builder;
use std::env;
use structopt::StructOpt;
use core::convert::TryFrom;

mod operator;
mod updater;
mod bumper;

const LOG_ENV_VAR: &str = "CM_LOG";

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
struct Opts {
    /// The directory to which persist the files retrieved from config maps.
    #[structopt(short,long, env = "CM_DIR")]
    dir: String,

    /// The namespace in which to look for the config maps to persist.
    #[structopt(short,long, env = "CM_NAMESPACE")]
    namespace: String,

    /// Whether to require valid certificate chain. True by default.
    #[structopt(short,long, env = "CM_TLS_VERIFY")]
    tls_verify: Option<bool>,

    /// An expression to match the labels against. Consult the Kubernetes documentation for the
    /// syntax required.
    #[structopt(short,long, env = "CM_LABELS")]
    labels: String,

    /// The commandline by which to identify the process to send the signal to. This can be regular expression.
    #[structopt(short,long, env = "CM_PROC_CMD")]
    process_command: Option<String>,

    /// The name of the signal to send to the process on the configuration files change.
    /// Use `kill -l` to get a list of possible signals and prepend it with "SIG". E.g. "SIGHUP", "SIGKILL", etc.
    #[structopt(short,long, env = "CM_PROC_SIGNAL")]
    signal: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if env::var(LOG_ENV_VAR).is_err() {
        //std::env::set_var("LOG_ENV_VAR", "info,cm_dump=trace,kube=trace");
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

    let bumper = if opt.process_command.is_some() && opt.signal.is_some() {
        let comm = opt.process_command.unwrap();
        let signal = opt.signal.unwrap();
        log::info!("Bumper will look for processes matching `{}` and send `{}` to it on config change.", comm, signal);
        Some(bumper::Bumper::new(&comm, &signal)?)
    } else {
        log::info!("Bumper not configured.");
        None
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
