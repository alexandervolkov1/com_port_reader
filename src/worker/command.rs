use crate::data::Signal;

pub enum WorkerCommand {
    Start,
    Stop,
    AddSignal(Signal),
    Shutdown,
}
