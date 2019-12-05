use crate::function::FunctionCode::Unknown;

pub(crate) mod constants {
    pub const READ_COILS : u8 = 1;
    pub const READ_DISCRETE_INPUTS : u8 = 2;
    pub const READ_HOLDING_REGISTERS : u8 = 3;
    pub const READ_INPUT_REGISTERS : u8 = 4;
    pub const WRITE_SINGLE_COIL : u8 = 5;
    pub const WRITE_SINGLE_REGISTER: u8 = 6;
    pub const WRITE_MULTIPLE_COILS: u8 = 15;
    pub const WRITE_MULTIPLE_REGISTERS: u8 = 16;

    pub const ERROR_DELIMITER: u8 = 0x80;
}

pub enum FunctionCode {
    ReadCoils,
    ReadDiscreteInputs,
    ReadHoldingRegisters,
    ReadInputRegisters,
    WriteSingleCoil,
    WriteSingleRegister,
    WriteMultipleCoils,
    WriteMultipleRegisters,
    Unknown(u8)
}

impl FunctionCode {
    pub fn from_u8(value: u8) -> FunctionCode {
        match value {
            constants::READ_COILS => FunctionCode::ReadCoils,
            constants::READ_DISCRETE_INPUTS => FunctionCode::ReadDiscreteInputs,
            constants::READ_HOLDING_REGISTERS => FunctionCode::ReadHoldingRegisters,
            constants::READ_INPUT_REGISTERS => FunctionCode::ReadInputRegisters,
            constants::WRITE_SINGLE_COIL => FunctionCode::WriteSingleCoil,
            constants::WRITE_SINGLE_REGISTER => FunctionCode::WriteSingleRegister,
            constants::WRITE_MULTIPLE_COILS => FunctionCode::WriteMultipleCoils,
            constants::WRITE_MULTIPLE_REGISTERS => FunctionCode::WriteMultipleRegisters,
            _  => Unknown(value)
        }
    }
}