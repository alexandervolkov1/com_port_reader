use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::{
    data::SeriesId,
    process_recorder::{
        ProcessAction, ProcessActionId, ProcessActionOrigin, ProcessActionResult,
        ProcessMeasurement, ProcessRecord,
    },
};

#[derive(Clone, Debug, PartialEq)]
pub enum ApplicationEvent {
    Measurements(Vec<ProcessMeasurement>),

    ActionRequested {
        action_id: ProcessActionId,
        origin: ProcessActionOrigin,
        action: ProcessAction,
    },

    ActionApplied {
        action_id: ProcessActionId,
        series_id: Option<SeriesId>,
        series_name: Option<String>,
        result: Option<ProcessActionResult>,
    },

    ActionFailed {
        action_id: ProcessActionId,
        error: String,
    },
}

impl ApplicationEvent {
    pub(crate) fn from_process_record(record: &ProcessRecord) -> Option<Self> {
        match record {
            ProcessRecord::Measurements { measurements } => {
                Some(Self::Measurements(measurements.clone()))
            }

            ProcessRecord::ActionRequested {
                action_id,
                origin,
                action,
                ..
            } => Some(Self::ActionRequested {
                action_id: *action_id,
                origin: *origin,
                action: action.clone(),
            }),

            ProcessRecord::ActionApplied {
                action_id,
                series_id,
                series_name,
                result,
                ..
            } => Some(Self::ActionApplied {
                action_id: *action_id,
                series_id: *series_id,
                series_name: series_name.clone(),
                result: result.clone(),
            }),

            ProcessRecord::ActionFailed {
                action_id, error, ..
            } => Some(Self::ActionFailed {
                action_id: *action_id,
                error: error.clone(),
            }),

            ProcessRecord::Log { .. }
            | ProcessRecord::ConfigurationLoaded { .. }
            | ProcessRecord::ControlOutput { .. } => None,
        }
    }
}

#[derive(Clone, Default)]
pub struct ApplicationEventHub {
    senders: Arc<Mutex<Vec<Sender<ApplicationEvent>>>>,
}

impl ApplicationEventHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(&self) -> Receiver<ApplicationEvent> {
        let (sender, receiver) = unbounded();

        self.senders
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(sender);

        receiver
    }

    pub(crate) fn publish(&self, event: ApplicationEvent) {
        let mut senders = self
            .senders
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        senders.retain(|sender| sender.send(event.clone()).is_ok());
    }
}

#[cfg(test)]
mod tests {
    use super::{ApplicationEvent, ApplicationEventHub};

    #[test]
    fn publishes_to_each_active_subscriber() {
        let hub = ApplicationEventHub::new();
        let first = hub.subscribe();
        let second = hub.subscribe();
        let event = ApplicationEvent::ActionFailed {
            action_id: crate::process_recorder::ProcessActionId::new(7),
            error: "failed".to_owned(),
        };

        hub.publish(event.clone());

        assert_eq!(first.recv().unwrap(), event);
        assert_eq!(second.recv().unwrap(), event);
    }

    #[test]
    fn drops_disconnected_subscribers() {
        let hub = ApplicationEventHub::new();
        let receiver = hub.subscribe();
        drop(receiver);

        hub.publish(ApplicationEvent::ActionFailed {
            action_id: crate::process_recorder::ProcessActionId::new(1),
            error: "failed".to_owned(),
        });

        assert!(
            hub.senders
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_empty()
        );
    }
}
