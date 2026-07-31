use crate::{
    protocol::metakon::{
        ReadRegisterError, ReadRegisterRequest, RegisterDataType, RegisterValue,
        WriteRegisterRequest, WriteRegisterValue, read_register,
    },
    serial_connection::SerialConnection,
};

pub const DEFAULT_DEVICE: u8 = 1;
pub const DEFAULT_CHANNEL: u8 = 0;
pub const CHANNEL_TYPE_CODE: u8 = 0x03;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metakon5x3 {
    device: u8,
    channel: u8,
}

impl Metakon5x3 {
    pub const fn new(device: u8, channel: u8) -> Self {
        Self { device, channel }
    }

    pub const fn device(self) -> u8 {
        self.device
    }

    pub const fn channel(self) -> u8 {
        self.channel
    }

    pub const fn read_request(self, register: Metakon5x3Register) -> ReadRegisterRequest {
        ReadRegisterRequest::new(self.device, self.channel, register.address())
    }

    pub fn read(
        self,
        connection: &mut SerialConnection,
        register: Metakon5x3Register,
    ) -> Result<RegisterValue, Metakon5x3ReadError> {
        read_register(
            connection,
            self.read_request(register),
            register.data_type(),
        )
        .map_err(|source| Metakon5x3ReadError {
            device: self.device,
            channel: self.channel,
            register,
            source,
        })
    }

    pub fn write_request(
        self,
        parameter: Metakon5x3Write,
    ) -> Result<WriteRegisterRequest, Metakon5x3ValueError> {
        parameter.validate()?;

        Ok(WriteRegisterRequest::new(
            self.device,
            self.channel,
            parameter.register().address(),
            parameter.value(),
        ))
    }
}

