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
    pub(crate) const SEND_CFC_65: u8 = 65;
    pub(crate) const SEND_CFC_66: u8 = 66;
    pub(crate) const SEND_CFC_67: u8 = 67;
    pub(crate) const SEND_CFC_68: u8 = 68;
    pub(crate) const SEND_CFC_69: u8 = 69;
    pub(crate) const SEND_CFC_70: u8 = 70;
    pub(crate) const SEND_CFC_71: u8 = 71;
    pub(crate) const SEND_CFC_72: u8 = 72;
    pub(crate) const SEND_CFC_100: u8 = 100;
    pub(crate) const SEND_CFC_101: u8 = 101;
    pub(crate) const SEND_CFC_102: u8 = 102;
    pub(crate) const SEND_CFC_103: u8 = 103;
    pub(crate) const SEND_CFC_104: u8 = 104;
    pub(crate) const SEND_CFC_105: u8 = 105;
    pub(crate) const SEND_CFC_106: u8 = 106;
    pub(crate) const SEND_CFC_107: u8 = 107;
    pub(crate) const SEND_CFC_108: u8 = 108;
    pub(crate) const SEND_CFC_109: u8 = 109;
    pub(crate) const SEND_CFC_110: u8 = 110;
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
    SendCFC65 = constants::SEND_CFC_65,
    SendCFC66 = constants::SEND_CFC_66,
    SendCFC67 = constants::SEND_CFC_67,
    SendCFC68 = constants::SEND_CFC_68,
    SendCFC69 = constants::SEND_CFC_69,
    SendCFC70 = constants::SEND_CFC_70,
    SendCFC71 = constants::SEND_CFC_71,
    SendCFC72 = constants::SEND_CFC_72,
    SendCFC100 = constants::SEND_CFC_100,
    SendCFC101 = constants::SEND_CFC_101,
    SendCFC102 = constants::SEND_CFC_102,
    SendCFC103 = constants::SEND_CFC_103,
    SendCFC104 = constants::SEND_CFC_104,
    SendCFC105 = constants::SEND_CFC_105,
    SendCFC106 = constants::SEND_CFC_106,
    SendCFC107 = constants::SEND_CFC_107,
    SendCFC108 = constants::SEND_CFC_108,
    SendCFC109 = constants::SEND_CFC_109,
    SendCFC110 = constants::SEND_CFC_110,
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
            FunctionCode::SendCFC65
            | FunctionCode::SendCFC66
            | FunctionCode::SendCFC67
            | FunctionCode::SendCFC68
            | FunctionCode::SendCFC69
            | FunctionCode::SendCFC70
            | FunctionCode::SendCFC71
            | FunctionCode::SendCFC72
            | FunctionCode::SendCFC100
            | FunctionCode::SendCFC101
            | FunctionCode::SendCFC102
            | FunctionCode::SendCFC103
            | FunctionCode::SendCFC104
            | FunctionCode::SendCFC105
            | FunctionCode::SendCFC106
            | FunctionCode::SendCFC107
            | FunctionCode::SendCFC108
            | FunctionCode::SendCFC109
            | FunctionCode::SendCFC110 => {
                write!(f, "SEND CUSTOM FUNCTION CODE ({:#04X})", self.get_value())
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
            constants::SEND_CFC_65 => Some(FunctionCode::SendCFC65),
            constants::SEND_CFC_66 => Some(FunctionCode::SendCFC66),
            constants::SEND_CFC_67 => Some(FunctionCode::SendCFC67),
            constants::SEND_CFC_68 => Some(FunctionCode::SendCFC68),
            constants::SEND_CFC_69 => Some(FunctionCode::SendCFC69),
            constants::SEND_CFC_70 => Some(FunctionCode::SendCFC70),
            constants::SEND_CFC_71 => Some(FunctionCode::SendCFC71),
            constants::SEND_CFC_72 => Some(FunctionCode::SendCFC72),
            constants::SEND_CFC_100 => Some(FunctionCode::SendCFC100),
            constants::SEND_CFC_101 => Some(FunctionCode::SendCFC101),
            constants::SEND_CFC_102 => Some(FunctionCode::SendCFC102),
            constants::SEND_CFC_103 => Some(FunctionCode::SendCFC103),
            constants::SEND_CFC_104 => Some(FunctionCode::SendCFC104),
            constants::SEND_CFC_105 => Some(FunctionCode::SendCFC105),
            constants::SEND_CFC_106 => Some(FunctionCode::SendCFC106),
            constants::SEND_CFC_107 => Some(FunctionCode::SendCFC107),
            constants::SEND_CFC_108 => Some(FunctionCode::SendCFC108),
            constants::SEND_CFC_109 => Some(FunctionCode::SendCFC109),
            constants::SEND_CFC_110 => Some(FunctionCode::SendCFC110),
            _ => None,
        }
    }
}
