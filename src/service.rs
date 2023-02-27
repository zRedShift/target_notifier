use super::*;
use core::ops::DerefMut;

#[derive(Clone, Copy)]
pub enum State {
    Inactive,
    Active(usize),
}
impl State {
    pub(super) fn incr(&mut self) {
        *self = match &*self {
            Self::Inactive => Self::Active(1),
            Self::Active(count) => Self::Active(*count + 1),
        }
    }
    pub(super) fn decr(&mut self) -> bool {
        let mut ret = false;
        *self = match &*self {
            Self::Active(count) if *count > 1 => Self::Active(*count - 1),
            _ => {
                ret = true;
                Self::Inactive
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
            Self::Inactive => 0,
            Self::Active(count) => *count,
        }
    }
}

#[derive(Debug)]
pub enum Error<T> {
    NotInitialized,
    Send(usize, prelude::SendError<T>),
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
    fn state(&self, call: &mut dyn FnMut(&mut State)) {
        self.2.lock(|cell| call(cell.borrow_mut().deref_mut()));
    }
    #[cfg(any(feature = "async-channel", feature = "async-std"))]
    fn state(&self, call: &mut dyn FnMut(&mut State)) {
        call(self.2.lock().deref_mut())
    }
    #[cfg(feature = "std")]
    fn state(&self, call: &mut dyn FnMut(&mut State)) {
        call(self.2.lock().unwrap().deref_mut())
    }
}
