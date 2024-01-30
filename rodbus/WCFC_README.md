# 69 (0x45) Write Custom Function Code

This document provides a detailed overview of the custom function code (0x45) used in the MODBUS Application Protocol. This function code is user-defined and falls within the range of 65 to 72, as specified in the MODBUS Application Protocol Specification V1.1b3 (Page 10, Section 5: Function Code Categories).


## Introduction
The 0x45 function code enables the implementation of user-defined logic on a remote server device. It facilitates the transmission, reception, and processing of a custom function code with a fixed-size data buffer. This buffer currently supports 4 arguments, each 2 bytes (u16) in size, allowing for the execution of custom logic remotely.

**Note:** To increase flexibility, support for a variable-length data buffer will be included in a future update.


## Request Structure
| Parameter          | Size     | Range / Value         |
|--------------------|----------|-----------------------|
| Function code      | 1 Byte   | 0x45                  |
| Length             | 2 Bytes  | 0x0004                |
| Data               | 8 Bytes  | 0x0000 to 0xFFFF      |


## Response Structure
| Parameter     | Size    | Value/Description    |
|---------------|---------|----------------------|
| Function code | 1 Byte  | 0x45                 |
| Length        | 2 Bytes | 0x0004               |
| Data          | 8 Bytes | 0x0000 to 0xFFFF     |


## Error Handling
| Parameter      | Size    | Description                       |
|----------------|---------|-----------------------------------|
| Function code  | 1 Byte  | Function code + 0x80 = 0xC5 (197) |
| Exception code | 1 Byte  | 01 or 02 or 03 or 04              |

### Error Codes:
- **01**: Illegal Function
- **02**: Illegal Data Address
- **03**: Illegal Data Value
- **04**: Server Device Failure


## Usage Example
### Request to send the custom buffer [0xC0, 0xDE, 0xCA, 0xFE]:

| Request Field             | Hex | Response Field         | Hex |
|---------------------------|-----|------------------------|-----|
| Function                  | 45  | Function               | 45  |
| Length                    | 04  | Byte Count             | 04  |
| Arg1                      | C0  | Arg1                   | C0  |
| Arg2                      | DE  | Arg2                   | DE  |
| Arg3                      | CA  | Arg3                   | CA  |
| Arg4                      | FE  | Arg4                   | FE  |


## Modify and Test Server-Side Buffer Handling
The server currently forwards the Custom Function Code buffer to the client again without alteration. To test modifying or processing the buffer on the remote server device, edit the `write_custom_function_code()` function in `src/examples/client.rs` and `src/examples/server.rs` as needed.


## Troubleshooting Tips
- Ensure the server and client are using the same communication and are connected to each other.
- Check for any error codes in the response and refer to the error handling section for resolution.


## Additional Resources
- For more information on the MODBUS protocol and function codes, refer to the [MODBUS Application Protocol Specification V1.1b3](https://modbus.org/docs/Modbus_Application_Protocol_V1_1b3.pdf).
