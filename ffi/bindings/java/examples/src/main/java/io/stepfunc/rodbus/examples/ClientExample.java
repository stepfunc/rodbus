// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

package io.stepfunc.rodbus.examples;

import static org.joou.Unsigned.*;

import io.stepfunc.rodbus.*;
import io.stepfunc.rodbus.Runtime;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.time.Duration;
import java.util.Arrays;
import java.util.List;
import java.util.concurrent.CompletionException;
import java.util.concurrent.CompletionStage;

import org.joou.UShort;

/**
 * Client example for the Rodbus library
 *
 * <p>usage: client &lt;tcp|rtu|tls-ca|tls-self-signed&gt; [endpoint|serial-port]
 *
 * <p>Run it from the repository root so the TLS certificate paths resolve.
 */
public class ClientExample {
    private static final String USAGE = "usage: client <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]";
    private static final List<String> TRANSPORTS = Arrays.asList("tcp", "rtu", "tls-ca", "tls-self-signed");
    private static final int UNIT_ID = 1;
    private static final String TCP_ENDPOINT = "127.0.0.1:502";
    private static final String TLS_ENDPOINT = "127.0.0.1:802";
    private static final String SERIAL_PORT = "/dev/ttySIM0";
    private static final int MAX_QUEUED_REQUESTS = 10;
    private static final Duration REQUEST_TIMEOUT = Duration.ofSeconds(1);

    public static void main(String[] args) {
        // ANCHOR: logging_init
        // initialize logging with the default configuration
        Logging.configure(new LoggingConfig(), new ConsoleLogger());
        // ANCHOR_END: logging_init

        if (args.length < 1 || args.length > 2 || !TRANSPORTS.contains(args[0])) {
            usage();
        }
        final String transport = args[0];
        final String argument = args.length == 2 ? args[1] : null;

        // ANCHOR: runtime_init
        RuntimeConfig runtimeConfig = new RuntimeConfig().withNumCoreThreads(ushort(4));
        Runtime runtime = new Runtime(runtimeConfig);
        // ANCHOR_END: runtime_init

        ClientChannel channel = null;
        try {
            channel = createChannel(runtime, transport, argument);
            channel.enable();

            // ANCHOR: request_param
            final RequestParam param = new RequestParam(ubyte(UNIT_ID), REQUEST_TIMEOUT);
            // ANCHOR_END: request_param
            // ANCHOR: address_range
            final AddressRange range = new AddressRange(ushort(0), ushort(5));
            // ANCHOR_END: address_range

            printHelp();

            // exit on "x" or at the end of input
            final BufferedReader reader = new BufferedReader(new InputStreamReader(System.in));
            while (true) {
                final String line = reader.readLine();
                if (line == null || line.equals("x")) {
                    return;
                } else if (line.equals("help")) {
                    printHelp();
                } else {
                    runCommand(channel, param, range, line);
                }
            }
        } catch (Exception ex) {
            System.err.println("error: " + ex.getMessage());
        } finally {
            if (channel != null) {
                channel.shutdown();
            }
            // ANCHOR: runtime_shutdown
            runtime.shutdown();
            // ANCHOR_END: runtime_shutdown
        }

        // only reached after an error, since the command loop exits by returning
        System.exit(1);
    }

    private static ClientChannel createChannel(Runtime runtime, String transport, String argument) {
        if (transport.equals("tcp")) {
            final String endpoint = argument != null ? argument : TCP_ENDPOINT;
            return createTcpChannel(runtime, getHost(endpoint), getPort(endpoint));
        } else if (transport.equals("rtu")) {
            return createRtuChannel(runtime, argument != null ? argument : SERIAL_PORT);
        } else if (transport.equals("tls-ca")) {
            final String endpoint = argument != null ? argument : TLS_ENDPOINT;
            return createTlsChannel(runtime, getHost(endpoint), getPort(endpoint), getCaTlsConfig());
        } else {
            // tls-self-signed, the only transport left after the check in main
            final String endpoint = argument != null ? argument : TLS_ENDPOINT;
            return createTlsChannel(runtime, getHost(endpoint), getPort(endpoint), getSelfSignedTlsConfig());
        }
    }

