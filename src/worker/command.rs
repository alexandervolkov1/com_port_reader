use crate::data::Signal;

pub enum WorkerCommand {
    AddSignal(Signal),
}
