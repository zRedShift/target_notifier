use super::*;

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
    fn get_state(&self) -> State {
        private::DynamicServiceState::get_state(self)
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

pub(super) mod private {
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
        fn get_state(&self) -> State {
            let mut ret = State::Inactive;
            self.state(&mut |state| ret = *state);
            ret
        }
        fn state(&self, call: &mut dyn FnMut(&mut State));
    }
    impl<F: DynamicServiceState> super::DynamicServiceState for F {}
}
