use kube::{
    runtime::Informer, api::{Api, Meta, ListParams, WatchEvent},
};
use log;
use serde::de::DeserializeOwned;
use thiserror::Error;
use futures::{StreamExt, TryStreamExt};

#[derive(Error, Debug)]
pub enum Error {
    #[error("Kubernetes error.")]
    KubernetesError(#[from] kube::Error),
    #[error("Logic error: {0}")]
    OperatorError(String),
}

/// The operator trait. Clients of this library implement this trait and pass it to the [run](run) method.
pub trait Operator<Incoming, Stored>
{
    fn prepare(&self, obj: Incoming) -> Stored;

    /// The operator reconsiles the state of the objects by implementing this method.
    /// If old is None, then the new object represents a newly created object, if new is None then the old represents an object
    /// that has been deleted.
    fn reconcile(&mut self, old: Option<&Stored>, new: Option<&Stored>) -> Result<(), Error>;
}

/// Runs the operator seeded with the list of the CR objects.
/// This method is blocking indefinitely unless interrupted by an error.
pub async fn run<Obj, Op, St>(api: Api<Obj>, operator: Op, params: ListParams) -> Result<(), Error>
where
    Obj: Clone + DeserializeOwned + Meta + PartialEq + std::fmt::Debug + Send + Sync,
    Op: Operator<Obj, St>,
{
    let inf = Informer::new(api).params(params);

    let mut operator_state = OperatorState::new(operator);

    loop {
        let mut stream = inf.poll().await?.boxed();
        while let Some(ev) = stream.try_next().await? {
            match ev {
                WatchEvent::Added(o) => {
                    match operator_state.on_create(o) {
                        Ok(_) => {}
                        Err(e) => log::error!("Failed to handle the creation of object: {}", e),
                    };
                }
                WatchEvent::Deleted(o) => {
                    match operator_state.on_delete(o) {
                        Ok(_) => {}
                        Err(e) => log::error!("Failed to handle the deletion of object: {}", e),
                    };
                }
                WatchEvent::Modified(o) => {
                    match operator_state.on_update(o) {
                        Ok(_) => {}
                        Err(e) => log::error!("Failed to handle the update of object: {}", e),
                    };
                }
                WatchEvent::Error(e) => {
                    if e.code == 410 {
                        // We're desynced because nothing happened for too long. This is handled by kube I believe...    
                    } else {
                        log::error!("Failed to watch objects: {}", e);
                    }
                },
                WatchEvent::Bookmark(_) => {
                    log::debug!("Received bookmark. Not handled.");
                }
            }
        }
    }
}

// private impls

type Objects<K> = std::collections::HashMap<String, K>;

/// Internal state of the operator.
struct OperatorState<Obj, Op, St>
where
    Obj: Clone + DeserializeOwned + Meta + PartialEq + std::fmt::Debug,
    Op: Operator<Obj, St> + Sized,
{
    objects: Objects<St>,
    operator: Op,
    _data: std::marker::PhantomData<Obj>,
}

impl<Obj, Op, St> OperatorState<Obj, Op, St>
where
    Obj: Clone + DeserializeOwned + Meta + PartialEq + std::fmt::Debug,
    Op: Operator<Obj, St>,
{
    fn new(operator: Op) -> OperatorState<Obj, Op, St> {
        let objs = Objects::new();

        OperatorState {
            operator: operator,
            objects: objs,
            _data: std::marker::PhantomData::default()
        }
    }

    /// Updates the internal state with the newly created object and let's the operator react as well.
    fn on_create(&mut self, object: Obj) -> Result<(), Error> {
        let name = object.name();
        let st = self.operator.prepare(object);
        match self.objects.insert(name.clone(), st) {
            Some(o) => {
                log::debug!("Received create message about an object we already know. Possible recovery from timeout.");
                self.operator.reconcile(Some(&o), Some(self.objects.get(&name).unwrap()))?;
                Ok(())
            },
            None => {
                log::debug!("Creating object: {}", name);
                self.operator
                    .reconcile(None, Some(self.objects.get(&name).unwrap()))?;
                log::debug!("Created object: {}", name);
                Ok(())
            }
        }
    }

    /// Updates the internal state with the freshly updated object and let's the operator react as well.
    fn on_update(&mut self, object: Obj) -> Result<(), Error> {
        let name = object.name();
        let st = self.operator.prepare(object);
        match self.objects.insert(name.clone(), st) {
            None => Err(Error::OperatorError(format!(
                "Received update message about an object not in cache: {}",
                name
            ))),
            Some(old) => {
                log::debug!("Updating object: {}", name);
                let new = self.objects.get(&name).unwrap();
                self.operator.reconcile(Some(&old), Some(new))?;
                log::debug!("Updated object: {}", name);
                Ok(())
            }
        }
    }

    /// Updates the internal state with the freshly deleted object and let's the operator react as well.
    fn on_delete(&mut self, object: Obj) -> Result<(), Error> {
        let name = object.name();
        match self.objects.remove(&name.clone()) {
            None => Err(Error::OperatorError(format!(
                "Received deletion message about an object not in cache: {}",
                name
            ))),
            Some(o) => {
                log::debug!("Deleting object: {}", name);
                self.operator.reconcile(Some(&o), None)?;
                log::debug!("Deleted object: {}", name);
                Ok(())
            }
        }
    }
}
