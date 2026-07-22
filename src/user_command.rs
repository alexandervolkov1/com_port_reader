use crate::data::NewSeries;

#[derive(Debug)]
pub enum UserCommand {
    Add(NewSeries),

    Delete {
        name: String,
    },

    Rename {
        current_name: String,
        new_name: String,
    },
}
