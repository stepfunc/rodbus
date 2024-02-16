# 69 (0x45) Send Custom Function Code

This document provides a detailed overview of the custom function code (0x45) used in the MODBUS Application Protocol. This function code is user-defined and falls within the range of 65 to 72, as specified in the MODBUS Application Protocol Specification V1.1b3 (Page 10, Section 5: Function Code Categories).


## Introduction
The 0x45 function code enables the implementation of user-defined logic on a remote server device. It facilitates the transmission, reception, and processing of a custom function code with a variable-size data buffer.


## Request Structure
| Parameter          | Size          | Range / Value         |
|--------------------|---------------|-----------------------|
| Function code      | 1 Byte        | 0x45                  |
| Byte Count         | 2 Bytes       | 0x0000 to 0xFFFF (N*) |
| Data               | N* x 2 Bytes  | 0x0000 to 0xFFFF      |


## Response Structure
| Parameter     | Size         | Value/Description     |
|---------------|--------------|-----------------------|
| Function code | 1 Byte       | 0x45                  |
| Byte Count    | 2 Bytes      | 0x0000 to 0xFFFF (N*) |
| Data          | N* x 2 Bytes | 0x0000 to 0xFFFF      |


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
### Request to send the custom buffer [0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE] (Byte Count = 4 -> 8 bytes):
| Request Field             | Hex | Response Field         | Hex |
|---------------------------|-----|------------------------|-----|
| Function                  | 45  | Function               | 45  |
| Byte Count Hi             | 00  | Byte Count Hi          | 00  |
| Byte Count Lo             | 04  | Byte Count Lo          | 04  |
| Arg1 Hi                   | C0  | Arg1 Hi                | C0  |
| Arg1 Lo                   | DE  | Arg1 Lo                | DE  |
| Arg2 Hi                   | CA  | Arg2 Hi                | CA  |
| Arg2 Lo                   | FE  | Arg2 Lo                | FE  |
| Arg3 Hi                   | C0  | Arg3 Hi                | C0  |
| Arg3 Lo                   | DE  | Arg3 Lo                | DE  |
| Arg4 Hi                   | CA  | Arg4 Hi                | CA  |
| Arg4 Lo                   | FE  | Arg4 Lo                | FE  |


## Modify and Test Server-Side Buffer Handling
The server currently forwards the Custom Function Code buffer to the client again without alteration. To test modifying or processing the buffer on the remote server device, edit the `send_custom_function_code()` function in `src/examples/client.rs` and `src/examples/server.rs` as needed.

## Usage
Make sure that you are in the `rodbus` project directory.

### Start the custom_server example
- `cargo run --example custom_server -- tcp`

Leave the terminal open and open another terminal.

### Start the custom_client example
- `cargo run --example custom_client -- tcp`

### Send the Custom Function Code CFC69 request
In the terminal with the running custom_client example, run:
- `scfc <u16 Byte Count> <u16 Arguments>`
- e.g. `scfc 0x02 0xC0DE 0xCAFE`


## Troubleshooting Tips
- Ensure the server and client are using the same communication and are connected to each other.
- Check for any error codes in the response and refer to the error handling section for resolution.


## Additional Resources
- For more information on the MODBUS protocol and function codes, refer to the [MODBUS Application Protocol Specification V1.1b3](https://modbus.org/docs/Modbus_Application_Protocol_V1_1b3.pdf).
