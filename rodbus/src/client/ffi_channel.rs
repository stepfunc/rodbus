use crate::client::message::{Command, Promise, RequestDetails, Setting};
use crate::client::requests::read_bits::ReadBits;
use crate::client::requests::read_registers::ReadRegisters;
use crate::client::requests::write_multiple::MultipleWriteRequest;
use crate::client::requests::write_single::SingleWrite;
use crate::client::{Channel, RequestParam, WriteMultiple};
use crate::{
    AddressRange, BitIterator, DecodeLevel, Indexed, InvalidRange, RegisterIterator, RequestError,
};
use tokio::sync::mpsc::error::TrySendError;

/// Callback-based, non-async session used only in combination with the FFI library.
///
/// No semver guarantees are applied to this type.
#[derive(Debug, Clone)]
pub struct FfiChannel {
    tx: tokio::sync::mpsc::Sender<Command>,
}

/// Errors returned on methods of the FfiSession
#[derive(Copy, Clone, Debug)]
pub enum FfiChannelError {
    /// Channel is full
    ChannelFull,
    /// Channel is closed
    ChannelClosed,
    /// Bad range value
    BadRange(InvalidRange),
}

impl FfiChannel {
    /// Create a [FfiChannel] from a [Channel] and the specified [RequestParam]
    pub fn new(channel: Channel) -> Self {
        Self { tx: channel.tx }
    }

    /// Enable the channel
    pub fn enable(&mut self) -> Result<(), FfiChannelError> {
        self.send(Command::Setting(Setting::Enable))
    }

    /// Disable the channel
    pub fn disable(&mut self) -> Result<(), FfiChannelError> {
        self.send(Command::Setting(Setting::Disable))
    }

    /// Set the decode level for the channel
    pub fn set_decode_level(&mut self, level: DecodeLevel) -> Result<(), FfiChannelError> {
        self.send(Command::Setting(Setting::DecodeLevel(level)))
    }

    /// Read coils from the server
    pub fn read_coils<C>(
        &mut self,
        param: RequestParam,
        range: AddressRange,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_bits(param, range, callback, RequestDetails::ReadCoils)
    }

    /// Read discrete inputs from the server
    pub fn read_discrete_inputs<C>(
        &mut self,
        param: RequestParam,
        range: AddressRange,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_bits(param, range, callback, RequestDetails::ReadDiscreteInputs)
    }

    /// Read holding registers from the server
    pub fn read_holding_registers<C>(
        &mut self,
        param: RequestParam,
        range: AddressRange,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_registers(param, range, callback, RequestDetails::ReadHoldingRegisters)
    }

    /// Read input registers from the server
    pub fn read_input_registers<C>(
        &mut self,
        param: RequestParam,
        range: AddressRange,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_registers(param, range, callback, RequestDetails::ReadInputRegisters)
    }

    /// Write a single coil to the server
    pub fn write_single_coil<C>(
        &mut self,
        param: RequestParam,
        value: Indexed<bool>,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<Indexed<bool>, RequestError>) + Send + Sync + 'static,
    {
        self.send(crate::client::channel::wrap(
            param,
            RequestDetails::WriteSingleCoil(SingleWrite::new(value, Promise::new(callback))),
        ))
    }

    /// Write a single registers to the server
    pub fn write_single_register<C>(
        &mut self,
        param: RequestParam,
        value: Indexed<u16>,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<Indexed<u16>, RequestError>) + Send + Sync + 'static,
    {
        self.send(crate::client::channel::wrap(
            param,
            RequestDetails::WriteSingleRegister(SingleWrite::new(value, Promise::new(callback))),
        ))
    }

    /// Write multiple contiguous registers to the server
    pub fn write_multiple_registers<C>(
        &mut self,
        param: RequestParam,
        value: WriteMultiple<u16>,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<AddressRange, RequestError>) + Send + Sync + 'static,
    {
        self.send(crate::client::channel::wrap(
            param,
            RequestDetails::WriteMultipleRegisters(MultipleWriteRequest::new(
                value,
                Promise::new(callback),
            )),
        ))
    }

    /// Write multiple contiguous coils to the server
    pub fn write_multiple_coils<C>(
        &mut self,
        param: RequestParam,
        value: WriteMultiple<bool>,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<AddressRange, RequestError>) + Send + Sync + 'static,
    {
        self.send(crate::client::channel::wrap(
            param,
            RequestDetails::WriteMultipleCoils(MultipleWriteRequest::new(
                value,
                Promise::new(callback),
            )),
        ))
    }

    fn read_bits<C, W>(
        &mut self,
        param: RequestParam,
        range: AddressRange,
        callback: C,
        wrap_req: W,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static,
        W: Fn(ReadBits) -> RequestDetails,
    {
        let range = range.of_read_bits()?;
        let promise = crate::client::requests::read_bits::Promise::new(callback);
        self.send(crate::client::channel::wrap(
            param,
            wrap_req(ReadBits::new(range, promise)),
        ))
    }

    fn read_registers<C, W>(
        &mut self,
        param: RequestParam,
        range: AddressRange,
        callback: C,
        wrap_req: W,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static,
        W: Fn(ReadRegisters) -> RequestDetails,
    {
        let promise = crate::client::requests::read_registers::Promise::new(callback);
        let range = range.of_read_registers()?;
        self.send(crate::client::channel::wrap(
            param,
            wrap_req(ReadRegisters::new(range, promise)),
        ))
    }

    fn send(&mut self, command: Command) -> Result<(), FfiChannelError> {
        // dropping the command will automatically fail requests with SHUTDOWN
        self.tx.try_send(command)?;
        Ok(())
    }
}

impl From<InvalidRange> for FfiChannelError {
    fn from(err: InvalidRange) -> FfiChannelError {
        Self::BadRange(err)
    }
}

impl<T> From<TrySendError<T>> for FfiChannelError {
    fn from(err: TrySendError<T>) -> FfiChannelError {
        match err {
            TrySendError::Full(_) => FfiChannelError::ChannelFull,
            TrySendError::Closed(_) => FfiChannelError::ChannelClosed,
        }
    }
}
