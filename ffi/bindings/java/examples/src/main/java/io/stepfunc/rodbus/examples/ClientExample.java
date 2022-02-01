package io.stepfunc.rodbus.examples;

import static org.joou.Unsigned.*;

import io.stepfunc.rodbus.*;
import io.stepfunc.rodbus.Runtime;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.time.Duration;
import java.util.Arrays;

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
        RuntimeConfig runtimeConfig = new RuntimeConfig();
        runtimeConfig.numCoreThreads = ushort(4);
        Runtime runtime = new Runtime(runtimeConfig);
        // ANCHOR_END: runtime_init

        if (args.length != 1)
        {
            System.out.println("you must specify a transport type");
            System.out.println("usage: client_example <channel> (tcp, rtu, tls-ca, tls-self-signed)");
            System.exit(-1);
        }

        // initialize a Modbus client channel
        Channel channel = createChannel(args[0], runtime);

        try {
            run(channel);
        } finally {
            // ANCHOR: runtime_shutdown
            runtime.shutdown();
            // ANCHOR_END: runtime_shutdown
        }
    }

    private static Channel createChannel(String type, Runtime runtime) {
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

    private static Channel createTcpChannel(Runtime runtime) {
        // ANCHOR: create_tcp_channel
        DecodeLevel decodeLevel = new DecodeLevel();
        Channel channel = Channel.createTcpClient(runtime, "127.0.0.1:502", ushort(100), new RetryStrategy(), decodeLevel);
        // ANCHOR_END: create_tcp_channel

        return channel;
    }

    private static Channel createRtuChannel(Runtime runtime) {
        // ANCHOR: create_rtu_channel
        DecodeLevel decodeLevel = new DecodeLevel();
        Channel channel = Channel.createRtuClient(
                runtime,
                "/dev/ttySIM0", // path
                new SerialPortSettings(), // serial settings
                ushort(1), // max queued requests
                Duration.ofSeconds(1), // retry delay
                decodeLevel // decode level
        );
        // ANCHOR_END: create_rtu_channel

        return channel;
    }

    private static Channel createTlsChannel(Runtime runtime, TlsClientConfig tlsConfig) {
        // ANCHOR: create_tls_channel
        DecodeLevel decodeLevel = new DecodeLevel();
        Channel channel = Channel.createTlsClient(runtime, "127.0.0.1:802", ushort(100), new RetryStrategy(), tlsConfig, decodeLevel);
        // ANCHOR_END: create_tls_channel

        return channel;
    }

    private static TlsClientConfig getCaTlsConfig() {
        // ANCHOR: tls_ca_chain_config
        TlsClientConfig tlsConfig = new TlsClientConfig(
                "test.com",
                "./certs/ca_chain/ca_cert.pem",
                "./certs/ca_chain/entity1_cert.pem",
                "./certs/ca_chain/entity1_key.pem",
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
        );
        tlsConfig.certificateMode = CertificateMode.SELF_SIGNED;
        // ANCHOR_END: tls_self_signed_config

        return tlsConfig;
    }

    private static void run(Channel channel) throws Exception {
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
                case "rc": {
                    // ANCHOR: read_coils
                    channel.readCoils(param, range).thenAccept(ClientExample::handleBitResult);
                    // ANCHOR_END: read_coils
                    break;
                }
                case "rdi": {
                    channel.readDiscreteInputs(param, range).thenAccept(ClientExample::handleBitResult);
                    break;
                }
                case "rhr": {
                    channel.readHoldingRegisters(param, range).thenAccept(ClientExample::handleRegisterResult);
                    break;
                }
                case "rir": {
                    channel.readInputRegisters(param, range).thenAccept(ClientExample::handleRegisterResult);
                    break;
                }
                case "wsc": {
                    /// ANCHOR: write_single_coil
                    channel.writeSingleCoil(param, new Bit(ushort(0), true)).thenAccept(ClientExample::handleWriteResult);
                    /// ANCHOR_END: write_single_coil
                    break;
                }
                case "wsr": {
                    channel.writeSingleRegister(param, new Register(ushort(0), ushort(76))).thenAccept(ClientExample::handleWriteResult);
                    break;
                }
                case "wmc": {
                    channel.writeMultipleCoils(param, ushort(0), Arrays.asList(true, false)).thenAccept(ClientExample::handleWriteResult);
                    break;
                }
                case "wmr": {
                    // ANCHOR: write_multiple_registers
                    channel.writeMultipleRegisters(param, ushort(0), Arrays.asList(ushort(0xCA), ushort(0xFE))).thenAccept(ClientExample::handleWriteResult);
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
    private static void handleBitResult(BitReadResult result) {
        if (result.result.summary == Status.OK) {
            System.out.println("success!");
            for(Bit bit : result.iterator) {
                System.out.println("index: " + bit.index + " value: " + bit.value);
            }
        } else if (result.result.summary == Status.EXCEPTION) {
            System.out.println("Modbus exception: " + result.result.exception);
        } else {
            System.out.println("error: " + result.result.summary);
        }
    }
    // ANCHOR_END: handle_bit_result

    private static void handleRegisterResult(RegisterReadResult result) {
        // ANCHOR: error_handling
        if (result.result.summary == Status.OK) {
            System.out.println("success!");
            for(Register register : result.iterator) {
                System.out.println("index: " + register.index + " value: " + register.value);
            }
        } else if (result.result.summary == Status.EXCEPTION) {
            System.out.println("Modbus exception: " + result.result.exception);
        } else {
            System.out.println("error: " + result.result.summary);
        }
        // ANCHOR_END: error_handling
    }

    // ANCHOR: handle_write_result
    private static void handleWriteResult(ErrorInfo result) {
        if (result.summary == Status.OK) {
            System.out.println("success!");
        } else if (result.summary == Status.EXCEPTION) {
            System.out.println("Modbus exception: " + result.exception);
        } else {
            System.out.println("error: " + result.summary);
        }
    }
    /// ANCHOR_END: handle_write_result
}
