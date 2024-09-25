use std::sync::{Arc, Weak};
use actix::{Actor, Handler, Message};
use actix::dev::MessageResponse;
use crate::context::Cortex;

pub(crate) struct Cat {
    pub(crate) cortex: Weak<Cortex>,
}

impl Cat {
    fn cortex(&self) -> Arc<Cortex> {
        self.cortex.upgrade().unwrap()
    }
}

impl<M, R> Handler<M> for Cat
where
    M: Message<Result=color_eyre::Result<R>>,
    R: MessageResponse<Cat, M>,
{
    type Result = R;

    fn handle(&mut self, msg: M, ctx: &mut Self::Context) -> Self::Result {
        // self.cortex().registry.
        todo!()
    }
}

impl Actor for Cat {
    type Context = actix::Context<Self>;
}
