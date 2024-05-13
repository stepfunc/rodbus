use crate::client::message::{Command, Promise, RequestDetails};
use crate::client::requests::read_bits::ReadBits;
use crate::client::requests::read_registers::ReadRegisters;
use crate::client::requests::write_multiple::MultipleWriteRequest;
use crate::client::requests::write_single::SingleWrite;
use crate::client::{Channel, RequestParam, WriteMultiple};
use crate::{AddressRange, BitIterator, Indexed, InvalidRange, RegisterIterator, RequestError};
use tokio::sync::mpsc::error::TrySendError;

/// Callback-based, non-async session used only in combination with the FFI library.
///
/// No semver guarantees are applied to this type.
#[derive(Debug, Clone)]
pub struct FfiSession {
    tx: tokio::sync::mpsc::Sender<Command>,
    param: RequestParam,
}

/// Errors returned on methods of the FfiSession
#[derive(Copy, Clone, Debug)]
pub enum FfiSessionError {
    /// Channel is full
    ChannelFull,
    /// Channel is closed
    ChannelClosed,
    /// Bad range value
    BadRange(InvalidRange),
}

impl FfiSession {
    /// Create a [FfiSession] from a [Channel] and the specified [RequestParam]
    pub fn new(channel: Channel, param: RequestParam) -> Self {
        Self {
            tx: channel.tx,
            param,
        }
    }

    /// Read coils from the server
    pub fn read_coils<C>(&mut self, range: AddressRange, callback: C) -> Result<(), FfiSessionError>
    where
        C: FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_bits(range, callback, RequestDetails::ReadCoils)
    }

    /// Read discrete inputs from the server
    pub fn read_discrete_inputs<C>(
        &mut self,
        range: AddressRange,
        callback: C,
    ) -> Result<(), FfiSessionError>
    where
        C: FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_bits(range, callback, RequestDetails::ReadDiscreteInputs)
    }

    /// Read holding registers from the server
    pub fn read_holding_registers<C>(
        &mut self,
        range: AddressRange,
        callback: C,
    ) -> Result<(), FfiSessionError>
    where
        C: FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_registers(range, callback, RequestDetails::ReadHoldingRegisters)
    }

    /// Read input registers from the server
    pub fn read_input_registers<C>(
        &mut self,
        range: AddressRange,
        callback: C,
    ) -> Result<(), FfiSessionError>
    where
        C: FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_registers(range, callback, RequestDetails::ReadInputRegisters)
    }

    /// Write a single coil to the server
    pub fn write_single_coil<C>(
        &mut self,
        value: Indexed<bool>,
        callback: C,
    ) -> Result<(), FfiSessionError>
    where
        C: FnOnce(Result<Indexed<bool>, RequestError>) + Send + Sync + 'static,
    {
        self.send(crate::client::channel::wrap(
            self.param,
            RequestDetails::WriteSingleCoil(SingleWrite::new(value, Promise::new(callback))),
        ))
    }

    /// Write a single registers to the server
    pub fn write_single_register<C>(
        &mut self,
        value: Indexed<u16>,
        callback: C,
    ) -> Result<(), FfiSessionError>
    where
        C: FnOnce(Result<Indexed<u16>, RequestError>) + Send + Sync + 'static,
    {
        self.send(crate::client::channel::wrap(
            self.param,
            RequestDetails::WriteSingleRegister(SingleWrite::new(value, Promise::new(callback))),
        ))
    }

    /// Write multiple contiguous registers to the server
    pub fn write_multiple_registers<C>(
        &mut self,
        value: WriteMultiple<u16>,
        callback: C,
    ) -> Result<(), FfiSessionError>
    where
        C: FnOnce(Result<AddressRange, RequestError>) + Send + Sync + 'static,
    {
        self.send(crate::client::channel::wrap(
            self.param,
            RequestDetails::WriteMultipleRegisters(MultipleWriteRequest::new(
                value,
                Promise::new(callback),
            )),
        ))
    }

    /// Write multiple contiguous coils to the server
    pub fn write_multiple_coils<C>(
        &mut self,
        value: WriteMultiple<bool>,
        callback: C,
    ) -> Result<(), FfiSessionError>
    where
        C: FnOnce(Result<AddressRange, RequestError>) + Send + Sync + 'static,
    {
        self.send(crate::client::channel::wrap(
            self.param,
            RequestDetails::WriteMultipleCoils(MultipleWriteRequest::new(
                value,
                Promise::new(callback),
            )),
        ))
    }

    fn read_bits<C, W>(
        &mut self,
        range: AddressRange,
        callback: C,
        wrap_req: W,
    ) -> Result<(), FfiSessionError>
    where
        C: FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static,
        W: Fn(ReadBits) -> RequestDetails,
    {
        let range = range.of_read_bits()?;
        let promise = crate::client::requests::read_bits::Promise::new(callback);
        self.send(crate::client::channel::wrap(
            self.param,
            wrap_req(ReadBits::new(range, promise)),
        ))
    }

    fn read_registers<C, W>(
        &mut self,
        range: AddressRange,
        callback: C,
        wrap_req: W,
    ) -> Result<(), FfiSessionError>
    where
        C: FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static,
        W: Fn(ReadRegisters) -> RequestDetails,
    {
        let promise = crate::client::requests::read_registers::Promise::new(callback);
        let range = range.of_read_registers()?;
        self.send(crate::client::channel::wrap(
            self.param,
            wrap_req(ReadRegisters::new(range, promise)),
        ))
    }

    fn send(&mut self, command: Command) -> Result<(), FfiSessionError> {
        // dropping the command will automatically fail requests with SHUTDOWN
        self.tx.try_send(command)?;
        Ok(())
    }
}

impl From<InvalidRange> for FfiSessionError {
    fn from(err: InvalidRange) -> FfiSessionError {
        Self::BadRange(err)
    }
}

impl<T> From<TrySendError<T>> for FfiSessionError {
    fn from(err: TrySendError<T>) -> FfiSessionError {
        match err {
            TrySendError::Full(_) => FfiSessionError::ChannelFull,
            TrySendError::Closed(_) => FfiSessionError::ChannelClosed,
        }
    }
}
