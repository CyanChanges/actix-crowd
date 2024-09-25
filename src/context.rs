use core::fmt;
use std::cell::UnsafeCell;
use std::fmt::Formatter;
use crate::any::KAny;
use crate::cat::Cat;
use crate::events::Handler;
use crate::plugin::Plugin;
use crate::pnp::Pluggable;
use crate::registry::Registry;
use crate::result;
use crate::utils::lazy::LazyUpdate;
use crate::utils::LateInit;
use actix::{Actor, Addr, Message};
use dashmap::{DashMap, DashSet};
use std::future::IntoFuture;
use std::hash::{Hash, Hasher};
use tokio::sync as concurrent;
use std::mem;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::panic::UnwindSafe;
use std::sync::{Arc, Mutex, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use crossbeam::atomic::AtomicCell;
use futures::{future, Future, FutureExt};
use crate::result::CrowdError;
use crate::tasker::{Task, Tasker};

const WORKER_COUNT: u8 = 1;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ScopeState {
    Pending,
    Active,
    Disposed,
    Failed,
}

pub struct MainScope {
    id: AtomicCell<Option<usize>>,
    name: Option<Arc<str>>,
    context: Weak<Cortex>,
    plugin: Option<Arc<dyn Plugin>>,
    children: DashSet<Arc<Scope>>,
    pub alone: bool,
}

impl fmt::Debug for MainScope {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.id.load().is_none() {
            return write!(f, "rt<uid=None>");
        }
        let uid = self.id.load().unwrap();
        match self.children.len() {
            0 => write!(f, "rt<id={uid} children=[]>"),
            1 => write!(f, "rt<id={uid} children=[{:?}]>", self.children.iter().next().unwrap().deref()),
            _ => write!(f, "rt<id={uid} children=[{:?}...{n} remains]>", self.children.iter().next().unwrap().deref(), n = self.children.len() - 1)
        }
    }
}

impl PartialEq for MainScope {
    fn eq(&self, other: &Self) -> bool {
        self.plugin == other.plugin &&
            self.id() == other.id()
    }
}

impl Eq for MainScope {}

impl MainScope {
    pub(crate) fn new(context: Arc<Cortex>, plugin: Option<impl Plugin + 'static>) -> Arc<MainScope> {
        Arc::new_cyclic(|weak| MainScope {
            id: AtomicCell::new(Some(context.registry.counter.fetch())),
            name: Some(Arc::from("root")),
            context: Arc::downgrade(&context),
            plugin: plugin.map(|plugin| Arc::new(plugin) as Arc<dyn Plugin + 'static>),
            children: DashSet::new(),
            alone: true,
        })
    }

    pub fn fork(self: Arc<MainScope>, parent: Arc<Cortex>, config: Arc<impl KAny + ?Sized>) -> Arc<Scope> {
        Scope::new(self.clone(), parent, config, WORKER_COUNT)
    }

    pub fn id(&self) -> usize {
        self.id.load().unwrap_or_else(|| usize::MAX)
    }

    pub fn plugin(&self) -> Option<&dyn Plugin> {
        self.plugin.as_ref().map(AsRef::as_ref)
    }

    pub fn dispose(self: Arc<MainScope>) {
        self.id.store(None)
        // TODO: reset
        // TODO: serial event
    }
}
pub struct Scope {
    lifecycle: Arc<Lifecycle>,
    runtime: Arc<MainScope>,
    context: Weak<Cortex>,
    config: Arc<dyn KAny>,
    handlers: Arc<DashMap<usize, Arc<dyn Handler>>>,
}

impl fmt::Debug for Scope {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Scope{lifecycle:?}", lifecycle = self.lifecycle)
    }
}

pub struct LifeStatus {
    mutex: Mutex<()>,
    is_active: UnsafeCell<bool>,
    error: UnsafeCell<Option<result::Error>>,
}

unsafe impl Send for LifeStatus {}
unsafe impl Sync for LifeStatus {}

impl LifeStatus {
    fn has_error(&self) -> bool {
        let _guard = self.mutex.lock();
        unsafe { self.error.get().as_ref() }.is_some()
    }

    fn is_active(&self) -> bool {
        let _guard = self.mutex.lock();
        unsafe { self.is_active.get().read() }
    }

    fn set_error(&self, err: result::Error) {
        let _guard = self.mutex.lock();
        unsafe { self.error.get().write(Some(err)); }
    }
}

