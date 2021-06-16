package io.stepfunc.rodbus.examples;

import static org.joou.Unsigned.*;

import io.stepfunc.rodbus.*;
import io.stepfunc.rodbus.Runtime;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.util.Arrays;

class ConsoleLogger implements Logger {
    @Override
    public void onMessage(LogLevel level, String message) {
        System.out.print(message);
    }
}

public class ClientExample {
    public static void main(String[] args) throws Exception {
        // initialize logging with the default configuration
        Logging.configure(new LoggingConfig(), new ConsoleLogger());

        // initialize the runtime
        RuntimeConfig runtimeConfig = new RuntimeConfig();
        runtimeConfig.numCoreThreads = ushort(4);
        Runtime runtime = new Runtime(runtimeConfig);

        // initialize a Modbus TCP client channel
        DecodeLevel decodeLevel = new DecodeLevel();
        Channel channel = Channel.createTcpClient(runtime, "127.0.0.1:502", ushort(100), decodeLevel);

        try {
            run(channel);
        } finally {
            runtime.shutdown();
        }
    }

    private static void run(Channel channel) throws Exception {
        final RequestParam param = new RequestParam(ubyte(1), uint(1000));
        final AddressRange range = new AddressRange(ushort(0), ushort(5));

        // Handle user input
        BufferedReader reader = new BufferedReader(new InputStreamReader(System.in));
        while (true) {
            String line = reader.readLine();
            switch (line) {
                case "x":
                    return;
                case "rc": {
                    channel.readCoils(range, param).thenAccept(ClientExample::handleBitResult);
                    break;
                }
                case "rdi": {
                    channel.readDiscreteInputs(range, param).thenAccept(ClientExample::handleBitResult);
                    break;
                }
                case "rhr": {
                    channel.readHoldingRegisters(range, param).thenAccept(ClientExample::handleRegisterResult);
                    break;
                }
                case "rir": {
                    channel.readInputRegisters(range, param).thenAccept(ClientExample::handleRegisterResult);
                    break;
                }
                case "wsc": {
                    channel.writeSingleCoil(new Bit(ushort(0), true), param).thenAccept(ClientExample::handleWriteResult);
                    break;
                }
                case "wsr": {
                    channel.writeSingleRegister(new Register(ushort(0), ushort(76)), param).thenAccept(ClientExample::handleWriteResult);
                    break;
                }
                case "wmc": {
                    channel.writeMultipleCoils(ushort(0), Arrays.asList(true, false), param).thenAccept(ClientExample::handleWriteResult);
                    break;
                }
                case "wmr": {
                    channel.writeMultipleRegisters(ushort(0), Arrays.asList(ushort(0xCA), ushort(0xFE)), param).thenAccept(ClientExample::handleWriteResult);
                    break;
                }
                default:
                    System.out.println("Unknown command");
                    break;
            }
        }
    }

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

    private static void handleRegisterResult(RegisterReadResult result) {
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
    }

    private static void handleWriteResult(ErrorInfo result) {
        if (result.summary == Status.OK) {
            System.out.println("success!");
        } else if (result.summary == Status.EXCEPTION) {
            System.out.println("Modbus exception: " + result.exception);
        } else {
            System.out.println("error: " + result.summary);
        }
    }
}
