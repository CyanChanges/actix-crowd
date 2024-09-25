use std::cell::UnsafeCell;
use std::cmp::PartialEq;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const UNINIT: u32 = 0;
const INITIALIZED: u32 = 1;
const UPDATING: u32 = 2;

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
pub enum LazyState {
    Uninit = 0,
    Initialized = 1,
    Updating = 2,
    // Nah, im too lazy to implement Poison state
}

pub struct LazyUpdate<T> {
    state: AtomicU32,
    mutex: Mutex<()>,
    updater: UnsafeCell<Box<dyn FnMut(Option<T>) -> T>>,
    value: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T: Send> Send for LazyUpdate<T> {}
unsafe impl<T: Sync> Sync for LazyUpdate<T> {}

fn u32_to_state(v: u32) -> LazyState {
    match v {
        UNINIT => LazyState::Uninit,
        INITIALIZED => LazyState::Initialized,
        UPDATING => LazyState::Updating,
        _ => unreachable!("invalid state")
    }
}

impl<T> LazyUpdate<T> {
    crate::construct_leaker!(leak, dyn FnMut(Option<T>) -> T, FnMut(Option<T>)->T);

    pub(crate) fn new<F: FnMut(Option<T>) -> T>(updater: F) -> Self {
        unsafe {
            LazyUpdate {
                state: AtomicU32::new(UNINIT),
                mutex: Mutex::new(()),
                value: UnsafeCell::new(MaybeUninit::uninit()),
                updater: UnsafeCell::new(unsafe { Self::leak(updater) }),
            }
        }
    }

    pub(crate) fn state(&self, ordering: Ordering) -> LazyState {
        u32_to_state(self.state.load(ordering))
    }

    pub(crate) fn update(&self) {
        let mut state = self.state(Ordering::Acquire);
        loop {
            match state {
                LazyState::Uninit | LazyState::Initialized => unsafe {
                    let _guard = self.mutex.lock();
                    if self.state(Ordering::Acquire) != LazyState::Uninit {
                        continue;
                    } else {
                        match self.state.compare_exchange_weak(state as u32, UPDATING, Ordering::Relaxed, Ordering::Acquire) {
                            Ok(_) => {}
                            Err(v) => {
                                state = u32_to_state(v);
                                continue;
                            }
                        }
                        let f = &mut *self.updater.get();
                        self.value.get().write(MaybeUninit::new(f(match state {
                            LazyState::Uninit => None,
                            LazyState::Initialized => Some(self.value.get().read().assume_init()),
                            LazyState::Updating => { unreachable!() }
                        })));
                        match self.state.compare_exchange_weak(UPDATING, UPDATING, Ordering::Relaxed, Ordering::Acquire) {
                            Ok(_) => break,
                            Err(_) => unreachable!()
                        }
                    }
                }
                LazyState::Updating => {
                    let _guard = self.mutex.lock();
                    break;
                }
            }
        }
    }

    pub(crate) fn force(this: &LazyUpdate<T>) {
        let mut state = this.state(Ordering::Acquire);
        loop {
            match state {
                LazyState::Uninit => unsafe {
                    let _guard = this.mutex.lock();
                    if this.state(Ordering::Acquire) != LazyState::Uninit {
                        continue;
                    } else {
                        match this.state.compare_exchange_weak(UNINIT, UPDATING, Ordering::Relaxed, Ordering::Acquire) {
                            Ok(_) => {}
                            Err(v) => {
                                state = u32_to_state(v);
                                continue;
                            }
                        }
                        let f = &mut *this.updater.get();
                        this.value.get().write(MaybeUninit::new(f(None)));
                        match this.state.compare_exchange_weak(UPDATING, UPDATING, Ordering::Relaxed, Ordering::Acquire) {
                            Ok(_) => break,
                            Err(_) => unreachable!()
                        }
                    }
                }
                LazyState::Initialized => break,
                LazyState::Updating => {
                    drop(this.mutex.lock());
                    break;
                }
            }
        }
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        let _guard = self.mutex.lock();
        unsafe { (*self.value.get()).assume_init_ref() }.clone()
    }
}



