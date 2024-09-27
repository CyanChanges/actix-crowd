use std::sync::Arc;
use mockall::mock;
use crate::context::Cortex;
use crate::events::{EventMatcher, UserEvent};
use crate::prelude::EventMessage;


#[tokio::test]
async fn test_plug() {
    let cortex = Cortex::new(Arc::new(()));
    let _ = cortex.plug(async |cortex: Arc<Cortex>| {
        println!("Hello World");
        cortex.scope.dispose();
        Ok(())
    }, ());
    cortex.run().await
}

// #[tokio::test]
// async fn test_serial() {
//     let cortex = Cortex::new(Arc::new(()));
//     let _ = cortex.plug(async move |cortex: Arc<Cortex>| {
//         println!("Hello World");
//         cortex.emit(UserEvent::new("test/serial", ())).await
//             .unwrap();
//         Ok(())
//     }, ());
//     let received = Arc::new(AtomicBool::new(false));
//     let received_clone = received.clone();
//     cortex.on(EventMatcher::UserEvent("test/serial"), async move |args: ()| {
//         received_clone.store(true, Ordering::SeqCst);
//     });
//     if !received.load(Ordering::SeqCst) {
//         panic!("event serial failed");
//     }
//     cortex.run().await;
// }