    private static ClientChannel createTcpChannel(Runtime runtime, String host, UShort port) {
        // ANCHOR: create_tcp_channel
        ClientChannel channel = ClientChannel.createTcp(
                runtime,
                host,
                port,
                ushort(MAX_QUEUED_REQUESTS),
                new RetryStrategy(),
                new DecodeLevel(AppDecodeLevel.DATA_HEADERS, FrameDecodeLevel.NOTHING, PhysDecodeLevel.NOTHING),
                new PrintingClientStateListener()
        );
        // ANCHOR_END: create_tcp_channel

        return channel;
    }

    private static ClientChannel createRtuChannel(Runtime runtime, String path) {
        // ANCHOR: create_rtu_channel
        ClientChannel channel = ClientChannel.createRtu(
                runtime,
                path, // path
                new SerialPortSettings(), // serial settings
                ushort(MAX_QUEUED_REQUESTS), // max queued requests
                new RetryStrategy(), // retry delays
                new DecodeLevel(AppDecodeLevel.DATA_HEADERS, FrameDecodeLevel.NOTHING, PhysDecodeLevel.NOTHING),
                new PrintingPortStateListener()
        );
        // ANCHOR_END: create_rtu_channel

        return channel;
    }

    private static ClientChannel createTlsChannel(Runtime runtime, String host, UShort port, TlsClientConfig tlsConfig) {
        // ANCHOR: create_tls_channel
        ClientChannel channel = ClientChannel.createTls(
                runtime,
                host,
                port,
                ushort(MAX_QUEUED_REQUESTS),
                new RetryStrategy(),
                tlsConfig,
                new DecodeLevel(AppDecodeLevel.DATA_HEADERS, FrameDecodeLevel.NOTHING, PhysDecodeLevel.NOTHING),
                new PrintingClientStateListener()
        );
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
        ).withMinTlsVersion(MinTlsVersion.V12);
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
        ).withMinTlsVersion(MinTlsVersion.V12).withCertificateMode(CertificateMode.SELF_SIGNED);
        // ANCHOR_END: tls_self_signed_config

        return tlsConfig;
    }

    private static void printHelp() {
        System.out.println("Commands:");
        System.out.println("  help - print this list");
        System.out.println("  x - exit");
        System.out.println("  ec - enable the channel");
        System.out.println("  dc - disable the channel");
        System.out.println("  ed - enable protocol decoding");
        System.out.println("  dd - disable protocol decoding");
        System.out.println("  rc - read coils 0 to 4");
        System.out.println("  rdi - read discrete inputs 0 to 4");
        System.out.println("  rhr - read holding registers 0 to 4");
        System.out.println("  rir - read input registers 0 to 4");
        System.out.println("  wsc - write coil 0 = true");
        System.out.println("  wsr - write holding register 0 = 76");
        System.out.println("  wmc - write coils 0 and 1 = true, false");
        System.out.println("  wmr - write holding registers 0 and 1 = 0xCA, 0xFE");
    }

    private static void runCommand(ClientChannel channel, RequestParam param, AddressRange range, String cmd) {
        try {
            // the settings commands only queue the change; the listener shows the channel's state
            if (cmd.equals("ec")) {
                channel.enable();
                System.out.println(cmd + ": requested");
            } else if (cmd.equals("dc")) {
                channel.disable();
                System.out.println(cmd + ": requested");
            } else if (cmd.equals("ed")) {
                channel.setDecodeLevel(new DecodeLevel(AppDecodeLevel.DATA_VALUES, FrameDecodeLevel.HEADER, PhysDecodeLevel.LENGTH));
                System.out.println(cmd + ": requested");
            } else if (cmd.equals("dd")) {
                channel.setDecodeLevel(DecodeLevel.nothing());
                System.out.println(cmd + ": requested");
            } else if (cmd.equals("rc")) {
                // ANCHOR: read_coils
                handleBitResult(cmd, "coil", channel.readCoils(param, range));
                // ANCHOR_END: read_coils
            } else if (cmd.equals("rdi")) {
                handleBitResult(cmd, "discrete input", channel.readDiscreteInputs(param, range));
            } else if (cmd.equals("rhr")) {
                handleRegisterResult(cmd, "holding register", channel.readHoldingRegisters(param, range));
            } else if (cmd.equals("rir")) {
                handleRegisterResult(cmd, "input register", channel.readInputRegisters(param, range));
            } else if (cmd.equals("wsc")) {
                // ANCHOR: write_single_coil
                handleWriteResult(cmd, channel.writeSingleCoil(param, new BitValue(ushort(0), true)));
                // ANCHOR_END: write_single_coil
            } else if (cmd.equals("wsr")) {
                handleWriteResult(cmd, channel.writeSingleRegister(param, new RegisterValue(ushort(0), ushort(76))));
            } else if (cmd.equals("wmc")) {
                handleWriteResult(cmd, channel.writeMultipleCoils(param, ushort(0), Arrays.asList(true, false)));
            } else if (cmd.equals("wmr")) {
                // ANCHOR: write_multiple_registers
                handleWriteResult(cmd, channel.writeMultipleRegisters(param, ushort(0), Arrays.asList(ushort(0xCA), ushort(0xFE))));
                // ANCHOR_END: write_multiple_registers
            } else {
                System.out.println(cmd + ": unknown command, type help for a list");
            }
        } catch (ParamException ex) {
            System.out.println(cmd + ": " + ex.getMessage());
        }
    }

    // ANCHOR: handle_bit_result
    // waits for the result and prints it
    private static void handleBitResult(String cmd, String name, CompletionStage<List<BitValue>> result) {
        try {
            final List<BitValue> values = result.toCompletableFuture().join();
            System.out.println(cmd + ": success");
            for (BitValue value : values) {
                System.out.println(name + " " + value.index.intValue() + ": " + value.value);
            }
        } catch (CompletionException ex) {
            System.out.println(cmd + ": " + ex.getCause().getMessage());
        }
    }
    // ANCHOR_END: handle_bit_result

    private static void handleRegisterResult(String cmd, String name, CompletionStage<List<RegisterValue>> result) {
        try {
            final List<RegisterValue> values = result.toCompletableFuture().join();
            System.out.println(cmd + ": success");
            for (RegisterValue value : values) {
                System.out.println(name + " " + value.index.intValue() + ": " + value.value.intValue());
            }
        } catch (CompletionException ex) {
            System.out.println(cmd + ": " + ex.getCause().getMessage());
        }
    }

    // ANCHOR: handle_write_result
    // waits for the result and prints it
    private static void handleWriteResult(String cmd, CompletionStage<Nothing> result) {
        try {
            result.toCompletableFuture().join();
            System.out.println(cmd + ": success");
        } catch (CompletionException ex) {
            System.out.println(cmd + ": " + ex.getCause().getMessage());
        }
    }
    // ANCHOR_END: handle_write_result

    // ANCHOR: logging_interface
    private static class ConsoleLogger implements Logger {
        @Override
        public void onMessage(LogLevel level, String message) {
            // the message already carries the level and ends in a newline
            System.out.print(message);
        }
    }
    // ANCHOR_END: logging_interface

    private static class PrintingClientStateListener implements ClientStateListener {
        @Override
        public void onChange(ClientState state) {
            System.out.println("client state: " + getDescription(state));
        }

        private static String getDescription(ClientState state) {
            if (state == ClientState.DISABLED) {
                return "disabled";
            } else if (state == ClientState.CONNECTING) {
                return "connecting";
            } else if (state == ClientState.CONNECTED) {
                return "connected";
            } else if (state == ClientState.WAIT_AFTER_FAILED_CONNECT) {
                return "connect failed, waiting to retry";
            } else if (state == ClientState.WAIT_AFTER_DISCONNECT) {
                return "disconnected, waiting to reconnect";
            } else {
                return "shutdown";
            }
        }
    }

    private static class PrintingPortStateListener implements PortStateListener {
        @Override
        public void onChange(PortState state) {
            System.out.println("port state: " + getDescription(state));
        }

        private static String getDescription(PortState state) {
            if (state == PortState.DISABLED) {
                return "disabled";
            } else if (state == PortState.WAIT) {
                return "open failed, waiting to retry";
            } else if (state == PortState.OPEN) {
                return "open";
            } else {
                return "shutdown";
            }
        }
    }

    private static String getHost(String endpoint) {
        return endpoint.substring(0, getPortSeparator(endpoint));
    }

    private static UShort getPort(String endpoint) {
        return ushort(Integer.parseInt(endpoint.substring(getPortSeparator(endpoint) + 1)));
    }

    // the bindings take the host and the port separately: split "host:port" at the last ':'
    private static int getPortSeparator(String endpoint) {
        final int separator = endpoint.lastIndexOf(':');
        if (separator < 0) {
            throw new IllegalArgumentException("endpoint is not host:port: " + endpoint);
        }
        return separator;
    }

    private static void usage() {
        System.err.println(USAGE);
        System.exit(-1);
    }
}