pub struct Lifecycle {
    uid: AtomicCell<Option<usize>>,
    state: LazyUpdate<ScopeState>,
    disposed: AtomicBool,
    notifier: concurrent::Notify,
    tasker: Tasker,
    status: LifeStatus,
    disposables: Vec<Box<dyn FnOnce() + Send + Sync>>,
}

impl fmt::Debug for Lifecycle {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.status.is_active() {
            write!(f, "<active uid={uid} state={state:?}>", uid = self.uid.load().unwrap(), state = self.state.get())
        } else if self.disposed.load(Ordering::Relaxed) {
            write!(f, "<disposed uid=None>")
        } else {
            write!(f, "<uid={uid} state={state:?}>", uid = self.uid.load().unwrap(), state = self.state.get())
        }
    }
}

impl Lifecycle {
    pub(crate) fn new(uid: usize, worker_count: u8) -> Arc<Self> {
        Arc::new_cyclic(|weak: &Weak<Lifecycle>| {
            let weak = weak.clone();
            Self {
                uid: AtomicCell::new(Some(uid)),
                state: LazyUpdate::new(move |prev| {
                    let this = weak.upgrade().unwrap();
                    if this.uid.load().is_none() { return ScopeState::Disposed; }
                    if this.status.has_error() { return ScopeState::Failed; }
                    if this.status.is_active() { return ScopeState::Active; }
                    ScopeState::Pending
                }),
                disposed: Default::default(),
                notifier: Default::default(),
                tasker: Tasker::new(worker_count),
                status: LifeStatus {
                    mutex: Mutex::new(()),
                    is_active: UnsafeCell::new(false),
                    error: UnsafeCell::new(None),
                },
                disposables: vec![],
            }
        })
    }

    pub(crate) fn notify_dispose(&self) {
        self.uid.store(None);
        self.disposed.store(true, Ordering::SeqCst);
        self.state.update();
        self.notifier.notify_waiters();
        self.tasker.dispose();
    }

    pub fn id(&self) -> Option<usize> {
        self.uid.load()
    }

    pub(crate) fn set_error(&self, err: result::Error) {
        self.status.set_error(err);
    }
}

impl Hash for Scope {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(self.id());
        state.write_usize(self.runtime.id());
    }
}

impl PartialEq for Scope {
    fn eq(&self, other: &Self) -> bool {
        self.runtime.plugin == other.runtime.plugin && self.id() == other.id()
    }
}

impl Eq for Scope {}


impl Scope {
    pub(crate) fn new<T: Send + Sync + ?Sized + 'static>(runtime: Arc<MainScope>, context: Arc<Cortex>, config: Arc<T>, worker_count: u8) -> Arc<Self> {
        let id = context.registry.counter.fetch();
        let mut this = Arc::new_cyclic(|weak| Self {
            context: Arc::downgrade(&context),
            config: Arc::new(config),
            runtime: runtime.clone(),
            handlers: Default::default(),
            lifecycle: Lifecycle::new(id, worker_count),
        });

