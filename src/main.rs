use k8s_openapi::api::core::v1::ConfigMap;
use kube::{
    api::{Api, ListParams},
    Client,
};
use log;
use pretty_env_logger::formatted_timed_builder;
use std::env;

mod operator;
mod updater;
mod bumper;

const LOG_ENV_VAR: &str = "CM_LOG";

fn get_env_or_exit(env_var: &str) -> String {
    match env::var(env_var) {
        Ok(v) => v,
        Err(_) => {
            log::error!("{} environment variable not defined.", env_var);
            std::process::exit(1);
        }
    }
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

    log::info!("cm-bump starting");

    let dir = get_env_or_exit("CM_DIR");
    let ns = get_env_or_exit("CM_NAMESPACE");
    let labels = get_env_or_exit("CM_LABELS");
    let proc_command = env::var("CM_PROC_COMM").ok();
    let signal = env::var("CM_PROC_SIGNAL").ok();

    let client = Client::try_default().await?;
    let cms: Api<ConfigMap> = Api::namespaced(client, &ns);
    let lp = ListParams::default().labels(&labels);

    let bumper = if proc_command.is_some() && signal.is_some() {
        Some(bumper::Bumper::new(&proc_command.unwrap(), &signal.unwrap())?)
    } else {
        None
    };

    let op = match updater::ConfigUpdater::new(&dir, bumper) {
        Ok(cu) => cu,
        Err(e) => {
            log::error!("{}", e);
            anyhow::bail!("{}", e)
        }
    };

    operator::run(cms, op, lp).await?;

    Ok(())
}