impl Default for Metakon5x3 {
    fn default() -> Self {
        Self::new(DEFAULT_DEVICE, DEFAULT_CHANNEL)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metakon5x3Register {
    ChannelType,
    Measurement,
    Setpoint,
    ProportionalBand,
    IntegralTime,
    DerivativeTime,
    OutputPower,
    PwmPositive,
    PwmNegative,
    UpperSetpoint,
    UpperHysteresis,
    UpperOutput,
    LowerSetpoint,
    LowerHysteresis,
    LowerOutput,
}

impl Metakon5x3Register {
    pub const fn from_address(address: u8) -> Option<Self> {
        match address {
            0x00 => Some(Self::ChannelType),
            0x01 => Some(Self::Measurement),
            0x02 => Some(Self::Setpoint),
            0x03 => Some(Self::ProportionalBand),
            0x04 => Some(Self::IntegralTime),
            0x05 => Some(Self::DerivativeTime),
            0x06 => Some(Self::OutputPower),
            0x07 => Some(Self::PwmPositive),
            0x08 => Some(Self::PwmNegative),
            0x09 => Some(Self::UpperSetpoint),
            0x0A => Some(Self::UpperHysteresis),
            0x0B => Some(Self::UpperOutput),
            0x0C => Some(Self::LowerSetpoint),
            0x0D => Some(Self::LowerHysteresis),
            0x0E => Some(Self::LowerOutput),
            _ => None,
        }
    }

    pub const fn address(self) -> u8 {
        match self {
            Self::ChannelType => 0x00,
            Self::Measurement => 0x01,
            Self::Setpoint => 0x02,
            Self::ProportionalBand => 0x03,
            Self::IntegralTime => 0x04,
            Self::DerivativeTime => 0x05,
            Self::OutputPower => 0x06,
            Self::PwmPositive => 0x07,
            Self::PwmNegative => 0x08,
            Self::UpperSetpoint => 0x09,
            Self::UpperHysteresis => 0x0A,
            Self::UpperOutput => 0x0B,
            Self::LowerSetpoint => 0x0C,
            Self::LowerHysteresis => 0x0D,
            Self::LowerOutput => 0x0E,
        }
    }

    pub const fn data_type(self) -> RegisterDataType {
        match self {
            Self::ChannelType
            | Self::DerivativeTime
            | Self::UpperHysteresis
            | Self::LowerHysteresis => RegisterDataType::Ubyte,

            Self::Measurement | Self::Setpoint | Self::UpperSetpoint | Self::LowerSetpoint => {
                RegisterDataType::Int
            }

            Self::ProportionalBand | Self::IntegralTime => RegisterDataType::Uint,

            Self::OutputPower => RegisterDataType::Byte,

            Self::PwmPositive | Self::PwmNegative | Self::UpperOutput | Self::LowerOutput => {
                RegisterDataType::Bool
            }
        }
    }

    pub const fn writable(self) -> bool {
        !matches!(
            self,
            Self::ChannelType | Self::Measurement | Self::PwmPositive | Self::PwmNegative
        )
    }
}

impl std::fmt::Display for Metakon5x3Register {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::ChannelType => "channel type",
            Self::Measurement => "measurement",
            Self::Setpoint => "setpoint",
            Self::ProportionalBand => "proportional band",
            Self::IntegralTime => "integral time",
            Self::DerivativeTime => "derivative time",
            Self::OutputPower => "output power",
            Self::PwmPositive => "PWM positive output",
            Self::PwmNegative => "PWM negative output",
            Self::UpperSetpoint => "upper setpoint",
            Self::UpperHysteresis => "upper hysteresis",
            Self::UpperOutput => "upper output",
            Self::LowerSetpoint => "lower setpoint",
            Self::LowerHysteresis => "lower hysteresis",
            Self::LowerOutput => "lower output",
        };

        formatter.write_str(name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metakon5x3Write {
    Setpoint(i16),
    ProportionalBand(u16),
    IntegralTime(u16),
    DerivativeTime(u8),
    OutputPower(i8),
    UpperSetpoint(i16),
    UpperHysteresis(u8),
    UpperOutput(bool),
    LowerSetpoint(i16),
    LowerHysteresis(u8),
    LowerOutput(bool),
}

impl Metakon5x3Write {
    pub const fn register(self) -> Metakon5x3Register {
        match self {
            Self::Setpoint(_) => Metakon5x3Register::Setpoint,

            Self::ProportionalBand(_) => Metakon5x3Register::ProportionalBand,

            Self::IntegralTime(_) => Metakon5x3Register::IntegralTime,

            Self::DerivativeTime(_) => Metakon5x3Register::DerivativeTime,

            Self::OutputPower(_) => Metakon5x3Register::OutputPower,

            Self::UpperSetpoint(_) => Metakon5x3Register::UpperSetpoint,

            Self::UpperHysteresis(_) => Metakon5x3Register::UpperHysteresis,

            Self::UpperOutput(_) => Metakon5x3Register::UpperOutput,

            Self::LowerSetpoint(_) => Metakon5x3Register::LowerSetpoint,

            Self::LowerHysteresis(_) => Metakon5x3Register::LowerHysteresis,

            Self::LowerOutput(_) => Metakon5x3Register::LowerOutput,
        }
    }

    pub const fn value(self) -> WriteRegisterValue {
        match self {
            Self::Setpoint(value) | Self::UpperSetpoint(value) | Self::LowerSetpoint(value) => {
                WriteRegisterValue::Int(value)
            }

            Self::ProportionalBand(value) | Self::IntegralTime(value) => {
                WriteRegisterValue::Uint(value)
            }

            Self::DerivativeTime(value)
            | Self::UpperHysteresis(value)
            | Self::LowerHysteresis(value) => WriteRegisterValue::Ubyte(value),

            Self::OutputPower(value) => WriteRegisterValue::Byte(value),

            Self::UpperOutput(value) | Self::LowerOutput(value) => WriteRegisterValue::Bool(value),
        }
    }

    fn validate(self) -> Result<(), Metakon5x3ValueError> {
        match self {
            Self::Setpoint(value) | Self::UpperSetpoint(value) | Self::LowerSetpoint(value) => {
                validate_range(self.register(), i64::from(value), -999, 9_999)
            }

            Self::ProportionalBand(value) => {
                validate_range(self.register(), i64::from(value), 1, 9_999)
            }

            Self::IntegralTime(value) => {
                validate_range(self.register(), i64::from(value), 1, 30_000)
            }

            Self::OutputPower(value) => {
                validate_range(self.register(), i64::from(value), -100, 100)
            }

            Self::DerivativeTime(_)
            | Self::UpperHysteresis(_)
            | Self::UpperOutput(_)
            | Self::LowerHysteresis(_)
            | Self::LowerOutput(_) => Ok(()),
        }
    }
}

#[derive(Debug)]
pub struct Metakon5x3ReadError {
    device: u8,
    channel: u8,
    register: Metakon5x3Register,
    source: ReadRegisterError,
}

impl std::fmt::Display for Metakon5x3ReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Metakon 5X3 device {}, channel {}, {} \
             register 0x{:02X} read failed: {}",
            self.device,
            self.channel,
            self.register,
            self.register.address(),
            self.source,
        )
    }
}

impl std::error::Error for Metakon5x3ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metakon5x3ValueError {
    register: Metakon5x3Register,
    value: i64,
    minimum: i64,
    maximum: i64,
}

impl std::fmt::Display for Metakon5x3ValueError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Metakon 5X3 {} must be between {} and {}, received {}",
            self.register, self.minimum, self.maximum, self.value,
        )
    }
}

impl std::error::Error for Metakon5x3ValueError {}

fn validate_range(
    register: Metakon5x3Register,
    value: i64,
    minimum: i64,
    maximum: i64,
) -> Result<(), Metakon5x3ValueError> {
    if (minimum..=maximum).contains(&value) {
        return Ok(());
    }

    Err(Metakon5x3ValueError {
        register,
        value,
        minimum,
        maximum,
    })
}

#[cfg(test)]
mod tests {
    use super::{CHANNEL_TYPE_CODE, Metakon5x3, Metakon5x3Register, Metakon5x3Write};

    use crate::protocol::metakon::{
        ReadRegisterRequest, RegisterDataType, WriteRegisterRequest, WriteRegisterValue,
    };

