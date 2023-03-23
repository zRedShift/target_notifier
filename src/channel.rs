use super::*;
use core::{
    ops::{Deref, DerefMut},
    usize,
};

pub struct Channel<'notif, Notif, Targets, const ID: usize> {
    notifier: &'notif Notif,
    target: Targets,
}

impl<'notif, Notif, Targets, const ID: usize> Channel<'notif, Notif, Targets, { ID }>
where
    Targets: Into<ID> + Copy,
    Notif: Notifier,
{
    pub fn new(notif: &'notif Notif, target: Targets) -> Self {
        Self {
            notifier: notif,
            target,
        }
    }

    pub fn sender(&self) -> Sender<'notif, Notif> {
        Sender(self.id(), self.notifier)
    }
    pub fn receiver<T>(&self) -> Receiver<'notif, T>
    where
        Notif: marker::ServiceGet<{ ID }, T>,
    {
        self.notifier.receiver::<{ ID }, T>(self.id().index())
    }
    pub fn split<T>(&self) -> (Sender<'notif, Notif>, Receiver<'notif, T>)
    where
        Notif: marker::ServiceGet<{ ID }, T>,
    {
        (self.sender(), self.receiver())
    }
    pub fn id(&self) -> ID {
        self.target.into()
    }
}

pub struct Channels<'notif, const SIZE: usize, Notif, Targets, const ID: usize> {
    array: [Channel<'notif, Notif, Targets, { ID }>; SIZE],
}
impl<'notif, const SIZE: usize, Notif, Targets, const ID: usize>
    Channels<'notif, SIZE, Notif, Targets, { ID }>
where
    Notif: Notifier,
{
    pub fn new(notif: &'notif Notif, target: Targets) -> Self
    where
        ID: From<Targets> + Into<Targets>,
    {
        let id = ID::from(target);
        Self {
            array: core::array::from_fn(|index| Channel {
                notifier: notif,
                target: id.set_index(index).into(),
            }),
        }
    }
    pub fn sender(&self, index: usize) -> Option<Sender<'notif, Notif>>
    where
        Targets: Into<ID> + Copy,
    {
        self.get(index).map(Channel::sender)
    }
    pub fn receiver<T>(&self, index: usize) -> Option<Receiver<'notif, T>>
    where
        Notif: marker::ServiceGet<{ ID }, T>,
        Targets: Into<ID> + Copy,
    {
        self.get(index).map(Channel::receiver)
    }
}
impl<'notif, const SIZE: usize, Notif, Targets, const ID: usize> Deref
    for Channels<'notif, SIZE, Notif, Targets, { ID }>
{
    type Target = [Channel<'notif, Notif, Targets, { ID }>; SIZE];
    fn deref(&self) -> &Self::Target {
        &self.array
    }
}
impl<'notif, const SIZE: usize, Notif, Targets, const ID: usize> DerefMut
    for Channels<'notif, SIZE, Notif, Targets, { ID }>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.array
    }
}
