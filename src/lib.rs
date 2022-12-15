#![cfg_attr(not(feature = "std"), no_std)]

use core::{
    fmt::{Debug, Display},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    usize,
};

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
        pub(super) fn sender(&self) -> Sender<'_, T> {
            self.0.clone()
        }
        pub(super) fn receiver(&self) -> Receiver<'_, T> {
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

pub trait ServiceGet<const ID: usize, T> {
    fn get(&self, index: Option<usize>) -> &dyn DynamicService<T>;
}
pub trait ChannelGet<const ID: usize, const SIZE: usize> {
    fn get(&self, index: usize) -> &dyn DynamicServiceId;
}
pub trait NotifierSenders<T> {
    type Iter<'ch>: Iterator<Item = &'ch dyn DynamicService<T>> + Clone
    where
        T: 'ch,
        Self: 'ch;
    fn get(&self) -> Self::Iter<'_>;
}
pub trait DynamicService<T>: private::DynamicService<T> {}
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
    fn decr(&mut self) -> bool {
        let mut ret = false;
        *self = match &*self {
            ServiceState::Active(count) if *count > 1 => ServiceState::Active(*count - 1),
            _ => {
                ret = true;
                ServiceState::Inactive
            }
        };
        ret
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
    NotInitialized,
    Send(usize, SendError<T>),
}
pub static INCORRECT_INDEX: &str = "Incorrect channel index";

#[derive(Clone, Copy, PartialEq)]
pub struct ID(usize, Option<usize>, &'static str);
impl ID {
    pub fn new(id: usize) -> Self {
        Self(id, None, "")
    }
    pub fn id(&self) -> usize {
        self.0
    }
    pub fn index(&self) -> Option<usize> {
        self.1
    }
    pub fn name(&self) -> &'static str {
        self.2
    }
    pub fn set_index(mut self, index: usize) -> Self {
        self.1 = Some(index);
        self
    }
    pub fn set_name(mut self, name: &'static str) -> Self {
        self.2 = name;
        self
    }
    fn eq_target(&self, other: &Self) -> bool {
        match other.1 {
            Some(_) => self.eq(other),
            None => self.0 == other.0,
        }
    }
}
impl Display for ID {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match (self.0, self.1) {
            (usize::MAX, _) => write!(f, "[{}]", self.2),
            (id, None) => write!(f, "[{}](Id: {id})", self.2),
            (id, Some(index)) => write!(f, "[{}({index})](Id: {id})", self.2),
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
            .for_each(|(index, item)| cb(ID::from(id).set_index(index), item));
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
impl<T, const N: usize> private::DynamicService<T> for Service<T, N> {
    fn sender(&self) -> prelude::Sender<'_, T> {
        self.1.sender().into()
    }

    fn receiver(&self) -> prelude::Receiver<'_, T> {
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

pub struct Channel<'notif, const I: usize, Notif, Targets>(
    Option<usize>,
    &'notif Notif,
    PhantomData<Targets>,
);
impl<'notif, const I: usize, Notif, Targets> Channel<'notif, I, Notif, Targets>
where
    Targets: Into<ID> + From<usize>,
{
    pub fn sender(&self) -> Sender<'notif, Notif> {
        Sender(self.id(), self.1)
    }
    pub fn receiver<T>(&self) -> Receiver<'notif, T>
    where
        Notif: ServiceGet<I, T>,
    {
        Receiver::new(self.1.get(self.0))
    }
    pub fn split<T>(&self) -> (Sender<'notif, Notif>, Receiver<'notif, T>)
    where
        Notif: ServiceGet<I, T>,
    {
        (self.sender(), self.receiver())
    }
    pub fn id(&self) -> ID {
        let mut id = Targets::from(I).into();
        id.1 = self.0;
        id
    }
}

pub struct Channels<'notif, const I: usize, const SIZE: usize, Notif, Targets>(
    &'notif Notif,
    [Channel<'notif, I, Notif, Targets>; SIZE],
)
where
    Notif: ChannelGet<I, SIZE>;
impl<'notif, const I: usize, const SIZE: usize, Notif, Targets>
    Channels<'notif, I, SIZE, Notif, Targets>
where
    Notif: ChannelGet<I, SIZE>,
{
    pub fn new(notif: &'notif Notif) -> Self {
        Self(
            notif,
            core::array::from_fn(|index| {
                notif
                    .get(index)
                    .id()
                    .map(|index| Channel(index.1, notif, Default::default()))
                    .unwrap()
            }),
        )
    }
    pub fn sender(&self, index: usize) -> Option<Sender<'notif, Notif>>
    where
        Targets: Into<ID> + From<usize>,
    {
        self.get(index).map(|ch| ch.sender())
    }
    pub fn receiver<T>(&self, index: usize) -> Option<Receiver<'notif, T>>
    where
        Notif: ServiceGet<I, T>,
        Targets: Into<ID> + From<usize>,
    {
        self.get(index).map(|ch| ch.receiver())
    }
}
impl<'notif, const I: usize, const SIZE: usize, Notif: ChannelGet<I, SIZE>, Targets> Deref
    for Channels<'notif, I, SIZE, Notif, Targets>
{
    type Target = [Channel<'notif, I, Notif, Targets>; SIZE];
    fn deref(&self) -> &Self::Target {
        &self.1
    }
}
impl<'notif, const I: usize, const SIZE: usize, Notif: ChannelGet<I, SIZE>, Targets> DerefMut
    for Channels<'notif, I, SIZE, Notif, Targets>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.1
    }
}
impl<'notif, Notif, const I: usize, Targets> From<&'notif Notif>
    for Channel<'notif, I, Notif, Targets>
{
    fn from(notif: &'notif Notif) -> Self {
        Self(None, notif, Default::default())
    }
}

