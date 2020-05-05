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

    let client = Client::try_default().await?;
    let cms: Api<ConfigMap> = Api::namespaced(client, &ns);
    let lp = ListParams::default().labels(&labels);

    let op = match updater::ConfigUpdater::new(&dir) {
        Ok(cu) => cu,
        Err(e) => {
            log::error!("{}", e);
            anyhow::bail!("{}", e)
        }
    };

    operator::run(cms, op, lp).await?;

    Ok(())
}
