use super::bumper::Bumper;
use super::operator;
use k8s_openapi::api::core::v1::ConfigMap;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct ConfigUpdater {
    dir: String,
    bumper: Option<Bumper>,
}

#[derive(Debug, Clone)]
pub struct ConfigFile {
    pub content: String,
    pub digest: String,
}

type ConfigFiles = BTreeMap<String, ConfigFile>;

impl ConfigUpdater {
    pub fn new(base_dir: &str, bumper: Option<Bumper>) -> Result<Self, operator::Error> {
        let base_dir = std::path::PathBuf::from(base_dir);
        let base_path = base_dir.to_string_lossy().to_string();
        let metadata = std::fs::metadata(base_dir.clone()).map_err(|e| {
            operator::Error::OperatorError(format!(
                "Failed to check the validity of the base directory `{}`: {}",
                base_path, e
            ))
        })?;

        if !metadata.is_dir() || metadata.permissions().readonly() {
            Err(operator::Error::OperatorError(format!(
                "The base directory `{}` needs to exist and be writable",
                base_path
            )))
        } else {
            match base_dir.to_str() {
                Some(p) => Ok(ConfigUpdater {
                    dir: p.to_owned(),
                    bumper: bumper,
                }),
                None => Err(operator::Error::OperatorError(format!(
                    "Base dir path `{}` is not valid UTF-8.",
                    base_path
                ))),
            }
        }
    }

    fn to_path(&self, file: &str) -> Box<std::path::Path> {
        let mut path = std::path::PathBuf::from(&self.dir);
        path.push(file);
        path.into_boxed_path()
    }
}

impl operator::Operator<ConfigMap, ConfigFiles> for ConfigUpdater {
    fn prepare(&self, cm: ConfigMap) -> ConfigFiles {
        let cm_name = cm
            .metadata
            .map(|m| m.name)
            .flatten()
            .unwrap_or_else(|| "<unknown>".into());

        log::debug!("Preparing config map {} for caching.", cm_name);

        let mut files = ConfigFiles::new();

        let mut sha = sha1::Sha1::new();

        if let Some(data) = cm.data {
            for (name, data) in data {
                log::debug!("Adding file {}", name);
                sha.reset();
                sha.update(data.as_bytes());

                let file = ConfigFile {
                    content: data,
                    digest: sha.digest().to_string(),
                };

                files.insert(name, file);
            }
        }

        log::debug!("Preparing config map {} for caching.", cm_name);

        files
    }

    fn reconcile(
        &mut self,
        old: Option<&ConfigFiles>,
        new: Option<&ConfigFiles>,
    ) -> Result<(), operator::Error> {
        if let Some(new_files) = new {
            // first let's delete all the files from old that are not in new
            if let Some(old_files) = old {
                for f in old_files.keys() {
                    if !new_files.contains_key(f) {
                        let path = self.to_path(f);
                        log::debug!("Deleting config file {:?}", path);
                        match std::fs::remove_file(path) {
                            Err(e) => {
                                log::error!(
                                    "Failed to delete a no longer required config file `{}`: {}",
                                    f,
                                    e
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }

            let mut sha = sha1::Sha1::new();

            // now let's create or update all the files that should be there according to the new config
            let mut updated = false;
            for (name, cfg) in new_files {
                let path = self.to_path(name);
                if path.exists() {
                    match std::fs::read(path.clone()) {
                        Ok(data) => {
                            sha.reset();
                            sha.update(&data);
                            let digest = sha.digest().to_string();
                            if digest == cfg.digest {
                                log::debug!(
                                    "Config file `{}` hasn't changed. Skipping update.",
                                    name
                                );
                                continue;
                            }
                        }
                        Err(e) => {
                            log::warn!("Will overwrite the config file `{}` forcefully because of failure to compute its checksum: {}", name, e);
                        }
                    }
                }

                match std::fs::write(path, cfg.content.as_bytes()) {
                    Ok(_) => {
                        log::debug!("Updated the config file `{}`", name);
                        updated = true;
                    }
                    Err(e) => {
                        log::error!("Failed to update the config file `{}`: {}", name, e);
                    }
                }
            }

            if updated {
                if let Some(ref mut b) = self.bumper {
                    b.bump()
                        .map_err(|e| operator::Error::OperatorError(format!("{}", e)))?;
                }
            }
        }
        Ok(())
    }
}
