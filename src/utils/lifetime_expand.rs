#[macro_export]
macro_rules! construct_leaker {
    ($ident:ident, $ty:ty, $($tr:tt)*) => {
            unsafe fn $ident<TVal: $($tr)*>(val: TVal) -> ::std::boxed::Box<$ty> {
                let boxed: ::std::boxed::Box<$ty> = ::std::boxed::Box::new(val);
                let leaked = ::std::boxed::Box::leak(boxed);
                let ptr = ::cve_rs::transmute::<&mut ($ty), *mut ($ty)>(leaked);
                ::std::boxed::Box::from_raw(ptr)
            }
    };
}