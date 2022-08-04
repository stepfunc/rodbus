package io.stepfunc.rodbus.examples;

import static org.joou.Unsigned.*;

import io.stepfunc.rodbus.*;
import io.stepfunc.rodbus.Runtime;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.time.Duration;
import java.util.Arrays;
import java.util.List;

// ANCHOR: logging_interface
class ConsoleLogger implements Logger {
    @Override
    public void onMessage(LogLevel level, String message) {
        System.out.print(message);
    }
}
// ANCHOR_END: logging_interface

public class ClientExample {
    public static void main(String[] args) throws Exception {
        // ANCHOR: logging_init
        // initialize logging with the default configuration
        Logging.configure(new LoggingConfig(), new ConsoleLogger());
        // ANCHOR_END: logging_init

        // initialize the runtime
        // ANCHOR: runtime_init
        RuntimeConfig runtimeConfig = new RuntimeConfig().withNumCoreThreads(ushort(4));
        Runtime runtime = new Runtime(runtimeConfig);
        // ANCHOR_END: runtime_init

        if (args.length != 1)
        {
            System.out.println("you must specify a transport type");
            System.out.println("usage: client_example <channel> (tcp, rtu, tls-ca, tls-self-signed)");
            System.exit(-1);
        }

        // initialize a Modbus client channel
        ClientChannel channel = createChannel(args[0], runtime);

        try {
            run(channel);
        } finally {
            // ANCHOR: runtime_shutdown
            runtime.shutdown();
            // ANCHOR_END: runtime_shutdown
        }
    }

    private static ClientChannel createChannel(String type, Runtime runtime) {
        switch (type)
        {
            case "tcp": return createTcpChannel(runtime);
            case "rtu": return createRtuChannel(runtime);
            case "tls-ca": return createTlsChannel(runtime, getCaTlsConfig());
            case "tls-self-signed": return createTlsChannel(runtime, getSelfSignedTlsConfig());
            default:
                System.out.println("unknown channel type: " + type);
                System.exit(-1);
                return null;
        }
    }

    private static ClientChannel createTcpChannel(Runtime runtime) {
        // ANCHOR: create_tcp_channel
        ClientChannel channel = ClientChannel.createTcp(runtime, "127.0.0.1", ushort(502), ushort(100), new RetryStrategy(), DecodeLevel.nothing());
        // ANCHOR_END: create_tcp_channel

        return channel;
    }

    private static ClientChannel createRtuChannel(Runtime runtime) {
        // ANCHOR: create_rtu_channel
        ClientChannel channel = ClientChannel.createRtu(
                runtime,
                "/dev/ttySIM0", // path
                new SerialPortSettings(), // serial settings
                ushort(1), // max queued requests
                Duration.ofSeconds(1), // retry delay
                DecodeLevel.nothing() // decode level
        );
        // ANCHOR_END: create_rtu_channel

        return channel;
    }

    private static ClientChannel createTlsChannel(Runtime runtime, TlsClientConfig tlsConfig) {
        // ANCHOR: create_tls_channel
        ClientChannel channel = ClientChannel.createTls(runtime, "127.0.0.1", ushort(802), ushort(100), new RetryStrategy(), tlsConfig, DecodeLevel.nothing());
        // ANCHOR_END: create_tls_channel

        return channel;
    }

    private static TlsClientConfig getCaTlsConfig() {
        // ANCHOR: tls_ca_chain_config
        TlsClientConfig tlsConfig = new TlsClientConfig(
                "test.com",
                "./certs/ca_chain/ca_cert.pem",
                "./certs/ca_chain/client_cert.pem",
                "./certs/ca_chain/client_key.pem",
                "" // no password
        );
        // ANCHOR_END: tls_ca_chain_config

        return tlsConfig;
    }

    private static TlsClientConfig getSelfSignedTlsConfig() {
        // ANCHOR: tls_self_signed_config
        TlsClientConfig tlsConfig = new TlsClientConfig(
                "test.com",
                "./certs/self_signed/entity2_cert.pem",
                "./certs/self_signed/entity1_cert.pem",
                "./certs/self_signed/entity1_key.pem",
                "" // no password
        ).withCertificateMode(CertificateMode.SELF_SIGNED);
        // ANCHOR_END: tls_self_signed_config

        return tlsConfig;
    }

