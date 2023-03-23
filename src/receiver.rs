use super::*;
use core::future::Future;

pub struct Receiver<'ch, T>(prelude::Receiver<'ch, T>, &'ch dyn DynamicService<T>);
impl<'ch, T> Receiver<'ch, T> {
    pub(super) fn new(field: &'ch dyn DynamicService<T>) -> Self {
        field.state(&mut |state| state.incr());
        Self(field.receiver(), field)
    }
    #[cfg(any(feature = "async-channel", feature = "async-std"))]
    pub fn recv(&mut self) -> impl Future<Output = T> + '_ {
        use futures_util::FutureExt;
        self.0.recv().map(Result::unwrap)
    }
    #[cfg(feature = "embassy")]
    pub fn recv(&mut self) -> impl Future<Output = T> + '_ {
        self.0.recv()
    }
    #[cfg(not(feature = "std"))]
    pub fn try_recv(&mut self) -> Result<T, prelude::RecvError> {
        self.0.try_recv()
    }
    #[cfg(feature = "std")]
    pub fn try_recv(&mut self) -> Result<T, prelude::RecvError> {
        self.0.recv()
    }
    pub fn deactivate(self) -> InactiveReceiver<'ch, T> {
        InactiveReceiver(self.1)
    }

    pub fn id(&self) -> Option<&ID> {
        self.1.id().as_ref()
    }

    pub fn target<Target: From<ID>>(&self) -> Target {
        self.1.id().map(Into::into).expect("Bad id")
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
