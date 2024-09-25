use std::cell::UnsafeCell;
use std::future::Future;
use std::iter;
use std::pin::Pin;
use std::sync::{Arc, Once};
use std::sync::atomic::AtomicUsize;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

pub struct Tasker {
    sender: flume::Sender<Task>,
    workers: Vec<Arc<Worker>>,
    notifier: Pin<Arc<Notify>>,
}

unsafe impl Sync for Tasker {}

impl Tasker {
    pub(crate) fn new(workers: u8) -> Self {
        let (tx, rx) = flume::unbounded();
        let notifier = Arc::pin(Notify::new());
        let workers: Vec<_> = iter::repeat(()).take(workers.into())
            .map(|_| Worker::new(rx.clone(), notifier.clone()).started())
            .collect();
        Self { notifier, sender: tx, workers }
    }

    pub fn sched(&self, task: Task) {
        self.sender.send(task).unwrap();
    }

    pub fn sched_many(&self, tasks: impl Iterator<Item=Task>) {
        tasks.for_each(|task| self.sender.send(task).unwrap())
    }

    pub(crate) fn dispose(&self) {
        self.notifier.notify_waiters();
    }
}

pub struct Task {
    name: Arc<str>,
    fut: Box<dyn Future<Output=()> + Unpin + Send + Sync>,
    blocking: bool,
}

impl Task {
    pub fn new(name: impl Into<Arc<str>>, fut: impl Future<Output=()> + Unpin + Send + Sync + 'static) -> Self {
        Task {
            name: name.into(),
            fut: Box::new(fut),
            blocking: false,
        }
    }
    pub fn new_blocking(name: impl Into<Arc<str>>, fut: impl Future<Output=()> + Unpin + Send + Sync + 'static) -> Self {
        Task {
            name: name.into(),
            fut: Box::new(fut),
            blocking: true,
        }
    }
}

pub struct Worker {
    once: Once,
    receiver: flume::Receiver<Task>,
    process_count: AtomicUsize,
    notify: Pin<Arc<Notify>>,
    handler: UnsafeCell<Option<JoinHandle<()>>>,
}

unsafe impl Sync for Worker {}

impl<'a> Worker {
    fn new(receiver: flume::Receiver<Task>, notify: Pin<Arc<Notify>>) -> Self {
        Self {
            once: Once::new(),
            receiver,
            notify,
            process_count: AtomicUsize::new(0),
            handler: UnsafeCell::new(None),
        }
    }

    async fn handle_task(&self, t: Task) {
        if t.blocking {
            tokio::task::spawn(t.fut).await.unwrap();
        } else {
            t.fut.await
        }
    }

    async fn worker_receiver(&self) -> Result<!, ()> {
        loop {
            if self.receiver.is_disconnected() {
                return Err(());
            }
            match self.receiver.recv_async().await {
                Ok(t) => self.handle_task(t).await,
                Err(_) => Err(())?
            }

            self.process_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    async fn worker_joiner(&self) -> Result<!, ()> {
        self.notify.notified().await;
        Err(())
    }

    #[allow(unreachable_code)]
    fn start(self: Arc<Self>) {
        unsafe {
            self.clone().once.call_once(move || self.handler.get().write(Some(tokio::task::spawn(async move {
                let _ = tokio::try_join!(self.worker_receiver(), self.worker_joiner());
            }))))
        };
    }

    fn started(self) -> Arc<Self> {
        let this = Arc::new(self);
        this.clone().start();
        this
    }

    fn abort(&self) {
        if let Some(handler) = unsafe { &*self.handler.get() }.as_ref() {
            handler.abort();
        }
    }
}


impl Drop for Worker {
    fn drop(&mut self) {
        self.abort();
    }
}
