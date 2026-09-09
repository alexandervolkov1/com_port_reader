use std::collections::{BTreeSet, HashMap};

use crate::{
    app_log::LogHandle,
    process_recorder::{ProcessActionContext, ProcessActionId, ProcessRecorder},
    user_command::AcquisitionCommand,
    worker::ConnectionRouter,
};

use super::{
    AcquisitionController,
    command_dispatcher::{
        AcquisitionActionKind, PendingAcquisitionAction, rollback_acquisition_connections,
    },
};

pub(crate) struct AcquisitionCommandHandler<'a> {
    controls: &'a mut AcquisitionController,
    connections: &'a ConnectionRouter,
    log: &'a LogHandle,
    recorder: &'a ProcessRecorder,
    pending_actions: &'a mut HashMap<ProcessActionId, PendingAcquisitionAction>,
}

impl<'a> AcquisitionCommandHandler<'a> {
    pub(crate) fn new(
        controls: &'a mut AcquisitionController,
        connections: &'a ConnectionRouter,
        log: &'a LogHandle,
        recorder: &'a ProcessRecorder,
        pending_actions: &'a mut HashMap<ProcessActionId, PendingAcquisitionAction>,
    ) -> Self {
        Self {
            controls,
            connections,
            log,
            recorder,
            pending_actions,
        }
    }

    pub(crate) fn execute(
        &mut self,
        command: AcquisitionCommand,
        action_context: Option<ProcessActionContext>,
    ) {
        match command {
            AcquisitionCommand::Start => {
                self.start(action_context);
            }

            AcquisitionCommand::Stop => {
                self.stop(action_context);
            }
        }
    }

    fn start(&mut self, action_context: Option<ProcessActionContext>) {
        let action_id = action_context.map(|context| context.action_id());

        let rollback_connections = self.controls.stopped_connection_ids();

        if let Some(action_id) = action_id {
            self.pending_actions.insert(
                action_id,
                PendingAcquisitionAction {
                    kind: AcquisitionActionKind::Start,
                    expected_responses: self.controls.worker_count(),
                    responded_connections: BTreeSet::new(),
                    rollback_connections: rollback_connections.clone(),
                },
            );
        }

        if let Err(error) = self.controls.start(action_id) {
            let error = format!("Failed to start acquisition: {error}",);

            rollback_acquisition_connections(self.connections, self.log, &rollback_connections);

            if let Some(action_id) = action_id {
                self.pending_actions.remove(&action_id);

                self.recorder.record_action_failed(action_id, error);
            }
        }
    }

    fn stop(&mut self, action_context: Option<ProcessActionContext>) {
        let action_id = action_context.map(|context| context.action_id());

        if let Some(action_id) = action_id {
            self.pending_actions.insert(
                action_id,
                PendingAcquisitionAction {
                    kind: AcquisitionActionKind::Stop,
                    expected_responses: self.controls.worker_count(),
                    responded_connections: BTreeSet::new(),
                    rollback_connections: BTreeSet::new(),
                },
            );
        }

        if let Err(error) = self.controls.stop(action_id) {
            let error = format!("Failed to stop acquisition: {error}",);

            if let Some(action_id) = action_id {
                self.pending_actions.remove(&action_id);

                self.recorder.record_action_failed(action_id, error);
            }
        }
    }
}
