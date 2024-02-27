use crate::decode::AppDecodeLevel;
use crate::error::{AduParseError, InvalidRange};

use scursor::ReadCursor;

use crate::error::RequestError;

/// Modbus unit identifier, just a type-safe wrapper around `u8`
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct UnitId {
    /// underlying raw value
    pub value: u8,
}

/// Start and count tuple used when making various requests
/// Cannot be constructed with invalid start/count
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddressRange {
    /// Starting address of the range
    pub start: u16,
    /// Count of elements in the range
    pub count: u16,
}

/// Specialized wrapper around an address
/// range only valid for ReadCoils / ReadDiscreteInputs
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ReadBitsRange {
    pub(crate) inner: AddressRange,
}

#[derive(Debug, Copy, Clone, PartialEq)]
///MODBUS Encapsulated Interface
pub enum MeiCode {
    ///Can Open General Reference (Unused)
    CanOpenGeneralReference = 0x0D,
    ///Request Device Identification
    ReadDeviceId = 0x0E,
}

impl TryFrom<u8> for MeiCode {
    type Error = AduParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x0D => Ok(MeiCode::CanOpenGeneralReference),
            0x0E => Ok(MeiCode::ReadDeviceId),
            value => Err(AduParseError::MeiCodeOutOfRange(value)),
        }
    }
}

impl std::fmt::Display for MeiCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadDeviceId => write!(f, " (MEICODE) READ DEVICE ID"),
            Self::CanOpenGeneralReference => write!(f, "(MEICODE) CAN OPEN GENERAL REFERENCE"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
///Specifies what part of the device information to access.
pub enum ReadDeviceCode {
    ///Access the basic information about the device.
    BasicStreaming = 0x01,
    ///Access regular information about the device.
    RegularStreaming = 0x02,
    ///Access extended information about the device.
    ExtendedStreaming = 0x03,
    ///Access a specific object inside the device information.
    Specific = 0x04,
}

impl TryFrom<u8> for ReadDeviceCode {
    type Error = AduParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(ReadDeviceCode::BasicStreaming),
            0x02 => Ok(ReadDeviceCode::RegularStreaming),
            0x03 => Ok(ReadDeviceCode::ExtendedStreaming),
            0x04 => Ok(ReadDeviceCode::Specific),
            value => Err(AduParseError::DeviceCodeOutOfRange(value)),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
///The conformity level is used by the server to tell the client if it can respond to the specified request.
pub enum DeviceConformityLevel {
    ///The client can access basic information about the device.
    BasicIdentificationStream = 0x01,
    ///The client can access regular information about the device.
    RegularIdentificationStream = 0x02,
    ///The client can access extended information about the device.
    ExtendedIdentificationStream = 0x03,
    ///The client can access individual elements in the basic information level.
    BasicIdentificationIndividual = 0x81,
    ///The client can access individual elements in the basic and regular information level.
    RegularIdentificationIndividual = 0x82,
    ///The client can access individual elements in the basic, regular and extended information level.
    ExtendedIdentificationIndividual = 0x83,
}

impl TryFrom<u8> for DeviceConformityLevel {
    type Error = AduParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(DeviceConformityLevel::BasicIdentificationStream),
            0x02 => Ok(DeviceConformityLevel::RegularIdentificationStream),
            0x03 => Ok(DeviceConformityLevel::ExtendedIdentificationStream),
            0x81 => Ok(DeviceConformityLevel::BasicIdentificationIndividual),
            0x82 => Ok(DeviceConformityLevel::RegularIdentificationIndividual),
            0x83 => Ok(DeviceConformityLevel::ExtendedIdentificationIndividual),
            value => Err(AduParseError::DeviceConformityLevelOutOfRange(value)),
        }
    }
}

