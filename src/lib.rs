#![cfg_attr(not(feature = "std"), no_std)]

// extern crate target_notifier_proc;

use core::ops::{Deref, DerefMut};

pub use prelude::RecvFuture;
pub use prelude::{RecvError, SendError};
pub use target_notifier_proc::Notifier;

mod prelude {
    #[cfg(feature = "async-channel")]
    use async_channel as channel;
    #[cfg(feature = "async-std")]
    use async_std::channel;

    #[cfg(any(feature = "async-channel", feature = "async-std"))]
    pub use channel::{Recv as RecvFuture, TryRecvError as RecvError, TrySendError as SendError};

    #[cfg(feature = "async-std")]
    use async_std::sync::Arc;
    #[cfg(feature = "async-channel")]
    use hybrid_rc::Arc;

    #[cfg(feature = "embassy")]
    use embassy_sync::blocking_mutex::{raw::CriticalSectionRawMutex, CriticalSectionMutex};
    #[cfg(feature = "embassy")]
    pub use embassy_sync::channel::{
        self, DynamicReceiver as Receiver, DynamicRecvFuture as RecvFuture,
        DynamicSender as Sender, TryRecvError as RecvError, TrySendError as SendError,
    };

    #[cfg(feature = "embassy")]
    use core::cell::RefCell;
    use core::ops::Deref;

    use crate::ServiceState;

    #[cfg(any(feature = "async-channel", feature = "async-std"))]
    pub(super) type Sender<'ch, T> = channel::Sender<T>;
    #[cfg(any(feature = "async-channel", feature = "async-std"))]
    pub(super) type Receiver<'ch, T> = channel::Receiver<T>;
    #[cfg(any(feature = "async-channel", feature = "async-std"))]
    pub(super) struct Channel<T, const N: usize>(Sender<'static, T>, Receiver<'static, T>);
    #[cfg(any(feature = "async-channel", feature = "async-std"))]
    impl<T, const N: usize> Channel<T, N> {
        pub(super) fn new() -> Self {
            let ch = channel::bounded(N);
            Self(ch.0, ch.1)
        }
        pub(super) fn sender<'ch>(&'ch self) -> Sender<'ch, T> {
            self.0.clone()
        }
        pub(super) fn receiver<'ch>(&'ch self) -> Receiver<'ch, T> {
            self.1.clone()
        }
    }
    #[cfg(any(feature = "async-channel", feature = "async-std"))]
    type MutexServiceState = Arc<parking_lot::Mutex<ServiceState>>;
    #[cfg(feature = "std")]
    type MutexServiceState = Arc<std::sync::Mutex<ServiceState>>;

    #[cfg(feature = "embassy")]
    pub(super) type Channel<T, const N: usize> = channel::Channel<CriticalSectionRawMutex, T, N>;
    #[cfg(feature = "embassy")]
    type MutexServiceState = CriticalSectionMutex<RefCell<ServiceState>>;

    pub(super) struct Mutex(MutexServiceState);
    impl Mutex {
        pub(super) fn new() -> Self {
            Self(Self::new_mutex())
        }
        #[cfg(feature = "embassy")]
        fn new_mutex() -> MutexServiceState {
            MutexServiceState::new(RefCell::new(ServiceState::Inactive))
        }
        #[cfg(any(feature = "async-channel", feature = "async-std"))]
        fn new_mutex() -> MutexServiceState {
            MutexServiceState::new(parking_lot::Mutex::new(ServiceState::Inactive))
        }
    }
    impl Deref for Mutex {
        type Target = MutexServiceState;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
}

pub trait ServiceGet<'ch, const ID: usize, T> {
    fn get(&'ch self, index: Option<usize>) -> &'ch dyn DynamicService<'ch, T>;
}
pub trait ChannelGet<'ch, const ID: usize, const SIZE: usize> {
    fn get(&'ch self, index: usize) -> &'ch dyn DynamicServiceId;
}
pub trait Notifier<'ch, T: 'static> {
    type Type;
    fn send(
        &'ch self,
        send: impl FnMut(&'_ mut dyn Iterator<Item = &'ch dyn DynamicService<'ch, T>>),
    );
}
pub trait DynamicService<'f, T>: private::DynamicService<'f, T> {}
pub trait DynamicServiceId: private::DynamicServiceId {}
pub trait DynamicServiceState: private::DynamicServiceState {
    fn get_state(&self) -> ServiceState {
        private::DynamicServiceState::get_state(self)
    }
}

#[derive(Clone, Copy)]
pub enum ServiceState {
    Inactive,
    Active(usize),
}
impl ServiceState {
    fn incr(&mut self) {
        *self = match &*self {
            ServiceState::Inactive => ServiceState::Active(1),
            ServiceState::Active(count) => ServiceState::Active(*count + 1),
        }
    }
    fn decr(&mut self) {
        *self = match &*self {
            ServiceState::Active(count) if *count > 0 => ServiceState::Active(*count - 1),
            _ => ServiceState::Inactive,
        }
    }
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active(_))
    }
    pub fn is_inactive(&self) -> bool {
        matches!(self, Self::Inactive)
    }
    pub fn count_receivers(&self) -> usize {
        match self {
            ServiceState::Inactive => 0,
            ServiceState::Active(count) => *count,
        }
    }
}