    private static void run(ClientChannel channel) throws Exception {
        // ANCHOR: enable_channel
        channel.enable();
        // ANCHOR_END: enable_channel

        // ANCHOR: request_param
        final RequestParam param = new RequestParam(ubyte(1), Duration.ofSeconds(1));
        // ANCHOR_END: request_param
        // ANCHOR: address_range
        final AddressRange range = new AddressRange(ushort(0), ushort(5));
        // ANCHOR_END: address_range

        // Handle user input
        BufferedReader reader = new BufferedReader(new InputStreamReader(System.in));
        while (true) {
            String line = reader.readLine();
            switch (line) {
                case "x":
                    return;
                case "ec": {
                    // enable channel
                    channel.enable();
                    break;
                }
                case "dc": {
                    // disable channel
                    channel.disable();
                    break;
                }
                case "ed": {
                    // enable decoding
                    channel.setDecodeLevel(new DecodeLevel(AppDecodeLevel.DATA_VALUES, FrameDecodeLevel.HEADER, PhysDecodeLevel.LENGTH));
                    break;
                }
                case "dd": {
                    // disable decoding
                    channel.setDecodeLevel(DecodeLevel.nothing());
                    break;
                }
                case "rc": {
                    // ANCHOR: read_coils
                    channel.readCoils(param, range).whenComplete(ClientExample::handleBitResult);
                    // ANCHOR_END: read_coils
                    break;
                }
                case "rdi": {
                    channel.readDiscreteInputs(param, range).whenComplete(ClientExample::handleBitResult);
                    break;
                }
                case "rhr": {
                    channel.readHoldingRegisters(param, range).whenComplete(ClientExample::handleRegisterResult);
                    break;
                }
                case "rir": {
                    channel.readInputRegisters(param, range).whenComplete(ClientExample::handleRegisterResult);
                    break;
                }
                case "wsc": {
                    /// ANCHOR: write_single_coil
                    channel.writeSingleCoil(param, new BitValue(ushort(0), true)).whenComplete(ClientExample::handleWriteResult);
                    /// ANCHOR_END: write_single_coil
                    break;
                }
                case "wsr": {
                    channel.writeSingleRegister(param, new RegisterValue(ushort(0), ushort(76))).whenComplete(ClientExample::handleWriteResult);
                    break;
                }
                case "wmc": {
                    channel.writeMultipleCoils(param, ushort(0), Arrays.asList(true, false)).whenComplete(ClientExample::handleWriteResult);
                    break;
                }
                case "wmr": {
                    // ANCHOR: write_multiple_registers
                    channel.writeMultipleRegisters(param, ushort(0), Arrays.asList(ushort(0xCA), ushort(0xFE))).whenComplete(ClientExample::handleWriteResult);
                    // ANCHOR_END: write_multiple_registers
                    break;
                }
                default:
                    System.out.println("Unknown command");
                    break;
            }
        }
    }

    // ANCHOR: handle_bit_result
    private static void handleBitResult(List<BitValue> bits, Throwable ex) {
        if (ex == null) {
            System.out.println("success!");
            for(BitValue bit : bits) {
                System.out.println("index: " + bit.index + " value: " + bit.value);
            }
        } else {
            System.out.println("error: " + ex.getMessage());
        }
    }
    // ANCHOR_END: handle_bit_result

    private static void handleRegisterResult(List<RegisterValue> registers, Throwable ex) {
        if (ex == null) {
            System.out.println("success!");
            for(RegisterValue register : registers) {
                System.out.println("index: " + register.index + " value: " + register.value);
            }
        } else {
            System.out.println("error: " + ex.getMessage());
        }
    }

    // ANCHOR: handle_write_result
    private static void handleWriteResult(Nothing nothing, Throwable ex) {
        if (ex == null) {
            System.out.println("success!");
        } else {
            System.out.println("error: " + ex.getMessage());
        }
    }
    /// ANCHOR_END: handle_write_result
}