    #[test]
    fn channel_type_code_is_three() {
        assert_eq!(CHANNEL_TYPE_CODE, 0x03);
    }

    #[test]
    fn describes_all_channel_registers() {
        let registers = [
            (
                Metakon5x3Register::ChannelType,
                0x00,
                RegisterDataType::Ubyte,
                false,
            ),
            (
                Metakon5x3Register::Measurement,
                0x01,
                RegisterDataType::Int,
                false,
            ),
            (
                Metakon5x3Register::Setpoint,
                0x02,
                RegisterDataType::Int,
                true,
            ),
            (
                Metakon5x3Register::ProportionalBand,
                0x03,
                RegisterDataType::Uint,
                true,
            ),
            (
                Metakon5x3Register::IntegralTime,
                0x04,
                RegisterDataType::Uint,
                true,
            ),
            (
                Metakon5x3Register::DerivativeTime,
                0x05,
                RegisterDataType::Ubyte,
                true,
            ),
            (
                Metakon5x3Register::OutputPower,
                0x06,
                RegisterDataType::Byte,
                true,
            ),
            (
                Metakon5x3Register::PwmPositive,
                0x07,
                RegisterDataType::Bool,
                false,
            ),
            (
                Metakon5x3Register::PwmNegative,
                0x08,
                RegisterDataType::Bool,
                false,
            ),
            (
                Metakon5x3Register::UpperSetpoint,
                0x09,
                RegisterDataType::Int,
                true,
            ),
            (
                Metakon5x3Register::UpperHysteresis,
                0x0A,
                RegisterDataType::Ubyte,
                true,
            ),
            (
                Metakon5x3Register::UpperOutput,
                0x0B,
                RegisterDataType::Bool,
                true,
            ),
            (
                Metakon5x3Register::LowerSetpoint,
                0x0C,
                RegisterDataType::Int,
                true,
            ),
            (
                Metakon5x3Register::LowerHysteresis,
                0x0D,
                RegisterDataType::Ubyte,
                true,
            ),
            (
                Metakon5x3Register::LowerOutput,
                0x0E,
                RegisterDataType::Bool,
                true,
            ),
        ];

        for (register, address, data_type, writable) in registers {
            assert_eq!(register.address(), address);
            assert_eq!(register.data_type(), data_type);
            assert_eq!(register.writable(), writable);
        }
    }

    #[test]
    fn creates_read_request() {
        let instrument = Metakon5x3::new(15, 0);

        assert_eq!(
            instrument.read_request(Metakon5x3Register::Measurement),
            ReadRegisterRequest::new(15, 0, 0x01),
        );
    }

    #[test]
    fn creates_setpoint_request() {
        let instrument = Metakon5x3::new(15, 0);

        assert_eq!(
            instrument.write_request(Metakon5x3Write::Setpoint(150)),
            Ok(WriteRegisterRequest::new(
                15,
                0,
                0x02,
                WriteRegisterValue::Int(150),
            )),
        );
    }

    #[test]
    fn rejects_invalid_setpoint() {
        let instrument = Metakon5x3::new(15, 0);

        assert!(
            instrument
                .write_request(Metakon5x3Write::Setpoint(10_000))
                .is_err()
        );
    }

    #[test]
    fn rejects_invalid_proportional_band() {
        let instrument = Metakon5x3::new(15, 0);

        assert!(
            instrument
                .write_request(Metakon5x3Write::ProportionalBand(0),)
                .is_err()
        );
    }

    #[test]
    fn rejects_invalid_integral_time() {
        let instrument = Metakon5x3::new(15, 0);

        assert!(
            instrument
                .write_request(Metakon5x3Write::IntegralTime(0))
                .is_err()
        );

        assert!(
            instrument
                .write_request(Metakon5x3Write::IntegralTime(30_001),)
                .is_err()
        );
    }

    #[test]
    fn rejects_invalid_output_power() {
        let instrument = Metakon5x3::new(15, 0);

        assert!(
            instrument
                .write_request(Metakon5x3Write::OutputPower(101),)
                .is_err()
        );
    }

    #[test]
    fn creates_boolean_output_request() {
        let instrument = Metakon5x3::new(15, 0);

        assert_eq!(
            instrument.write_request(Metakon5x3Write::UpperOutput(true),),
            Ok(WriteRegisterRequest::new(
                15,
                0,
                0x0B,
                WriteRegisterValue::Bool(true),
            )),
        );
    }

    #[test]
    fn uses_default_address() {
        let instrument = Metakon5x3::default();

        assert_eq!(instrument.device(), 1);
        assert_eq!(instrument.channel(), 0);
    }

    #[test]
    fn finds_register_by_address() {
        for address in 0x00..=0x0E {
            let register =
                Metakon5x3Register::from_address(address).expect("documented register must exist");

            assert_eq!(register.address(), address);
        }
    }

    #[test]
    fn rejects_unknown_register_address() {
        assert_eq!(Metakon5x3Register::from_address(0x0F), None,);

        assert_eq!(Metakon5x3Register::from_address(0xFF), None,);
    }
}
