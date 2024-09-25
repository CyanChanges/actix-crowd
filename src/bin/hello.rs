#![feature(async_closure)]
use std::sync::Arc;
use actix_crowd::prelude::*;

#[tokio::main]
async fn main() {
    color_eyre::install().ok();
    let cortex = Cortex::new(Arc::new(()));
    let _scope = cortex.plug(async move |cortex: Arc<Cortex>| {
        println!("Hello World");
        cortex.scope.dispose();
        Ok(())
    }, ()).expect("failed to `Cortex::plug` the plugin");
    cortex.run().await
}