#[derive(Debug)]
pub enum Error<T> {
    NotInitalized,
    Send(usize, SendError<T>),
}
pub static INCORRECT_INDEX: &'static str = "Incorrect channel index";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ID(usize, Option<usize>);
impl ID {
    pub fn new(id: usize) -> Self {
        Self(id, None)
    }
    pub fn index(mut self, index: usize) -> Self {
        self.1 = Some(index);
        self
    }
    fn eq_target(&self, other: &Self) -> bool {
        match other.1 {
            Some(_) => self.eq(other),
            None => self.0 == other.0,
        }
    }
}

pub struct Service<T, const N: usize>(Option<ID>, prelude::Channel<T, N>, prelude::Mutex);
impl<T, const N: usize> Service<T, N> {
    pub fn init(&mut self, id: impl Into<ID>) {
        self.0 = Some(id.into());
    }
}
impl Service<(), 0> {
    pub fn array<FS, F, const SIZE: usize, I: Copy>(id: I, arr: &mut [FS; SIZE], mut cb: F)
    where
        F: FnMut(ID, &mut FS),
        ID: From<I>,
    {
        arr.iter_mut()
            .enumerate()
            .for_each(|(index, item)| cb(ID::from(id).index(index), item));
    }
}
impl<T, const N: usize> Default for Service<T, N> {
    fn default() -> Self {
        Self(None, prelude::Channel::new(), prelude::Mutex::new())
    }
}
impl<T, const N: usize> private::DynamicServiceId for Service<T, N> {
    fn id(&self) -> &Option<ID> {
        &self.0
    }
}
impl<'f, T, const N: usize> private::DynamicService<'f, T> for Service<T, N> {
    fn sender(&'f self) -> prelude::Sender<'f, T> {
        self.1.sender().into()
    }

    fn receiver(&'f self) -> prelude::Receiver<'f, T> {
        self.1.receiver().into()
    }
}
impl<T, const N: usize> private::DynamicServiceState for Service<T, N> {
    #[cfg(feature = "embassy")]
    fn state(&self, call: &mut dyn FnMut(&mut ServiceState)) {
        self.2.lock(|cell| call(cell.borrow_mut().deref_mut()));
    }
    #[cfg(any(feature = "async-channel", feature = "async-std"))]
    fn state(&self, call: &mut dyn FnMut(&mut ServiceState)) {
        call(self.2.lock().deref_mut())
    }
}

pub struct Channel<'notif, const I: usize, Notif>(Option<usize>, &'notif Notif);
impl<'notif, const I: usize, Notif> Channel<'notif, I, Notif> {
    pub fn sender(&self) -> Sender<'notif, Notif> {
        Sender(ID(I, self.0), self.1)
    }
    pub fn receiver<T: 'static>(&'notif self) -> Receiver<'_, T>
    where
        Notif: ServiceGet<'notif, I, T>,
    {
        Receiver::new(self.1.get(self.0))
    }
}

pub struct Channels<'notif, const I: usize, const SIZE: usize, Notif: ChannelGet<'notif, I, SIZE>>(
    &'notif Notif,
    [Channel<'notif, I, Notif>; SIZE],
);
impl<'notif, const I: usize, const SIZE: usize, Notif: ChannelGet<'notif, I, SIZE>>
    Channels<'notif, I, SIZE, Notif>
{
    pub fn new(notif: &'notif Notif) -> Self {
        Self(
            notif,
            core::array::from_fn(|index| {
                notif
                    .get(index)
                    .id()
                    .map(|index| Channel(index.1, notif))
                    .unwrap()
            }),
        )
    }
}
impl<'notif, const I: usize, const SIZE: usize, Notif: ChannelGet<'notif, I, SIZE>> Deref
    for Channels<'notif, I, SIZE, Notif>
{
    type Target = [Channel<'notif, I, Notif>; SIZE];
    fn deref(&self) -> &Self::Target {
        &self.1
    }
}
impl<'notif, const I: usize, const SIZE: usize, Notif: ChannelGet<'notif, I, SIZE>> DerefMut
    for Channels<'notif, I, SIZE, Notif>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.1
    }
}
impl<'notif, Notif, const I: usize> From<&'notif Notif> for Channel<'notif, I, Notif> {
    fn from(notif: &'notif Notif) -> Self {
        Self(None, notif)
    }
}

