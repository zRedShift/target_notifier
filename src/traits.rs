use super::*;

pub trait ServiceGet<T> {
    fn get(&self, target: impl Into<ID>) -> Option<&dyn DynamicService<T>>;
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
    fn sender(&self, target: impl Into<ID>) -> Sender<Self> {
        Sender(target.into(), self)
    }
    fn receiver<const ID: usize, T>(&self, index: Option<usize>) -> Receiver<'_, T>
    where
        Self: marker::ServiceGet<{ ID }, T>,
    {
        let id = match index {
            Some(index) => ID::new(ID).set_index(index),
            None => ID::new(ID),
        };
        self.get(id).map(Receiver::new).expect(INCORRECT_INDEX)
    }

    fn receiver_by_target<T>(&self, target: impl Into<ID>) -> Option<Receiver<'_, T>>
    where
        Self: ServiceGet<T>,
    {
        self.get(target).map(Receiver::new)
    }
}

pub mod marker {
    pub trait ServiceGet<const ID: usize, T>: super::ServiceGet<T> {}
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
