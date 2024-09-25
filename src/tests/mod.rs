use std::sync::Arc;
use crate::context::Cortex;

#[tokio::test]
async fn test_app() {
    let cortex = Cortex::new(Arc::new(()));
    let _ = cortex.plug(move |cortex: Arc<Cortex>| {
        println!("Hello World");
        cortex.scope.dispose();
        Ok(())
    }, ());
    cortex.run().await
}