use std::panic::UnwindSafe;
use std::sync::Arc;
use async_trait::async_trait;
use crate::any::KAny;
use crate::context::Cortex;
use crate::pnp::{Hot, Pluggable};

pub type Id = u128;

#[async_trait]
pub trait Plugin: Send + Sync + UnwindSafe {
    fn name(&self) -> Arc<str>;
    async fn apply(&self, cortex: Arc<Cortex>) -> color_eyre::Result<()>;
    async fn hot(&self, config: Box<dyn KAny>) -> Result<Hot, ()>;
    fn identifier(&self) -> Id;
}

#[async_trait]
impl Plugin for () {
    fn name(&self) -> Arc<str> {
        Arc::from("root")
    }

    async fn apply(&self, cortex: Arc<Cortex>) -> color_eyre::Result<()> {
        Ok(())
    }

    async fn hot(&self, config: Box<dyn KAny>) -> Result<Hot, ()> {
        Ok(Hot::Updated)
    }

    fn identifier(&self) -> Id {
        0
    }
}

impl PartialEq for dyn Plugin {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name()
    }
}

pub(crate) struct Plug<T: KAny> {
    name: Arc<str>,
    inner: Box<dyn Pluggable<T>>,
    id: Id,
}

impl<T: KAny> Plug<T> {
    pub(crate) fn new<P: Pluggable<T> + 'static>(pluggable: P) -> Self {
        Plug {
            name: Arc::from(P::name()),
            id: P::apply as usize as u128,
            inner: Box::new(pluggable),
        }
    }
    pub(crate) fn type_id(&self) -> Id {
        self.id
    }
}

#[async_trait]
impl<T: KAny + Send + Sync + UnwindSafe> Plugin for Plug<T> {
    fn name(&self) -> Arc<str> {
        self.name.clone()
    }

    async fn apply(&self, cortex: Arc<Cortex>) -> color_eyre::Result<()> {
        self.inner.apply(cortex).await
    }

    async fn hot(&self, config: Box<dyn KAny>) -> Result<Hot, ()> {
        self.inner.hot(match config.downcast::<T>() {
            Some(val) => val,
            None => return Err(())
        }).await
    }

    fn identifier(&self) -> Id {
        self.id
    }
}