impl std::fmt::Display for ReadDeviceCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BasicStreaming => write!(f, " (READ DEVICE ID CODE) Basic Streaming"),
            Self::RegularStreaming => write!(f, " (READ DEVICE ID CODE) Regular Streaming"),
            Self::ExtendedStreaming => write!(f, " (READ DEVICE ID CODE) Extended Streaming"),
            Self::Specific => write!(f, " (READ DEVICE ID CODE) Specific"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
///MODBUS client request for retrieving information about a Device.
pub struct ReadDeviceRequest {
    ///The MODBUS Extended interface should be 0x0E.
    pub(crate) mei_code: MeiCode,
    ///The access level requested by the user. See MODBUS Documentation or ReadDeviceIdCode for further details.
    pub(crate) dev_id: ReadDeviceCode,
    ///Start the read at the specified position, if this field is none the read will start with element 0.
    pub(crate) obj_id: Option<u8>,
}

impl ReadDeviceRequest {
    ///Create a new Read Device Info Request
    pub fn new(dev_id: ReadDeviceCode, obj_id: Option<u8>) -> Self {
        Self {
            mei_code: MeiCode::ReadDeviceId, //NOTE(Kay): According to the modbus specification this value is always 0x0E(ReadDeviceId) for this particular function code.
            dev_id,
            obj_id,
        }
    }
}

impl std::fmt::Display for ReadDeviceRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}, {}, {:?}",
            self.mei_code,
            self.dev_id,
            if let Some(value) = self.obj_id {
                value
            } else {
                0x00
            }
        )
    }
}

/// String information objects defined by the standard
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StringInfoObject {
    /// Name of the vendor of the device
    VendorName,
    /// Product code
    ProductCode,
    /// Version of the software on the device
    MajorMinorRevision,
    /// URL of the vendor's website
    VendorUrl,
    /// Name of the produce
    ProductName,
    /// Model of the product
    ModelName,
    /// User application name
    UserApplicationName,
}

/// Type that models information objects defined by the standard and private types
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InfoObject {
    /// The type is string defined by the standard
    DefinedString(StringInfoObject, String),
    /// The type is a reserved or private type
    Other(u8, Vec<u8>),
}

#[derive(Debug, PartialEq)]
///Data that is returned by a modbus server can contain ascii and in some cases other types of data. This data structure converts the raw bytes into ASCII Strings
/// if possible. In the case where the data cannot be converted into ASCII due to unknown bytes we return the raw bytes to the user.
pub enum ModbusInfoObject {
    ///Type that represents an ascii string. used for everything that comes from the basic and regular storage of the server (see modbus specification page 43 object table)
    ModbusString(u8, String),
    ///Stores a unknown piece of binary data returned by the server. cast these into ascii strings at your own discretion. used for everything that comes from the extended storage (see modbus specification page 43 object table)
    ModbusRawData(u8, Vec<u8>),
}

