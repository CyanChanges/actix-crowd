use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Weak};
use dashmap::DashMap;
use crate::any::KAny;
use crate::context::{Cortex, MainScope, Scope};
use crate::plugin::{Id, Plug, Plugin};
use crate::pnp::Pluggable;

pub(crate) struct Counter {
    counter: AtomicUsize,
}

impl Default for Counter {
    fn default() -> Self {
        Self {
            counter: AtomicUsize::new(1)
        }
    }
}

impl Counter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fetch(&self) -> usize {
        self.counter.fetch_add(1, Relaxed)
    }
}

pub struct Registry {
    pub(crate) context: Weak<Cortex>,
    pub(crate) counter: Counter,
    pub(crate) entries: DashMap<Id, Weak<MainScope>>,
}

impl Registry {
    pub fn new(ctx: Weak<Cortex>, config: Arc<impl KAny>) -> Self {
        Self {
            context: ctx,
            counter: Counter::default(),
            entries: DashMap::new(),
        }
    }

    pub fn get(&self, plugin: &dyn Plugin) -> Option<Arc<MainScope>> {
        let identifier = plugin.identifier();
        let runtime = self.entries.get(&identifier);
        runtime.and_then(|opt| opt.upgrade())
    }

    pub(crate) fn set(&self, plugin: &dyn Plugin, state: Weak<MainScope>) {
        let identifier = plugin.identifier();
        let _ = self.entries.insert(identifier, state);
    }

    pub(crate) fn delete(&self, plugin: &dyn Plugin) -> bool {
        let identifier = plugin.identifier();
        match self.entries.remove(&identifier) {
            None => false,
            Some((_, rt)) =>{
                let rt = match rt.upgrade() {
                    None => return false,
                    Some(rt) => rt
                };
                rt.dispose();
                true
            }
        }
    }

    fn ctx(&self) -> Arc<Cortex> {
        self.context.upgrade().unwrap()
    }

    pub fn plugin<T: KAny + std::panic::UnwindSafe>(&self, pluggable: impl Pluggable<T> + 'static, config: T) -> Arc<Scope> {
        let plugged = Plug::new(pluggable);
        match self.get(&plugged) {
            None => {
                let shared = Arc::new(config);
                let rt = MainScope::new(self.ctx(), Some(plugged));
                self.set(rt.plugin().unwrap(), Arc::downgrade(&rt));
                rt.fork(self.ctx(), shared)
            }
            Some(rt) if rt.alone => {
                // serial
                todo!("serial warning");
            }
            Some(rt) => {
                rt.fork(self.ctx(), Arc::new(config))
            }
        }
    }
}