pub struct Receiver<'ch, T>(prelude::Receiver<'ch, T>, &'ch dyn DynamicService<T>);
impl<'ch, T> Receiver<'ch, T> {
    fn new(field: &'ch dyn DynamicService<T>) -> Self {
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
impl<'ch, T> Clone for Receiver<'ch, T> {
    fn clone(&self) -> Self {
        Self::new(self.1)
    }
}
impl<'ch, T> Drop for Receiver<'ch, T> {
    fn drop(&mut self) {
        let mut to_clear = false;
        self.1.state(&mut |state| to_clear = state.decr());
        if to_clear {
            while self.0.try_recv().is_ok() {}
        }
    }
}

pub struct InactiveReceiver<'ch, T>(&'ch dyn DynamicService<T>);
impl<'ch, T> InactiveReceiver<'ch, T> {
    pub fn activate(self) -> Receiver<'ch, T> {
        Receiver::new(self.0)
    }
}

#[derive(Clone, Copy)]
pub struct Sender<'notif, Notif>(ID, &'notif Notif);
impl<'notif, Notif> Sender<'notif, Notif> {
    pub fn id(&self) -> ID {
        self.0
    }

    #[inline]
    pub fn send<T: Debug + Clone>(&self, event: T) -> Result<(), Error<T>>
    where
        Notif: NotifierSenders<T>,
    {
        self.send_filtered([], event)
    }

    pub fn send_filtered<Target: Copy, T: Debug + Clone, const S: usize>(
        &self,
        filter: [Target; S],
        event: T,
    ) -> Result<(), Error<T>>
    where
        ID: From<Target>,
        Notif: NotifierSenders<T>,
    {
        let filter = filter.map(ID::from);
        self.send_impl(
            move |id, state| {
                id != &self.0 && state.is_active() && filter.iter().all(|t_id| !id.eq_target(t_id))
            },
            event,
        )
    }

    pub fn send_to<Tg, T, const S: usize>(&self, targets: [Tg; S], event: T) -> Result<(), Error<T>>
    where
        Tg: Copy,
        T: Debug + Clone,
        ID: From<Tg>,
        Notif: NotifierSenders<T>,
    {
        let targets = targets.map(ID::from);

        self.send_impl(|id, _| targets.iter().any(|t_id| id.eq_target(t_id)), event)
    }

    fn send_impl<F, T: Debug + Clone>(&self, mut filter: F, event: T) -> Result<(), Error<T>>
    where
        Notif: NotifierSenders<T>,
        F: FnMut(&ID, ServiceState) -> bool + Clone,
    {
        let mut ret = Ok(());
        let handle_err = |id: &ID, ret: &mut Result<(), Error<T>>, res| {
            if let Err(err) = res {
                log::info!("Error sending to {id}");
                *ret = Err(Error::Send(id.0, err))
            } else {
                log::info!("Sent to {id}");
            }
        };

        let mut slice = self.1.get().filter_map(move |field| match field.id() {
            Some(id) if filter(id, field.get_state()) => Some((id, field)),
            _ => None,
        });
        let count = slice.clone().count();

        match count {
            0 => ret = Err(Error::NotInitialized),
            1 => {
                let (id, field) = slice.next().unwrap();
                let res = field.sender().try_send(event);
                handle_err(id, &mut ret, res)
            }
            _ => {
                for (id, field) in slice {
                    let res = field.sender().try_send(event.clone());
                    handle_err(id, &mut ret, res);
                }
            }
        };

        ret
    }
}

pub trait Notifier: Sized {
    fn sender(&self, id: impl Into<ID>) -> Sender<Self> {
        Sender(id.into(), self)
    }
    fn receiver<const I: usize, T>(&self, index: Option<usize>) -> Receiver<'_, T>
    where
        Self: ServiceGet<I, T>,
    {
        Receiver::new(self.get(index))
    }
}

mod private {
    use super::*;

    pub trait DynamicService<T>: DynamicServiceId + DynamicServiceState {
        fn sender(&self) -> prelude::Sender<'_, T>;
        fn receiver(&self) -> prelude::Receiver<'_, T>;
    }
    impl<T, F: DynamicService<T>> super::DynamicService<T> for F {}

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
