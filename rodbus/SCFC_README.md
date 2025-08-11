# Send Custom Function Code (65-72 & 100-110)

This document provides a detailed overview of the implemented rodbus custom function code feature. These user-defined function codes fall within the range of 65 to 72 and 100 to 110, as specified in the MODBUS Application Protocol Specification V1.1b3 (Page 10, Section 5: Function Code Categories), and allow custom server-side execution logic.


## Introduction
The custom function codes enable the implementation of user-defined logic on a remote server device. It facilitates the transmission, reception, and processing of a custom function code with a variable-size data buffer.


## Request Structure
| Parameter           | Size          | Range / Value         |
|---------------------|---------------|-----------------------|
| Function code       | 1 Byte        | 0x41-0x48 / 0x64-0x6E |
| Byte Count (Input)  | 1 Byte        | 0x00 to 0xFF (N*)     |
| Byte Count (Output) | 1 Byte        | 0x00 to 0xFF          |
| Data                | N* x 2 Bytes  | 0x0000 to 0xFFFF      |


## Response Structure
| Parameter           | Size         | Value/Description     |
|---------------------|--------------|-----------------------|
| Function code       | 1 Byte       | 0x41-0x48 / 0x64-0x6E |
| Byte Count (Input)  | 1 Byte       | 0x00 to 0xFF (N*)     |
| Byte Count (Output) | 1 Byte       | 0x00 to 0xFF          |
| Data                | N* x 2 Bytes | 0x0000 to 0xFFFF      |


## Error Handling
| Parameter      | Size    | Description          |
|----------------|---------|----------------------|
| Function code  | 1 Byte  | Function code + 0x80 |
| Exception code | 1 Byte  | 01 or 02 or 03 or 04 |

### Error Codes:
- **01**: Illegal Function
- **02**: Illegal Data Address
- **03**: Illegal Data Value
- **04**: Server Device Failure


## Usage Example
### Request to send the custom FC 69 with a buffer of [0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE] (Byte Count = 4 -> 8 bytes):
| Request Field             | Hex | Response Field         | Hex |
|---------------------------|-----|------------------------|-----|
| Function code             | 45  | Function code          | 45  |
| Byte Count (Input)        | 04  | Byte Count (Input)     | 04  |
| Byte Count (Output)       | 04  | Byte Count (Output)    | 04  |
| Arg1 Hi                   | C0  | Arg1 Hi                | C0  |
| Arg1 Lo                   | DE  | Arg1 Lo                | DF  |
| Arg2 Hi                   | CA  | Arg2 Hi                | CA  |
| Arg2 Lo                   | FE  | Arg2 Lo                | FF  |
| Arg3 Hi                   | C0  | Arg3 Hi                | C0  |
| Arg3 Lo                   | DE  | Arg3 Lo                | DF  |
| Arg4 Hi                   | CA  | Arg4 Hi                | CA  |
| Arg4 Lo                   | FE  | Arg4 Lo                | FF  |


## Usage
Make sure that you are in the `rodbus` project directory.


### Start the custom_server example
- `cargo run --example custom_server -- tcp`
- Once it's up, run `ed` to enable decoding

Leave the terminal open and open another terminal.


### Start the custom_client example
- `cargo run --example custom_client -- tcp`
- Once it's up, run `ed` to enable decoding


### Send the Custom Function Code 69
In the terminal with the running custom_client example, run:
- `scfc <u8 Function Code> <u8 Byte Count Input> <u8 Byte Count Output> <u16 Arguments>`
- E.g. `scfc 0x45 0x02 0x02 0xC0DE 0xCAFE`
- The response would be for example: `fc: 0x45, bytes in: 2, bytes out: 2, values: [49375, 51967], hex: [0xC0DF, 0xCAFF]`


## Troubleshooting Tips
- Ensure the server and client are using the same communication method and are connected to each other.
- Check for any error codes in the response and refer to the error handling section for resolution.


## Additional Resources
- For more information on the MODBUS protocol and function codes, refer to the [MODBUS Application Protocol Specification V1.1b3](https://modbus.org/docs/Modbus_Application_Protocol_V1_1b3.pdf).