        Scope::init(Arc::get_mut(&mut this).unwrap());
        // TODO: this.dispose = ...
        runtime.children.insert(this.clone());
        // TODO: add dispose to runtime.disposables
        // TODO: emit event
        this
    }

    pub fn start(this: &mut Self) {
        // if (!this.ready || this.isActive || this.uid === null) return true
        // this.isActive = true
        // this.updateStatus(() => this.hasError = false)

        if this.runtime.plugin.is_none() {
            return;
        }

        let rt = this.runtime.clone();
        let cortex = this.context.upgrade().unwrap();
        this.ensure(move || {
            // freak I do the freaking unsafe magic to make it unwind safe
            cve_rs::transmute::<Box<dyn Future<Output=Result<(), result::Error>> + Send + Unpin>,
                Box<dyn Future<Output=Result<(), result::Error>> + Send + Sync + UnwindSafe + Unpin>>(
                Box::new(rt.plugin.as_ref().unwrap().apply(cortex)
                    .map(|r| r.map_err(|e| result::Error::Other(e)))
                ))
        });
    }

    pub fn id(&self) -> usize {
        self.lifecycle.id().unwrap_or_else(|| usize::MAX)
    }

    pub fn ensure<F, Fut>(&self, callback: F)
    where
        F: FnOnce() -> Fut + 'static,
        F: Send + Sync,
        Fut: IntoFuture<Output=result::Result<()>>,
        <Fut as IntoFuture>::IntoFuture: Send + Sync + UnwindSafe,
    {
        let lifecycle = self.lifecycle.clone();
        let wrapped = async move {
            let fut: Result<result::Result<()>, Box<dyn std::any::Any + Send>> = callback().into_future().catch_unwind().await;
            match fut {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    lifecycle.set_error(err);
                }
                Err(e) => {
                    lifecycle.set_error(result::Error::PnpPanic("panic when executing plugin lifecycle".to_string()));
                    std::panic::resume_unwind(e);
                    // self.lifecycle.error.replace(e);
                }
            }
            lifecycle.state.update()
        };
        self.lifecycle.tasker.sched(Task::new_blocking(Arc::from("anonymous"), Box::pin(wrapped)))
        // const task = callback()
        //     .catch((reason) => {
        //         this.context.emit(this.ctx, 'internal/error', reason)
        //         this.cancel(reason)
        //     })
        //     .finally(() => {
        //         this.updateStatus(() => this.tasks.delete(task))
        //         this.context.events._tasks.delete(task)
        //     })
        // this.updateStatus(() => this.tasks.add(task))
        // this.context.events._tasks.add(task)
    }

    pub fn init(this: &mut Self) {
        Scope::start(this);
    }

    pub(crate) fn assert_active(&self) -> result::Result<(), CrowdError> {
        Ok(())
    }

    fn ctx(&self) -> Arc<Cortex> {
        self.context.upgrade().unwrap()
    }

    pub fn dispose(self: &Arc<Scope>) -> bool {
        let result = self.runtime.children.remove(self).is_some();
        self.lifecycle.notify_dispose();
        if self.runtime.children.is_empty() {
            if self.runtime.plugin.is_some() {
                self.ctx().registry.delete(self.runtime.plugin.as_ref().unwrap().deref());
            }
        }
        result
    }
}

pub struct Cortex {
    pub root: Weak<Cortex>,
    pub parent: Weak<Cortex>,
    pub scope: Arc<Scope>,
    pub registry: Arc<Registry>,
    actor: LateInit<Addr<Cat>>,
}

unsafe impl Send for Cortex {}
unsafe impl Sync for Cortex {}

impl PartialEq for Cortex {
    fn eq(&self, other: &Self) -> bool {
        self.scope.id() == other.scope.id()
    }
}

impl Eq for Cortex {}


impl Cortex {
    #[allow(invalid_value)]
    #[allow(clippy::uninit_assumed_init)]
    pub fn new(config: Arc<impl KAny>) -> Arc<Self> {
        let _ = tokio::runtime::Handle::try_current().expect("expect a tokio runtime");
        let mut ctx = Arc::new_cyclic(|weak| Cortex {
            root: weak.clone(),
            parent: weak.clone(),
            registry: Arc::new(Registry::new(weak.clone(), Arc::new(config.clone()))),
            scope: Arc::new(unsafe { MaybeUninit::uninit().assume_init() }),
            actor: LateInit::new(|| (Cat { cortex: weak.clone() }).start()),
        });
        let runtime = MainScope::new(ctx.clone(), None::<()>);
        let scope = Scope::new(runtime, ctx.clone(), config, WORKER_COUNT);
        let arc: *mut Cortex = Arc::as_ptr(&ctx) as *mut _;
        mem::forget(mem::replace(&mut (unsafe { arc.as_mut().unwrap() }).scope, scope));
        ctx
    }

    pub fn runtime(&self) -> &MainScope {
        &self.scope.runtime
    }

    pub fn plug<T: Send + Sync + 'static + std::panic::UnwindSafe>(&self, pluggable: impl Pluggable<T> + 'static, config: T) -> result::Result<Arc<Scope>> {
        self.scope.assert_active()?;
        Ok(self.registry.plugin(pluggable, config))
    }

    fn actor(&self) -> &Addr<Cat> {
        &self.actor
    }

    pub async fn serial<M, R>(&self, msg: M) -> color_eyre::Result<R>
    where
        M: Message<Result=color_eyre::Result<R>> + Send + 'static,
        R: Send + actix::dev::MessageResponse<Cat, M>,
    {
        let actor = self.actor();
        actor.send(msg).await?
    }

    pub async fn run(self: Arc<Cortex>) {
        while !self.runtime().children.is_empty() {
            tokio::task::yield_now().await
        }
    }
}

