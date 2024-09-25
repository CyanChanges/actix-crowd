use std::sync::Arc;
use actix_crowd::context::Cortex;

#[tokio::main]
async fn main() {
    color_eyre::install().ok();
    let cortex = Cortex::new(Arc::new(()));
    let scope = cortex.plug(|cortex: Arc<Cortex>| {
        println!("Hello World");
        cortex.scope.dispose();
        Ok(())
    }, ()).expect("Cortex::plug failed");
    cortex.run().await
}