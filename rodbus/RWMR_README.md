# 23 (0x17) Read/Write Multiple registers

This document provides an overview of the Function Code 23 (0x17) (Read & Write Multiple Registers) request as specified in the MODBUS Application Protocol.


## Description
This function code performs a combination of one read operation and one write operation in a single MODBUS transaction. The write operation is performed before the read.

The request specifies the starting address and number of holding registers to be read as well as the starting address, number of holding registers, and the data to be written. The byte count specifies the number of bytes to follow in the write data field.

The normal response contains the data from the group of registers that were read. The byte count field specifies the quantity of bytes to follow in the read data field.


## Request Structure
| Parameter              | Size         | Range / Value            |
|------------------------|--------------|--------------------------|
| Function code          | 1 Byte       | 0x17                     |
| Read Starting Address  | 2 Bytes      | 0x0000 to 0xFFFF         |
| Quantity to Read       | 2 Bytes      | 0x0001 to 0x007D         |
| Write Starting Address | 2 Bytes      | 0x0000 to 0xFFFF         |
| Quantity to Write      | 2 Bytes      | 0x0001 to 0x0079         |
| Write Byte Count       | 1 Byte       | 2 x N*                   |
| Write Registers Value  | N* x 2 Bytes |                          |
( N* = Quantity to Write )

## Response Structure
| Parameter            | Size         | Value / Description    |
|----------------------|--------------|------------------------|
| Function code        | 1 Byte       | 0x17                   |
| Byte Count           | 1 Byte       | 2 x N*                 |
| Read Registers value | N* x 2 Bytes |                        |
( N* = Quantity to Read )

## Error Handling
| Parameter      | Size    | Description                       |
|----------------|---------|-----------------------------------|
| Error code     | 1 Byte  | Function code + 0x80 = 0x97 (151) |
| Exception code | 1 Byte  | 01 or 02 or 03 or 04              |

### Error Codes:
- **01**: Illegal Function
- **02**: Illegal Data Address
- **03**: Illegal Data Value
- **04**: Server Device Failure


## Example
Here is an example of a request to read six registers starting at register 4, and to write three
registers starting at register 15:

| Request Field                  | Hex | Response Field             | Hex |
|--------------------------------|-----|----------------------------|-----|
| Function                       | 17  | Function                   | 17  |
| Read Starting Address Hi       | 00  | Byte Count                 | 0C  |
| Read Starting Address Lo       | 03  | Read Registers value Hi    | 00  |
| Quantity to Read Hi            | 00  | Read Registers value Lo    | FE  |
| Quantity to Read Lo            | 06  | Read Registers value Hi    | 0A  |
| Write Starting Address Hi      | 00  | Read Registers value Lo    | CD  |
| Write Starting Address Lo      | 0E  | Read Registers value Hi    | 00  |
| Quantity to Write Hi           | 00  | Read Registers value Lo    | 01  |
| Quantity to Write Lo           | 03  | Read Registers value Hi    | 00  |
| Write Byte Count               | 06  | Read Registers value Lo    | 03  |
| Write Registers Value Hi (1st) | 00  | Read Registers value Hi    | 00  |
| Write Registers Value Lo (1st) | FF  | Read Registers value Lo    | 0D  |
| Write Registers Value Hi (2nd) | 00  | Read Registers value Hi    | 00  |
| Write Registers Value Lo (2nd) | FF  | Read Registers value Lo    | FF  |
| Write Registers Value Hi (3rd) | 00  |                            |     |
| Write Registers Value Lo (3rd) | FF  |                            |     |


## Troubleshooting Tips
- Ensure the server and client are using the same communication method and are connected to each other
- Check for any returned error code in the response and refer to the error handling section for resolution


## Additional Resources
- For more information on the MODBUS protocol and function codes, refer to the [MODBUS Application Protocol Specification V1.1b3](https://modbus.org/docs/Modbus_Application_Protocol_V1_1b3.pdf), Page 38 (Read/Write Multiple registers).
