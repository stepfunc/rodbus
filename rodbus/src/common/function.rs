use std::fmt::{Display, Formatter};

mod constants {
    pub(crate) const READ_COILS: u8 = 1;
    pub(crate) const READ_DISCRETE_INPUTS: u8 = 2;
    pub(crate) const READ_HOLDING_REGISTERS: u8 = 3;
    pub(crate) const READ_INPUT_REGISTERS: u8 = 4;
    pub(crate) const WRITE_SINGLE_COIL: u8 = 5;
    pub(crate) const WRITE_SINGLE_REGISTER: u8 = 6;
    pub(crate) const WRITE_MULTIPLE_COILS: u8 = 15;
    pub(crate) const WRITE_MULTIPLE_REGISTERS: u8 = 16;
    pub(crate) const WRITE_CUSTOM_FUNCTION_CODE: u8 = 69;
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub(crate) enum FunctionCode {
    ReadCoils = constants::READ_COILS,
    ReadDiscreteInputs = constants::READ_DISCRETE_INPUTS,
    ReadHoldingRegisters = constants::READ_HOLDING_REGISTERS,
    ReadInputRegisters = constants::READ_INPUT_REGISTERS,
    WriteSingleCoil = constants::WRITE_SINGLE_COIL,
    WriteSingleRegister = constants::WRITE_SINGLE_REGISTER,
    WriteMultipleCoils = constants::WRITE_MULTIPLE_COILS,
    WriteMultipleRegisters = constants::WRITE_MULTIPLE_REGISTERS,
    WriteCustomFunctionCode = constants::WRITE_CUSTOM_FUNCTION_CODE,
}

impl Display for FunctionCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            FunctionCode::ReadCoils => write!(f, "READ COILS ({:#04X})", self.get_value()),
            FunctionCode::ReadDiscreteInputs => {
                write!(f, "READ DISCRETE INPUTS ({:#04X})", self.get_value())
            }
            FunctionCode::ReadHoldingRegisters => {
                write!(f, "READ HOLDING REGISTERS ({:#04X})", self.get_value())
            }
            FunctionCode::ReadInputRegisters => {
                write!(f, "READ INPUT REGISTERS ({:#04X})", self.get_value())
            }
            FunctionCode::WriteSingleCoil => {
                write!(f, "WRITE SINGLE COIL ({:#04X})", self.get_value())
            }
            FunctionCode::WriteSingleRegister => {
                write!(f, "WRITE SINGLE REGISTER ({:#04X})", self.get_value())
            }
            FunctionCode::WriteMultipleCoils => {
                write!(f, "WRITE MULTIPLE COILS ({:#04X})", self.get_value())
            }
            FunctionCode::WriteMultipleRegisters => {
                write!(f, "WRITE MULTIPLE REGISTERS ({:#04X})", self.get_value())
            }
            FunctionCode::WriteCustomFunctionCode => {
                write!(f, "WRITE CUSTOM FUNCTION CODE ({:#04X})", self.get_value())
            }
        }
    }
}

impl FunctionCode {
    pub(crate) const fn get_value(self) -> u8 {
        self as u8
    }

    pub(crate) const fn as_error(self) -> u8 {
        self.get_value() | 0x80
    }

    pub(crate) fn get(value: u8) -> Option<Self> {
        match value {
            constants::READ_COILS => Some(FunctionCode::ReadCoils),
            constants::READ_DISCRETE_INPUTS => Some(FunctionCode::ReadDiscreteInputs),
            constants::READ_HOLDING_REGISTERS => Some(FunctionCode::ReadHoldingRegisters),
            constants::READ_INPUT_REGISTERS => Some(FunctionCode::ReadInputRegisters),
            constants::WRITE_SINGLE_COIL => Some(FunctionCode::WriteSingleCoil),
            constants::WRITE_SINGLE_REGISTER => Some(FunctionCode::WriteSingleRegister),
            constants::WRITE_MULTIPLE_COILS => Some(FunctionCode::WriteMultipleCoils),
            constants::WRITE_MULTIPLE_REGISTERS => Some(FunctionCode::WriteMultipleRegisters),
            constants::WRITE_CUSTOM_FUNCTION_CODE => Some(FunctionCode::WriteCustomFunctionCode),
            _ => None,
        }
    }
}
