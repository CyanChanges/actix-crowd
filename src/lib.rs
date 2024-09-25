#![feature(new_uninit)]
#![feature(unboxed_closures)]
#![feature(tuple_trait)]
#![feature(associated_type_defaults)]
#![feature(async_fn_traits)]
#![feature(async_closure)]
#![feature(fn_traits)]
#![feature(allocator_api)]
#![feature(ptr_metadata)]
#![feature(never_type)]
extern crate core;

pub mod pnp;
pub mod context;
pub mod any;
pub mod plugin;
mod registry;
mod events;
mod cat;
mod utils;
mod result;
mod tasker;

pub mod prelude {}

#[cfg(test)]
mod tests;
