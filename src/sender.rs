use super::*;
use core::fmt::Debug;

#[derive(Clone, Copy)]
pub struct Sender<'notif, Notif>(pub(super) ID, pub(super) &'notif Notif);
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
        F: FnMut(&ID, State) -> bool + Clone,
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
                #[cfg(not(feature = "std"))]
                let res = field.sender().try_send(event);
                #[cfg(feature = "std")]
                let res = field.sender().send(event);
                handle_err(id, &mut ret, res)
            }
            _ => {
                for (id, field) in slice {
                    #[cfg(not(feature = "std"))]
                    let res = field.sender().try_send(event.clone());
                    #[cfg(feature = "std")]
                    let res = field.sender().send(event.clone());
                    handle_err(id, &mut ret, res);
                }
            }
        };

        ret
    }
}