impl From<RawModbusInfoObject> for ModbusInfoObject {
    fn from(value: RawModbusInfoObject) -> Self {
        match value.device_code {
            ReadDeviceCode::BasicStreaming | ReadDeviceCode::RegularStreaming
                if value.raw_data.is_ascii() =>
            {
                Self::ModbusString(value.index, String::from_utf8(value.raw_data).unwrap())
            }
            ReadDeviceCode::BasicStreaming | ReadDeviceCode::RegularStreaming => {
                Self::ModbusRawData(value.index, value.raw_data)
            }
            ReadDeviceCode::ExtendedStreaming | ReadDeviceCode::Specific => {
                Self::ModbusRawData(value.index, value.raw_data)
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub struct RawModbusInfoObject {
    device_code: ReadDeviceCode,
    pub index: u8,
    pub length: u8,
    raw_data: Vec<u8>,
}

impl RawModbusInfoObject {
    #[allow(missing_docs)]
    pub fn new(device_code: ReadDeviceCode, index: u8, length: u8, data: &[u8]) -> Self {
        Self {
            device_code,
            index,
            length,
            raw_data: data.to_vec(),
        }
    }

    ///Get the raw data for serialization
    pub fn get_data(&self) -> &[u8] {
        &self.raw_data
    }

    ///Convert the raw data value into a ModbusInfoObject
    pub fn convert(self) -> ModbusInfoObject {
        self.into()
    }
}

#[derive(Debug, PartialEq)]
///DeviceInfo the response generated by the server.
pub struct DeviceInfo {
    ///This value is always 0x14 for the read device function code. For further details see MODBUS specification.
    pub mei_code: MeiCode,
    ///The requested access. For further details see ReadDeviceIdCode or the MODBUS specification.
    pub read_device_id: ReadDeviceCode,
    ///The Access level the server is willing to grant. For further details see ReadDeviceConformityLevel and the MODBUS specification.
    pub conformity_level: DeviceConformityLevel,
    ///The amount of objects the __WHOLE__ transmission will contain. If the message gets split into parts this field will still contain the whole message count even if there are only parts of it in the current transmission.
    pub number_objects: u8,
    ///If the server could not fit all the information in a single response this field will be Some and contain the index of the next read. See the MODBUS specification for more details.
    pub continue_at: Option<u8>,
    ///The actual information will be put into this vector can be empty if there was no information to read.
    pub storage: Vec<RawModbusInfoObject>,
}

impl DeviceInfo {
    ///Creates a new Device Identification Reply
    pub fn new(
        mei_code: MeiCode,
        device_id: ReadDeviceCode,
        conformity_level: DeviceConformityLevel,
        number_objects: u8,
    ) -> Self {
        Self {
            mei_code,
            read_device_id: device_id,
            conformity_level,
            number_objects,
            continue_at: None,
            storage: vec![],
        }
    }

    ///Checks if the response is complete and decorates the continue_at field accordingly
    pub fn continue_at(mut self, more_follows: u8, value: u8) -> Self {
        self.continue_at = if more_follows == 0xFF {
            Some(value)
        } else {
            None
        };

        self
    }

    /*pub(crate) fn response_message_count(&self, max_msg_size: u8) -> Option<u8> {
        const ADDITIONAL_BYTES: u8 = 0x02; //Two bytes get consumed by the object id and the length of the object itself.

        let mut max_length = max_msg_size;

        for (idx, object) in self.storage.iter().enumerate() {
            let length = object.raw_data.len();

            if max_length < ((length as u8) + ADDITIONAL_BYTES) {
                return Some(idx as u8);
            }

            max_length = max_length.saturating_sub((length as u8) + ADDITIONAL_BYTES);
        }

        None
    }*/

    ///Convert all Raw Modbus objects into ModbusObjects
    pub fn finalize_and_retrieve_objects(&self) -> Vec<ModbusInfoObject> {
        let mut result = vec![];
        for raw_object in &self.storage {
            result.push(raw_object.clone().into());
        }

        result
    }
}

impl std::fmt::Display for DeviceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let next_value = if let Some(value) = self.continue_at {
            value
        } else {
            0x00
        };
        write!(
            f,
            "DEVICE INFO  ({:?}) ({:?}) ({:?}) (More Follows: {} Position: {}) storage: {:#?}",
            self.mei_code,
            self.read_device_id,
            self.conformity_level,
            self.continue_at.is_some(),
            next_value,
            self.storage
        )
    }
}

impl ReadBitsRange {
    /// retrieve the underlying [AddressRange]
    pub(crate) fn get(self) -> AddressRange {
        self.inner
    }
}

/// Specialized wrapper around an `AddressRange`
/// only valid for ReadHoldingRegisters / ReadInputRegisters
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ReadRegistersRange {
    pub(crate) inner: AddressRange,
}

impl ReadRegistersRange {
    /// Retrieve the underlying [AddressRange]
    pub(crate) fn get(self) -> AddressRange {
        self.inner
    }
}

/// Value and its address
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Indexed<T> {
    /// Address of the value
    pub index: u16,
    /// Associated value
    pub value: T,
}

/// Zero-copy type used to iterate over a collection of bits
#[derive(Debug, Copy, Clone)]
pub struct BitIterator<'a> {
    bytes: &'a [u8],
    range: AddressRange,
    pos: u16,
}

pub(crate) struct BitIteratorDisplay<'a> {
    iterator: BitIterator<'a>,
    level: AppDecodeLevel,
}

/// Zero-copy type used to iterate over a collection of registers
#[derive(Debug, Copy, Clone)]
pub struct RegisterIterator<'a> {
    bytes: &'a [u8],
    range: AddressRange,
    pos: u16,
}

pub(crate) struct RegisterIteratorDisplay<'a> {
    iterator: RegisterIterator<'a>,
    level: AppDecodeLevel,
}

impl std::fmt::Display for UnitId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#04X}", self.value)
    }
}