pub struct Receiver<'ch, T>(prelude::Receiver<'ch, T>, &'ch dyn DynamicService<'ch, T>);
impl<'ch, T> Receiver<'ch, T> {
    fn new(field: &'ch dyn DynamicService<'ch, T>) -> Self {
        field.state(&mut |state| state.incr());
        Self(field.receiver(), field)
    }
    pub fn recv(&mut self) -> RecvFuture<T> {
        self.0.recv()
    }
    pub fn try_recv(&mut self) -> Result<T, RecvError> {
        self.0.try_recv()
    }
    pub fn deactivate(self) -> InactiveReceiver<'ch, T> {
        InactiveReceiver(self.1)
    }
}
impl<'ch, T> Drop for Receiver<'ch, T> {
    fn drop(&mut self) {
        self.1.state(&mut |state| state.decr());
    }
}

pub struct InactiveReceiver<'ch, T>(&'ch dyn DynamicService<'ch, T>);
impl<'ch, T> InactiveReceiver<'ch, T> {
    pub fn activate(self) -> Receiver<'ch, T> {
        Receiver::new(self.0)
    }
}

#[derive(Clone, Copy)]
pub struct Sender<'notif, Notif>(ID, &'notif Notif);
impl<'notif, Notif> Sender<'notif, Notif> {
    pub fn send<T: Clone + 'static>(&self, event: T) -> Result<(), Error<T>>
    where
        Notif: Notifier<'notif, T, Type = T>,
    {
        self.send_impl(Err(Error::NotInitalized), |_| true, event)
    }

    pub fn send_to<Target: Into<ID> + Copy, T: Clone + 'static, const S: usize>(
        &self,
        targets: [Target; S],
        event: T,
    ) -> Result<(), Error<T>>
    where
        Notif: Notifier<'notif, T, Type = T>,
    {
        let ret = if targets.is_empty() {
            Ok(())
        } else {
            Err(Error::NotInitalized)
        };

        let targets: [ID; S] = targets.map(|target| target.into());
        self.send_impl(
            ret,
            |id| targets.iter().find(|t_id| id.eq_target(t_id)).is_some(),
            event,
        )
    }

    fn send_impl<'a, F, T: Clone + 'static>(
        &self,
        mut ret: Result<(), Error<T>>,
        mut filter: F,
        event: T,
    ) -> Result<(), Error<T>>
    where
        Notif: Notifier<'notif, T, Type = T>,
        F: FnMut(&ID) -> bool,
    {
        self.1.send(|slice| {
            for (id, res) in slice.filter_map(|field| match field.id() {
                Some(id) if id != &self.0 && filter(id) && field.get_state().is_active() => {
                    Some((id, field.sender().try_send(event.clone())))
                }
                _ => None,
            }) {
                if let Err(error) = res {
                    ret = Err(Error::Send(id.0, error));
                    break;
                } else if ret.is_err() {
                    ret = Ok(())
                }
                log::info!("Send; Sended to {id:?}");
            }
        });

        ret
    }
}

mod private {
    use super::*;

    pub trait DynamicService<'f, T>: DynamicServiceId + DynamicServiceState {
        fn sender(&'f self) -> prelude::Sender<'f, T>;
        fn receiver(&'f self) -> prelude::Receiver<'f, T>;
    }
    impl<'f, T, F: DynamicService<'f, T>> super::DynamicService<'f, T> for F {}

    pub trait DynamicServiceId {
        fn id(&self) -> &Option<ID>;
    }
    impl<F: DynamicServiceId> super::DynamicServiceId for F {}

    pub trait DynamicServiceState {
        fn get_state(&self) -> ServiceState {
            let mut ret = ServiceState::Inactive;
            self.state(&mut |state| ret = *state);
            ret
        }
        fn state(&self, call: &mut dyn FnMut(&mut ServiceState));
    }
    impl<F: DynamicServiceState> super::DynamicServiceState for F {}
}
