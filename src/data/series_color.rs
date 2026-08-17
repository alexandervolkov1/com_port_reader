use std::{error::Error, fmt, str::FromStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeriesColor {
    red: u8,
    green: u8,
    blue: u8,
}

impl SeriesColor {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub const fn red(self) -> u8 {
        self.red
    }

    pub const fn green(self) -> u8 {
        self.green
    }

    pub const fn blue(self) -> u8 {
        self.blue
    }
}

impl fmt::Display for SeriesColor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "#{:02X}{:02X}{:02X}",
            self.red, self.green, self.blue,
        )
    }
}

impl FromStr for SeriesColor {
    type Err = SeriesColorParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some(hex) = value.strip_prefix('#') else {
            return Err(SeriesColorParseError);
        };

        if hex.len() != 6 || !hex.is_ascii() {
            return Err(SeriesColorParseError);
        }

        let component = |range: std::ops::Range<usize>| {
            u8::from_str_radix(&hex[range], 16).map_err(|_| SeriesColorParseError)
        };

        Ok(Self::new(
            component(0..2)?,
            component(2..4)?,
            component(4..6)?,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeriesColorParseError;

impl fmt::Display for SeriesColorParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Series color must use the #RRGGBB format")
    }
}

impl Error for SeriesColorParseError {}

#[cfg(test)]
mod tests {
    use super::{SeriesColor, SeriesColorParseError};

    #[test]
    fn parses_series_color() {
        assert_eq!(
            "#1A2B3C".parse::<SeriesColor>(),
            Ok(SeriesColor::new(0x1A, 0x2B, 0x3C)),
        );

        assert_eq!(
            "#a0b1c2".parse::<SeriesColor>(),
            Ok(SeriesColor::new(0xA0, 0xB1, 0xC2)),
        );
    }

    #[test]
    fn formats_series_color() {
        assert_eq!(SeriesColor::new(0x01, 0xAB, 0xF0).to_string(), "#01ABF0",);
    }

    #[test]
    fn rejects_invalid_series_colors() {
        for value in ["", "112233", "#12345", "#1234567", "#12GG56", "#FFFFFF00"] {
            assert_eq!(value.parse::<SeriesColor>(), Err(SeriesColorParseError),);
        }
    }
}
