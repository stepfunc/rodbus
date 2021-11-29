package io.stepfunc.rodbus.examples;

import static org.joou.Unsigned.*;

import io.stepfunc.rodbus.*;
import io.stepfunc.rodbus.Runtime;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.util.Arrays;

public class ClientTlsExample {
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

        // ANCHOR: tls_self_signed_config
        TlsClientConfig selfSignedTlsConfig =
            new TlsClientConfig(
                "test.com",
                "./certs/self_signed/entity2_cert.pem",
                "./certs/self_signed/entity1_cert.pem",
                "./certs/self_signed/entity1_key.pem",
                "" // no password
        );
        selfSignedTlsConfig.certificateMode = CertificateMode.SELF_SIGNED;
        // ANCHOR_END: tls_self_signed_config

        // ANCHOR: tls_ca_chain_config
        TlsClientConfig caChainTlsConfig =
            new TlsClientConfig(
                "test.com",
                "./certs/ca_chain/ca_cert.pem",
                "./certs/ca_chain/entity1_cert.pem",
                "./certs/ca_chain/entity1_key.pem",
                "" // no password
        );
        // ANCHOR_END: tls_ca_chain_config

        TlsClientConfig tlsConfig = caChainTlsConfig;

        // initialize a Modbus TLS client channel
        // ANCHOR: create_tls_channel
        DecodeLevel decodeLevel = new DecodeLevel();
        Channel channel = Channel.createTlsClient(runtime, "127.0.0.1:802", ushort(100), new RetryStrategy(), tlsConfig, decodeLevel);
        // ANCHOR_END: create_tls_channel

        try {
            run(channel);
        } finally {
            // ANCHOR: runtime_shutdown
            runtime.shutdown();
            // ANCHOR_END: runtime_shutdown
        }
    }

    private static void run(Channel channel) throws Exception {
        // ANCHOR: request_param
        final RequestParam param = new RequestParam(ubyte(1), uint(1000));
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
                    channel.readCoils(param, range).thenAccept(ClientTlsExample::handleBitResult);
                    // ANCHOR_END: read_coils
                    break;
                }
                case "rdi": {
                    channel.readDiscreteInputs(param, range).thenAccept(ClientTlsExample::handleBitResult);
                    break;
                }
                case "rhr": {
                    channel.readHoldingRegisters(param, range).thenAccept(ClientTlsExample::handleRegisterResult);
                    break;
                }
                case "rir": {
                    channel.readInputRegisters(param, range).thenAccept(ClientTlsExample::handleRegisterResult);
                    break;
                }
                case "wsc": {
                    /// ANCHOR: write_single_coil
                    channel.writeSingleCoil(param, new Bit(ushort(0), true)).thenAccept(ClientTlsExample::handleWriteResult);
                    /// ANCHOR_END: write_single_coil
                    break;
                }
                case "wsr": {
                    channel.writeSingleRegister(param, new Register(ushort(0), ushort(76))).thenAccept(ClientTlsExample::handleWriteResult);
                    break;
                }
                case "wmc": {
                    channel.writeMultipleCoils(param, ushort(0), Arrays.asList(true, false)).thenAccept(ClientTlsExample::handleWriteResult);
                    break;
                }
                case "wmr": {
                    // ANCHOR: write_multiple_registers
                    channel.writeMultipleRegisters(param, ushort(0), Arrays.asList(ushort(0xCA), ushort(0xFE))).thenAccept(ClientTlsExample::handleWriteResult);
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
