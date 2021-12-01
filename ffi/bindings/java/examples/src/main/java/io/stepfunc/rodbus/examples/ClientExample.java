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
        RuntimeConfig runtimeConfig = new RuntimeConfig();
        runtimeConfig.numCoreThreads = ushort(4);
        Runtime runtime = new Runtime(runtimeConfig);
        // ANCHOR_END: runtime_init

        // initialize a Modbus TCP client channel
        // ANCHOR: create_tcp_channel
        DecodeLevel decodeLevel = new DecodeLevel();
        ClientChannel channel = ClientChannel.createTcp(runtime, "127.0.0.1:502", ushort(100), new RetryStrategy(), decodeLevel);
        // ANCHOR_END: create_tcp_channel

        try {
            run(channel);
        } finally {
            // ANCHOR: runtime_shutdown
            runtime.shutdown();
            // ANCHOR_END: runtime_shutdown
        }
    }

    private static void run(ClientChannel channel) throws Exception {
        // Handle user input
        BufferedReader reader = new BufferedReader(new InputStreamReader(System.in));
        while (true) {
            String line = reader.readLine();

            if (line.equals("x")) return;

            try {
                runOneCommand(channel, line);
            } catch(Exception ex) {
                System.out.println("error: " + ex.getMessage());
            }
        }
    }

    private static void runOneCommand(ClientChannel channel, String command) {
        // ANCHOR: request_param
        final RequestParam param = new RequestParam(ubyte(1), Duration.ofSeconds(1));
        // ANCHOR_END: request_param
        // ANCHOR: address_range
        final AddressRange range = new AddressRange(ushort(0), ushort(5));
        // ANCHOR_END: address_range

        switch (command) {
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
