#[derive(Default)]
pub struct SettingsModel {
    open: bool,
}

impl SettingsModel {
    pub const fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }
}