impl<'a> BitIterator<'a> {
    pub(crate) fn parse_all(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<Self, RequestError> {
        let bytes = cursor.read_bytes(crate::common::bits::num_bytes_for_bits(range.count))?;
        cursor.expect_empty()?;
        Ok(Self {
            bytes,
            range,
            pos: 0,
        })
    }
}

impl<'a> BitIteratorDisplay<'a> {
    pub(crate) fn new(level: AppDecodeLevel, iterator: BitIterator<'a>) -> Self {
        Self { iterator, level }
    }
}

impl std::fmt::Display for BitIteratorDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.iterator.range)?;

        if self.level.data_values() {
            for x in self.iterator {
                write!(f, "\n{x}")?;
            }
        }

        Ok(())
    }
}

impl<'a> RegisterIterator<'a> {
    pub(crate) fn parse_all(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<Self, RequestError> {
        let bytes = cursor.read_bytes(2 * (range.count as usize))?;
        cursor.expect_empty()?;
        Ok(Self {
            bytes,
            range,
            pos: 0,
        })
    }
}

impl<'a> RegisterIteratorDisplay<'a> {
    pub(crate) fn new(level: AppDecodeLevel, iterator: RegisterIterator<'a>) -> Self {
        Self { iterator, level }
    }
}

impl std::fmt::Display for RegisterIteratorDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.iterator.range)?;

        if self.level.data_values() {
            for x in self.iterator {
                write!(f, "\n{x}")?;
            }
        }

        Ok(())
    }
}

impl<'a> Iterator for BitIterator<'a> {
    type Item = Indexed<bool>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos == self.range.count {
            return None;
        }
        let byte = self.pos / 8;
        let bit = (self.pos % 8) as u8;

        match self.bytes.get(byte as usize) {
            Some(value) => {
                let bit = (*value & (1 << bit)) != 0;
                let address = self.range.start + self.pos;
                self.pos += 1;
                Some(Indexed::new(address, bit))
            }
            None => None,
        }
    }

    /// implementing this allows collect to optimize the vector capacity
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.range.count - self.pos) as usize;
        (remaining, Some(remaining))
    }
}

impl<'a> Iterator for RegisterIterator<'a> {
    type Item = Indexed<u16>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos == self.range.count {
            return None;
        }

        let pos = 2 * (self.pos as usize);
        match self.bytes.get(pos..pos + 2) {
            Some([high, low]) => {
                let value = ((*high as u16) << 8) | *low as u16;
                let index = self.pos + self.range.start;
                self.pos += 1;
                Some(Indexed::new(index, value))
            }
            _ => None,
        }
    }

    // implementing this allows collect to optimize the vector capacity
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.range.count - self.pos) as usize;
        (remaining, Some(remaining))
    }
}

impl<T> From<(u16, T)> for Indexed<T>
where
    T: Copy,
{
    fn from(tuple: (u16, T)) -> Self {
        let (index, value) = tuple;
        Self::new(index, value)
    }
}

pub(crate) fn coil_from_u16(value: u16) -> Result<bool, AduParseError> {
    match value {
        crate::constants::coil::ON => Ok(true),
        crate::constants::coil::OFF => Ok(false),
        _ => Err(AduParseError::UnknownCoilState(value)),
    }
}

pub(crate) fn coil_to_u16(value: bool) -> u16 {
    if value {
        crate::constants::coil::ON
    } else {
        crate::constants::coil::OFF
    }
}

impl AddressRange {
    /// Create a new address range
    pub fn try_from(start: u16, count: u16) -> Result<Self, InvalidRange> {
        if count == 0 {
            return Err(InvalidRange::CountOfZero);
        }

        let max_start = std::u16::MAX - (count - 1);

        if start > max_start {
            return Err(InvalidRange::AddressOverflow(start, count));
        }

        Ok(Self { start, count })
    }

    /// Converts to std::ops::Range
    pub fn to_std_range(self) -> std::ops::Range<usize> {
        let start = self.start as usize;
        let end = start + (self.count as usize);
        start..end
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = u16> {
        AddressIterator::new(self.start, self.count)
    }

    pub(crate) fn of_read_bits(self) -> Result<ReadBitsRange, InvalidRange> {
        Ok(ReadBitsRange {
            inner: self.limited_count(crate::constants::limits::MAX_READ_COILS_COUNT)?,
        })
    }

    pub(crate) fn of_read_registers(self) -> Result<ReadRegistersRange, InvalidRange> {
        Ok(ReadRegistersRange {
            inner: self.limited_count(crate::constants::limits::MAX_READ_REGISTERS_COUNT)?,
        })
    }

    fn limited_count(self, limit: u16) -> Result<Self, InvalidRange> {
        if self.count > limit {
            return Err(InvalidRange::CountTooLargeForType(self.count, limit));
        }
        Ok(self)
    }
}

impl std::fmt::Display for AddressRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "start: {:#06X} qty: {}", self.start, self.count)
    }
}

