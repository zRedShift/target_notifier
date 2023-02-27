#[cfg(feature = "async-channel")]
use async_channel as channel;
#[cfg(feature = "async-std")]
use async_std::channel;
#[cfg(feature = "std")]
use std::sync::mpsc as channel;

#[cfg(feature = "std")]
pub use channel::{RecvError, SendError};
#[cfg(any(feature = "async-channel", feature = "async-std"))]
pub use channel::{TryRecvError as RecvError, TrySendError as SendError};

#[cfg(feature = "async-std")]
use async_std::sync::Arc;
#[cfg(feature = "async-channel")]
use hybrid_rc::Arc;
#[cfg(feature = "std")]
use std::sync::Arc;

#[cfg(any(feature = "async-channel", feature = "async-std"))]
use parking_lot as mutex;
#[cfg(feature = "std")]
use std::sync as mutex;

#[cfg(feature = "embassy")]
use embassy_sync::blocking_mutex::{raw::CriticalSectionRawMutex, CriticalSectionMutex};
#[cfg(feature = "embassy")]
pub use embassy_sync::channel::{
    self, DynamicReceiver as Receiver, DynamicSender as Sender, TryRecvError as RecvError,
    TrySendError as SendError,
};

#[cfg(feature = "embassy")]
use core::cell::RefCell;

use crate::State;
use core::ops::Deref;

#[cfg(any(feature = "async-channel", feature = "async-std", feature = "std"))]
pub(super) type Sender<'ch, T> = channel::Sender<T>;
#[cfg(any(feature = "async-channel", feature = "async-std", feature = "std"))]
pub(super) type Receiver<'ch, T> = channel::Receiver<T>;
#[cfg(any(feature = "async-channel", feature = "async-std", feature = "std"))]
pub(super) struct Channel<T, const N: usize>(Sender<'static, T>, Receiver<'static, T>);
#[cfg(any(feature = "async-channel", feature = "async-std", feature = "std"))]
impl<T, const N: usize> Channel<T, N> {
    pub(super) fn new() -> Self {
        #[cfg(any(feature = "async-channel", feature = "async-std"))]
        let ch = channel::bounded(N);
        #[cfg(feature = "std")]
        let ch = channel::channel();
        Self(ch.0, ch.1)
    }
    pub(super) fn sender(&self) -> Sender<'_, T> {
        self.0.clone()
    }
    pub(super) fn receiver(&self) -> Receiver<'_, T> {
        self.1.clone()
    }
}
#[cfg(any(feature = "async-channel", feature = "async-std", feature = "std"))]
type MutexServiceState = Arc<mutex::Mutex<State>>;

#[cfg(feature = "embassy")]
pub(super) type Channel<T, const N: usize> = channel::Channel<CriticalSectionRawMutex, T, N>;
#[cfg(feature = "embassy")]
type MutexServiceState = CriticalSectionMutex<RefCell<State>>;

pub(super) struct Mutex(MutexServiceState);
impl Mutex {
    pub(super) fn new() -> Self {
        Self(Self::new_mutex())
    }
    #[cfg(feature = "embassy")]
    fn new_mutex() -> MutexServiceState {
        MutexServiceState::new(RefCell::new(State::Inactive))
    }
    #[cfg(any(feature = "async-channel", feature = "async-std", feature = "std"))]
    fn new_mutex() -> MutexServiceState {
        MutexServiceState::new(mutex::Mutex::new(State::Inactive))
    }
}
impl Deref for Mutex {
    type Target = MutexServiceState;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
