use std::any::{Any, TypeId};
use std::hash::{Hash, Hasher};

pub trait KAny: Any + Send + Sync {
    fn tid(&self) -> u64;
}

impl dyn KAny {
    pub fn is<T: KAny>(&self) -> bool {
        tid::<T>() == self.tid()
    }

    pub fn downcast_ref<T: KAny + 'static>(&self) -> Option<&T> {
        if self.is::<T>() {
            unsafe { Some(self.downcast_ref_unchecked()) }
        } else {
            None
        }
    }
    pub fn downcast_mut<T: KAny + 'static>(&mut self) -> Option<&mut T> {
        if self.is::<T>() {
            unsafe { Some(self.downcast_mut_unchecked()) }
        } else {
            None
        }
    }

    /// # Safety
    /// the target type must match the origin type
    pub unsafe fn downcast_ref_unchecked<T: 'static>(&self) -> &T {
        &*(self as *const _ as *const T)
    }

    /// # Safety
    /// the target type must match the origin type
    pub unsafe fn downcast_mut_unchecked<T: 'static>(&mut self) -> &mut T {
        &mut *(self as *mut _ as *mut T)
    }

    pub fn downcast<T: KAny>(self: Box<Self>) -> Option<T> {
        if self.is::<T>() {
            unsafe { Some(*self.downcast_unchecked()) }
        } else {
            None
        }
    }

    /// # Safety
    /// the target type must match the origin type
    pub unsafe fn downcast_unchecked<T: KAny>(self: Box<Self>) -> Box<T> {
        debug_assert!(self.is::<T>());
        unsafe {
            let (raw, alloc): (*mut dyn KAny, _) = Box::into_raw_with_allocator(self);
            Box::from_raw_in(raw as *mut T, alloc)
        }
    }
}

fn tid<T: 'static>() -> u64 {
    struct MyHasher(u64);
    impl Hasher for MyHasher {
        fn finish(&self) -> u64 {
            unimplemented!()
        }

        fn write(&mut self, _: &[u8]) {
            unimplemented!();
        }

        #[inline]
        fn write_u64(&mut self, i: u64) {
            self.0 = i
        }
    }
    let mut hasher = MyHasher(0);
    TypeId::of::<T>().hash(&mut hasher);
    hasher.0
}

impl<T: 'static + Sync + Send> KAny for T {
    fn tid(&self) -> u64 {
        tid::<T>()
    }
}
