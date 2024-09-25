use std::any::TypeId;
use std::sync::Arc;
use async_trait::async_trait;
use crate::any::KAny;
use crate::context::{Cortex, MainScope, Scope, ScopeState};

#[derive(Clone, Eq, PartialEq)]
pub enum InternalEvent {
    Fork(Arc<Scope>),
    Runtime(Arc<MainScope>),
    State(Arc<Scope>, ScopeState),
    Trace(String),
    Info(String),
    Warn(String),
    Debug(String),
    Error(String),
    Service,
    Listener,
}


#[derive(Clone)]
pub enum BuiltinEvent {
    Fork(Arc<Cortex>, Arc<dyn KAny>),
    Ready,
    Dispose,
    Internal(InternalEvent),
}

impl PartialEq for BuiltinEvent {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (BuiltinEvent::Fork(cortex1, args1), BuiltinEvent::Fork(cortex2, args2)) =>
                cortex1 == cortex2 && Arc::ptr_eq(args1, args2),
            (BuiltinEvent::Ready, BuiltinEvent::Ready) => true,
            (BuiltinEvent::Dispose, BuiltinEvent::Dispose) => true,
            (BuiltinEvent::Internal(event1), BuiltinEvent::Internal(event2)) =>
                event1 == event2,
            _ => false
        }
    }
}

impl Eq for BuiltinEvent {}

#[derive(Clone, Eq)]
pub enum EventMessage {
    Builtin(BuiltinEvent),
    User(UserEvent),
}

#[derive(Clone)]
pub struct UserEvent(String, Arc<dyn KAny>);
impl UserEvent {
    pub fn new(name: impl Into<String>, args: impl KAny) -> Self {
        UserEvent(name.into(), Arc::new(args))
    }

    pub fn name(&self) -> &str {
        &self.0
    }

    pub fn args(&self) -> Arc<dyn KAny> {
        self.1.clone()
    }
}

impl PartialEq for UserEvent {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0 && Arc::ptr_eq(&self.1, &other.1)
    }
}

impl Eq for UserEvent {}

impl PartialEq for EventMessage {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (EventMessage::Builtin(event1), EventMessage::Builtin(event2)) => event1 == event2,
            (EventMessage::User(UserEvent(name1, args1)), EventMessage::User(UserEvent(name2, args2))) =>
                name1 == name2 && std::ptr::eq(args1, args2),
            _ => false,
        }
    }
}
impl EventMessage {
    /// Creates a new `EventMessage` wrapping the given `BuiltinEvent`.
    ///
    /// # Arguments
    ///
    /// * `evt`: The `BuiltinEvent` to wrap.
    pub(crate) fn internal(evt: BuiltinEvent) -> EventMessage {
        EventMessage::Builtin(evt)
    }

    /// Creates a new `EventMessage` representing a user event with the given name and arguments.
    ///
    /// # Arguments
    ///
    /// * `name`: The name of the user event.
    /// * `args`: The arguments for the user event.
    pub fn new(name: String, args: impl KAny) -> EventMessage {
        EventMessage::User(UserEvent(name, Arc::new(args)))
    }

    /// Returns whether this `EventMessage` represents a built-in event.
    #[inline]
    pub fn is_builtin(&self) -> bool { matches!(self, EventMessage::Builtin(..)) }

    /// Returns whether this `EventMessage` represents a user event.
    #[inline]
    pub fn is_user(&self) -> bool { matches!(self, EventMessage::User(..)) }

    /// Unwraps this `EventMessage` as a `BuiltinEvent`.
    ///
    /// # Panics
    ///
    /// If this `EventMessage` represents a user event.
    pub fn unwrap_builtin(self) -> BuiltinEvent {
        match self {
            EventMessage::Builtin(evt) => evt,
            EventMessage::User(..) => panic!("called EventMessage::unwrap_builtin on a `User` value"),
        }
    }