pub(crate) struct AddressIterator {
    pub(crate) current: u16,
    pub(crate) remain: u16,
}

impl AddressIterator {
    pub(crate) fn new(current: u16, remain: u16) -> Self {
        Self { current, remain }
    }
}

impl Iterator for AddressIterator {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        match self.remain.checked_sub(1) {
            Some(x) => {
                let ret = self.current;
                self.current += 1;
                self.remain = x;
                Some(ret)
            }
            None => None,
        }
    }
}

impl<T> Indexed<T> {
    /// Create a new indexed value
    pub fn new(index: u16, value: T) -> Self {
        Indexed { index, value }
    }
}

impl std::fmt::Display for Indexed<bool> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "idx: {:#06X} value: {}", self.index, self.value as i32)
    }
}

impl std::fmt::Display for Indexed<u16> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "idx: {:#06X} value: {:#06X}", self.index, self.value)
    }
}

impl UnitId {
    /// Create a new UnitId
    pub fn new(value: u8) -> Self {
        Self { value }
    }

    /// Broadcast address (only in RTU)
    pub fn broadcast() -> Self {
        Self { value: 0x00 }
    }

    /// Returns true if the address is reserved in RTU mode
    ///
    /// Users should *not* use reserved addresses in RTU mode.
    pub fn is_rtu_reserved(&self) -> bool {
        self.value >= 248
    }
}

/// Create the default UnitId of `0xFF`
impl Default for UnitId {
    fn default() -> Self {
        Self { value: 0xFF }
    }
}

#[cfg(test)]
mod tests {
    use crate::error::*;

    use super::*;

    #[test]
    fn address_start_max_count_of_one_is_allowed() {
        AddressRange::try_from(std::u16::MAX, 1).unwrap();
    }

    #[test]
    fn address_maximum_range_is_ok() {
        AddressRange::try_from(0, 0xFFFF).unwrap();
    }

    #[test]
    fn address_count_zero_fails_validation() {
        assert_eq!(AddressRange::try_from(0, 0), Err(InvalidRange::CountOfZero));
    }

    #[test]
    fn start_max_count_of_two_overflows() {
        assert_eq!(
            AddressRange::try_from(u16::MAX, 2),
            Err(InvalidRange::AddressOverflow(u16::MAX, 2))
        );
    }

    #[test]
    fn correctly_iterates_over_low_order_bits() {
        let mut cursor = ReadCursor::new(&[0x03]);
        let iterator =
            BitIterator::parse_all(AddressRange::try_from(1, 3).unwrap(), &mut cursor).unwrap();
        assert_eq!(iterator.size_hint(), (3, Some(3)));
        let values: Vec<Indexed<bool>> = iterator.collect();
        assert_eq!(
            values,
            vec![
                Indexed::new(1, true),
                Indexed::new(2, true),
                Indexed::new(3, false)
            ]
        );
    }

    #[test]
    fn correctly_iterates_over_registers() {
        let mut cursor = ReadCursor::new(&[0xFF, 0xFF, 0x01, 0xCC]);
        let iterator =
            RegisterIterator::parse_all(AddressRange::try_from(1, 2).unwrap(), &mut cursor)
                .unwrap();

        assert_eq!(iterator.size_hint(), (2, Some(2)));
        let values: Vec<Indexed<u16>> = iterator.collect();
        assert_eq!(
            values,
            vec![Indexed::new(1, 0xFFFF), Indexed::new(2, 0x01CC)]
        );
    }

    #[test]
    fn broadcast_address() {
        assert_eq!(UnitId::broadcast(), UnitId::new(0x00));
    }

    #[test]
    fn rtu_reserved_address() {
        assert!(UnitId::new(248).is_rtu_reserved());
        assert!(UnitId::new(255).is_rtu_reserved());
        assert!(!UnitId::new(41).is_rtu_reserved());
    }
}
