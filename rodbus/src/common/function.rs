// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

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
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum FunctionCode {
    ReadCoils,
    ReadDiscreteInputs,
    ReadHoldingRegisters,
    ReadInputRegisters,
    WriteSingleCoil,
    WriteSingleRegister,
    WriteMultipleCoils,
    WriteMultipleRegisters,
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
        }
    }
}

impl FunctionCode {
    pub(crate) const fn get_value(self) -> u8 {
        match self {
            FunctionCode::ReadCoils => constants::READ_COILS,
            FunctionCode::ReadDiscreteInputs => constants::READ_DISCRETE_INPUTS,
            FunctionCode::ReadHoldingRegisters => constants::READ_HOLDING_REGISTERS,
            FunctionCode::ReadInputRegisters => constants::READ_INPUT_REGISTERS,
            FunctionCode::WriteSingleCoil => constants::WRITE_SINGLE_COIL,
            FunctionCode::WriteSingleRegister => constants::WRITE_SINGLE_REGISTER,
            FunctionCode::WriteMultipleCoils => constants::WRITE_MULTIPLE_COILS,
            FunctionCode::WriteMultipleRegisters => constants::WRITE_MULTIPLE_REGISTERS,
        }
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
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // MODBUS Application Protocol Specification V1.1b3 6.1-6.6, 6.11, 6.12
    const FUNCTION_CODES: [(u8, FunctionCode); 8] = [
        (0x01, FunctionCode::ReadCoils),
        (0x02, FunctionCode::ReadDiscreteInputs),
        (0x03, FunctionCode::ReadHoldingRegisters),
        (0x04, FunctionCode::ReadInputRegisters),
        (0x05, FunctionCode::WriteSingleCoil),
        (0x06, FunctionCode::WriteSingleRegister),
        (0x0F, FunctionCode::WriteMultipleCoils),
        (0x10, FunctionCode::WriteMultipleRegisters),
    ];

    #[test]
    fn only_the_supported_function_codes_are_recognized_and_each_maps_back_to_its_value() {
        for value in u8::MIN..=u8::MAX {
            let expected = FUNCTION_CODES
                .iter()
                .find(|(code, _)| *code == value)
                .map(|(_, function)| *function);
            assert_eq!(FunctionCode::get(value), expected, "value {value:#04X}");
            if let Some(function) = expected {
                assert_eq!(function.get_value(), value);
                assert_eq!(function.as_error(), value | 0x80);
            }
        }
    }
}
