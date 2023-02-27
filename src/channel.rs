use super::*;
use core::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    usize,
};

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
