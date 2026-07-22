use crate::data::NewSeries;

#[derive(Debug)]
pub enum UserCommand {
    AddSeries(NewSeries),

    DeleteSeries { name: String },
}
