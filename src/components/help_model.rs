#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HelpLanguage {
    #[default]
    English,
    Russian,
}

#[derive(Default)]
pub struct HelpModel {
    command_reference_open: bool,
    language: HelpLanguage,
    pub(super) search: String,
    pub(super) category: HelpCategory,
    pub(super) document_error: Option<String>,
}

impl HelpLanguage {
    pub(super) fn choose<'a>(self, english: &'a str, russian: &'a str) -> &'a str {
        match self {
            Self::English => english,
            Self::Russian => russian,
        }
    }
}

/// Stable categories shared by catalog filtering and the bilingual navigation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(super) enum HelpCategory {
    #[default]
    All,
    Application,
    Series,
    Instruments,
    Controllers,
    References,
    Panels,
    Scenarios,
    Setup,
}

impl HelpCategory {
    pub const ALL: [Self; 9] = [
        Self::All,
        Self::Application,
        Self::Series,
        Self::Instruments,
        Self::Controllers,
        Self::References,
        Self::Panels,
        Self::Scenarios,
        Self::Setup,
    ];

    pub fn label(self, language: HelpLanguage) -> &'static str {
        let (en, ru) = match self {
            Self::All => ("All", "Все"),
            Self::Application => ("Application", "Приложение"),
            Self::Series => ("Series & filters", "Серии и фильтры"),
            Self::Instruments => ("Instruments", "Приборы"),
            Self::Controllers => ("Controllers", "Регуляторы"),
            Self::References => ("References & diagnostics", "Уставки и диагностики"),
            Self::Panels => ("Panels", "Панели"),
            Self::Scenarios => ("Scenarios", "Сценарии"),
            Self::Setup => ("Profiles & models", "Профили и модели"),
        };
        language.choose(en, ru)
    }
}

impl HelpModel {
    pub fn open_command_reference(&mut self) {
        self.command_reference_open = true;
    }

    pub fn command_reference_open(&self) -> bool {
        self.command_reference_open
    }

    pub fn set_command_reference_open(&mut self, open: bool) {
        self.command_reference_open = open;
    }

    pub const fn language(&self) -> HelpLanguage {
        self.language
    }

    pub fn set_language(&mut self, language: HelpLanguage) {
        self.language = language;
    }
}
