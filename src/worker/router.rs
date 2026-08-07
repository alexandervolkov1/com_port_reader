use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use crate::connection::ConnectionId;

use super::WorkerHandle;

#[derive(Clone, Default)]
pub struct ConnectionRouter {
    handles: Arc<RwLock<BTreeMap<ConnectionId, WorkerHandle>>>,
}

impl ConnectionRouter {
    pub fn handle(&self, connection_id: ConnectionId) -> Option<WorkerHandle> {
        self.handles
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&connection_id)
            .cloned()
    }

    pub(crate) fn insert(&self, handle: WorkerHandle) {
        let connection_id = handle.connection_id();

        let previous = self
            .handles
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(connection_id, handle);

        assert!(
            previous.is_none(),
            "connection router already contains \
             connection {connection_id}",
        );
    }
}

#[cfg(test)]
mod tests {
    use crossbeam_channel::unbounded;

    use super::ConnectionRouter;
    use crate::{connection::ConnectionId, worker::WorkerHandle};

    #[test]
    fn resolves_registered_handle() {
        let router = ConnectionRouter::default();

        let connection_id = ConnectionId::new(7);

        let (sender, _receiver) = unbounded();

        router.insert(WorkerHandle::new(connection_id, sender));

        let handle = router
            .handle(connection_id)
            .expect("registered handle must be found");

        assert_eq!(handle.connection_id(), connection_id,);
    }

    #[test]
    fn reports_missing_handle() {
        let router = ConnectionRouter::default();

        assert!(router.handle(ConnectionId::new(9)).is_none(),);
    }
}