    /// Unwraps this `EventMessage` as a user event with the given name and arguments.
    ///
    /// # Panics
    ///
    /// If this `EventMessage` represents a built-in event.
    pub fn unwrap_user(self) -> (String, Arc<dyn KAny>) {
        match self {
            EventMessage::User(UserEvent(name, args)) => (name, args),
            EventMessage::Builtin(_) => panic!("called EventMessage::unwrap_user on a `Builtin` value"),
        }
    }

    /// Returns whether this `EventMessage` represents a lifecycle event.
    pub fn is_lifecycle(&self) -> bool {
        if let EventMessage::Builtin(evt) = self {
            evt.is_internal()
        } else { false }
    }

    /// Returns whether this `EventMessage` represents a strict lifecycle event.
    pub fn is_strict_lifecycle(&self) -> bool {
        if let EventMessage::Builtin(evt) = self {
            evt.is_strict_lifecycle()
        } else { false }
    }

    /// Returns whether this `EventMessage` represents an internal event.
    pub fn is_internal(&self) -> bool {
        if let EventMessage::Builtin(evt) = self {
            evt.is_internal()
        } else { false }
    }
}

impl BuiltinEvent {
    /// Returns whether this `BuiltinEvent` represents a lifecycle event.
    ///
    /// A lifecycle event is one that corresponds to the creation or destruction of a scope, such as
    /// `Fork`, `Ready`, or `Dispose`.
    pub fn is_lifecycle(&self) -> bool {
        matches!(*self, BuiltinEvent::Fork(..) | BuiltinEvent::Ready | BuiltinEvent::Dispose)
    }

    /// Returns whether this `BuiltinEvent` represents a strict lifecycle event.
    ///
    /// A strict lifecycle event is one that corresponds to the creation of a scope, such as
    /// `Fork`. This includes internal events like `Internal(Fork(...))`.
    pub fn is_strict_lifecycle(&self) -> bool {
        matches!(*self, BuiltinEvent::Fork(..) | BuiltinEvent::Ready)
    }

    /// Returns whether this `BuiltinEvent` represents an internal event.
    ///
    /// An internal event is one that is generated internally by the runtime and is not user-specified.
    pub fn is_internal(&self) -> bool {
        matches!(*self, BuiltinEvent::Internal(..))
    }
}

pub enum LifecycleEvent {
    Fork,
    Ready,
    Dispose,
}

mod sealed {
    pub trait Handler : Send + Sync {}
}

pub trait EventNya : Send + Sync {
    type Args;
    type Result = ();
}

impl EventNya for InternalEvent {
    type Args = InternalEvent;
}
impl EventNya for LifecycleEvent {
    type Args = LifecycleEvent;
}

#[async_trait]
pub(crate) trait HandlerTrait<E: EventNya>: sealed::Handler {
    async fn handle(&mut self, data: E::Args) -> color_eyre::Result<E::Result>;
}

#[async_trait]
pub(crate) trait Handler : Send + Sync {
    fn should_call(&self, evt: &EventMessage) -> bool;
    async fn call(&self, args: Box<dyn KAny>);
}

pub struct EventHandler<E: EventNya> {
    inner: Box<dyn HandlerTrait<E>>,
}

#[async_trait]
impl<E: EventNya + 'static> Handler for EventHandler<E> {
    fn should_call(&self, evt: &EventMessage) -> bool {
        match evt {
            EventMessage::Builtin(_) if TypeId::of::<E>() == TypeId::of::<LifecycleEvent>() => evt.is_strict_lifecycle(),
            EventMessage::Builtin(_) if TypeId::of::<E>() == TypeId::of::<InternalEvent>() => evt.is_internal(),
            EventMessage::Builtin(_) if TypeId::of::<E>() == TypeId::of::<BuiltinEvent>() => true,
            EventMessage::User(_)  if TypeId::of::<E>() == TypeId::of::<UserEvent>() => true,
            _ => false
        }
    }

    async fn call(&self, args: Box<dyn KAny>) {
        todo!()
    }
}

