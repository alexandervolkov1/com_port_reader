use super::PidController;

#[derive(Debug)]
pub enum Controller {
    Pid(PidController),
}

impl From<PidController> for Controller {
    fn from(controller: PidController) -> Self {
        Self::Pid(controller)
    }
}
