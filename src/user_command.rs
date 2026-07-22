use crate::data::NewSeries;

#[derive(Debug)]
pub enum UserCommand {
    AddSeries(NewSeries),
    DeleteSeries {
        name: String,
    },
    RenameSeries {
        current_name: String,
        new_name: String,
    },
}
