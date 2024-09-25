use std::panic::UnwindSafe;
use std::sync::Arc;
use async_trait::async_trait;
use crate::context::Cortex;

pub enum Hot {
    ToRestart,
    Updated,
}

#[async_trait]
pub trait Pluggable<T>: Send + Sync + UnwindSafe {
    fn name() -> &'static str
    where
        Self: Sized;
    async fn apply(&self, cortex: Arc<Cortex>) -> color_eyre::Result<()>;
    async fn hot(&self, config: T) -> Result<Hot, ()>;
}

#[async_trait]
impl<F> Pluggable<()> for F
where
    F: Fn(Arc<Cortex>) -> color_eyre::Result<()> + 'static + Send + Sync + UnwindSafe,
    Self: Send + Sync,
{
    fn name() -> &'static str
    where
        Self: Sized,
    {
        "anonymous"
    }

    async fn apply(&self, cortex: Arc<Cortex>) -> color_eyre::Result<()> {
        self.call((cortex,))
    }

    async fn hot(&self, config: ()) -> Result<Hot, ()> {
        todo!()
    }
}