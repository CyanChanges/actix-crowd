use std::cell::UnsafeCell;
use std::mem::{ManuallyDrop};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Once;

union Data<T> {
    value: ManuallyDrop<T>,
    f: ManuallyDrop<Box<dyn FnOnce() -> T>>,
}

pub(crate) struct LateInit<T> {
    once: Once,
    data: UnsafeCell<Data<T>>,
}

impl<T: Default> Default for LateInit<T> {
    fn default() -> Self {
        Self::new(T::default)
    }
}

impl<T> LateInit<T> {
    pub fn new<F: FnOnce() -> T + Send + Sync>(func: F) -> Self {
        let boxed: Box<dyn FnOnce() -> T + Send + Sync> = Box::new(func);
        let leaked = Box::leak(boxed);
        // It's fine you know
        let ptr = cve_rs::transmute::<&mut (dyn FnOnce() -> T + Send + Sync), *mut (dyn FnOnce() -> T + Send + Sync)>(leaked);

        LateInit {
            once: Once::new(),
            data: UnsafeCell::new(Data::<T> { f: ManuallyDrop::new(unsafe { Box::from_raw(ptr) }) }),
        }
    }

    pub fn ensure_initialized(&self) {
        if !self.once.is_completed() {
            self.once.call_once(|| unsafe {
                let data = &mut *self.data.get();
                let val = ManuallyDrop::take(&mut data.f)();
                data.value = ManuallyDrop::new(val);
            })
        }
    }


    pub unsafe fn get_ref_unchecked(&self) -> &T {
        &(*self.data.get()).value
    }

    #[allow(clippy::mut_from_ref)]
    pub unsafe fn get_mut_unchecked(&self) -> &mut T {
        &mut (*self.data.get()).value
    }

    pub unsafe fn get_ptr_unchecked(&self) -> *mut T {
        &(*self.data.get()).value as *const _ as *mut _
    }


    pub(crate) unsafe fn hack_state(&self) -> ExclusiveState {
        const INCOMPLETE: u32 = 0;
        /// Some thread has previously attempted to initialize the Once, but it panicked,
        /// so the Once is now poisoned. There are no other threads currently accessing
        /// this Once.
        const POISONED: u32 = 1;
        /// Some thread is currently attempting to run initialization. It may succeed,
        /// so all future threads need to wait for it to finish.
        const RUNNING: u32 = 2;
        /// Some thread is currently attempting to run initialization and there are threads
        /// waiting for it to finish.
        const QUEUED: u32 = 3;
        /// Initialization has completed and all future calls should finish immediately.
        const COMPLETE: u32 = 4;

        // im totally not steal the data =)
        let ptr = &self.once as *const _ as *mut AtomicU32;
        let val = &*ptr;
        match val.load(Ordering::Acquire) {
            INCOMPLETE => ExclusiveState::Incomplete,
            POISONED => ExclusiveState::Poisoned,
            COMPLETE => ExclusiveState::Complete,
            _ => unreachable!("illegal state, is your operating system a off-brand?"),
        }
    }
}

pub(crate) enum ExclusiveState {
    Incomplete,
    Poisoned,
    Complete,
}

unsafe impl<T: Send> Send for LateInit<T> {}
unsafe impl<T: Sync> Sync for LateInit<T> {}

impl<T> Deref for LateInit<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.ensure_initialized();
        unsafe { self.get_ref_unchecked() }
    }
}

impl<T> DerefMut for LateInit<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ensure_initialized();
        unsafe { self.get_mut_unchecked() }
    }
}


impl<T> Drop for LateInit<T> {
    fn drop(&mut self) {
        unsafe {
            match self.hack_state() {
                ExclusiveState::Incomplete => ManuallyDrop::drop(&mut self.data.get_mut().f),
                ExclusiveState::Complete => ManuallyDrop::drop(&mut self.data.get_mut().value),
                ExclusiveState::Poisoned => {}
            }
        }
    }
}
