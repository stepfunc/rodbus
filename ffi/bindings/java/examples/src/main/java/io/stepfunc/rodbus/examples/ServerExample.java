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
import java.util.Arrays;
import java.util.List;

import org.joou.UByte;
import org.joou.UShort;

/**
 * Server example for the Rodbus library
 *
 * <p>usage: server &lt;tcp|rtu|tls-ca|tls-self-signed&gt; [endpoint|serial-port]
 *
 * <p>Run it from the repository root so the TLS certificate paths resolve.
 */
public class ServerExample {
    private static final String USAGE = "usage: server <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]";
    private static final List<String> TRANSPORTS = Arrays.asList("tcp", "rtu", "tls-ca", "tls-self-signed");
    private static final int UNIT_ID = 1;
    private static final String TCP_ENDPOINT = "127.0.0.1:502";
    private static final String TLS_ENDPOINT = "127.0.0.1:802";
    private static final String SERIAL_PORT = "/dev/ttySIM1";
    private static final int MAX_SESSIONS = 10;
    private static final int POINT_COUNT = 10;

    public static void main(String[] args) {
        // initialize logging with the default configuration
        Logging.configure(new LoggingConfig(), new ConsoleLogger());

        if (args.length < 1 || args.length > 2 || !TRANSPORTS.contains(args[0])) {
            usage();
        }
        final String transport = args[0];
        final String argument = args.length == 2 ? args[1] : null;

        RuntimeConfig runtimeConfig = new RuntimeConfig().withNumCoreThreads(ushort(4));
        Runtime runtime = new Runtime(runtimeConfig);

        Server server = null;
        try {
            final DeviceMap map = createDeviceMap();
            server = createServer(runtime, transport, argument, map);

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
                    runCommand(server, line);
                }
            }
        } catch (Exception ex) {
            System.err.println("error: " + ex.getMessage());
        } finally {
            if (server != null) {
                server.shutdown();
            }
            runtime.shutdown();
        }

        // only reached after an error, since the command loop exits by returning
        System.exit(1);
    }

    private static DeviceMap createDeviceMap() {
        // ANCHOR: device_map_init
        DeviceMap map = new DeviceMap();
        map.addEndpoint(ubyte(UNIT_ID), new ExampleWriteHandler(), db -> {
            for (int i = 0; i < POINT_COUNT; i++) {
                db.addCoil(ushort(i), false);
                db.addDiscreteInput(ushort(i), false);
                db.addHoldingRegister(ushort(i), ushort(0));
                db.addInputRegister(ushort(i), ushort(0));
            }
        });
        // ANCHOR_END: device_map_init

        return map;
    }

    private static Server createServer(Runtime runtime, String transport, String argument, DeviceMap map) {
        if (transport.equals("tcp")) {
            final String endpoint = argument != null ? argument : TCP_ENDPOINT;
            return createTcpServer(runtime, getHost(endpoint), getPort(endpoint), map);
        } else if (transport.equals("rtu")) {
            return createRtuServer(runtime, argument != null ? argument : SERIAL_PORT, map);
        } else if (transport.equals("tls-ca")) {
            final String endpoint = argument != null ? argument : TLS_ENDPOINT;
            return createTlsServer(runtime, getHost(endpoint), getPort(endpoint), map, getCaTlsConfig());
        } else {
            // tls-self-signed, the only transport left after the check in main
            final String endpoint = argument != null ? argument : TLS_ENDPOINT;
            return createTlsServer(runtime, getHost(endpoint), getPort(endpoint), map, getSelfSignedTlsConfig());
        }
    }

    private static Server createTcpServer(Runtime runtime, String host, UShort port, DeviceMap map) {
        // ANCHOR: tcp_server_create
        Server server = Server.createTcp(
                runtime,
                host,
                port,
                AddressFilter.any(),
                ushort(MAX_SESSIONS),
                map,
                new DecodeLevel(AppDecodeLevel.DATA_HEADERS, FrameDecodeLevel.NOTHING, PhysDecodeLevel.NOTHING)
        );
        // ANCHOR_END: tcp_server_create

        return server;
    }

    private static Server createRtuServer(Runtime runtime, String path, DeviceMap map) {
        // ANCHOR: rtu_server_create
        Server server = Server.createRtu(
                runtime,
                path,
                new SerialPortSettings(),
                new RetryStrategy(),
                map,
                new DecodeLevel(AppDecodeLevel.DATA_HEADERS, FrameDecodeLevel.NOTHING, PhysDecodeLevel.NOTHING)
        );
        // ANCHOR_END: rtu_server_create

        return server;
    }

    private static Server createTlsServer(Runtime runtime, String host, UShort port, DeviceMap map, TlsServerConfig tlsConfig) {
        // ANCHOR: tls_server_create
        Server server = Server.createTlsWithAuthz(
                runtime,
                host,
                port,
                AddressFilter.any(),
                ushort(MAX_SESSIONS),
                map,
                tlsConfig,
                new ExampleAuthorizationHandler(),
                new DecodeLevel(AppDecodeLevel.DATA_HEADERS, FrameDecodeLevel.NOTHING, PhysDecodeLevel.NOTHING)
        );
        // ANCHOR_END: tls_server_create

        return server;
    }

    private static TlsServerConfig getCaTlsConfig() {
        // ANCHOR: tls_ca_chain_config
        TlsServerConfig tlsConfig = new TlsServerConfig(
                "./certs/ca_chain/ca_cert.pem",
                "./certs/ca_chain/server_cert.pem",
                "./certs/ca_chain/server_key.pem",
                "" // no password
        ).withMinTlsVersion(MinTlsVersion.V12);
        // ANCHOR_END: tls_ca_chain_config

        return tlsConfig;
    }

    private static TlsServerConfig getSelfSignedTlsConfig() {
        // ANCHOR: tls_self_signed_config
        TlsServerConfig tlsConfig = new TlsServerConfig(
                "./certs/self_signed/entity1_cert.pem",
                "./certs/self_signed/entity2_cert.pem",
                "./certs/self_signed/entity2_key.pem",
                "" // no password
        ).withMinTlsVersion(MinTlsVersion.V12).withCertificateMode(CertificateMode.SELF_SIGNED);
        // ANCHOR_END: tls_self_signed_config

        return tlsConfig;
    }

    private static void printHelp() {
        System.out.println("Commands:");
        System.out.println("  help - print this list");
        System.out.println("  x - exit");
        System.out.println("  ed - enable protocol decoding");
        System.out.println("  dd - disable protocol decoding");
        System.out.println("  uc - toggle every coil");
        System.out.println("  udi - toggle every discrete input");
        System.out.println("  uhr - increment every holding register");
        System.out.println("  uir - increment every input register");
    }

    private static void runCommand(Server server, String cmd) {
        try {
            // the decode level commands only queue the change
            if (cmd.equals("ed")) {
                server.setDecodeLevel(new DecodeLevel(AppDecodeLevel.DATA_VALUES, FrameDecodeLevel.HEADER, PhysDecodeLevel.LENGTH));
                System.out.println(cmd + ": requested");
            } else if (cmd.equals("dd")) {
                server.setDecodeLevel(DecodeLevel.nothing());
                System.out.println(cmd + ": requested");
            } else if (cmd.equals("uc")) {
                // ANCHOR: update_coil
                // toggle every coil, reading the current value inside the transaction
                server.updateDatabase(ubyte(UNIT_ID), db -> {
                    for (int i = 0; i < POINT_COUNT; i++) {
                        db.updateCoil(ushort(i), !db.getCoil(ushort(i)));
                    }
                });
                // ANCHOR_END: update_coil
                System.out.println(cmd + ": success");
            } else if (cmd.equals("udi")) {
                server.updateDatabase(ubyte(UNIT_ID), db -> {
                    for (int i = 0; i < POINT_COUNT; i++) {
                        db.updateDiscreteInput(ushort(i), !db.getDiscreteInput(ushort(i)));
                    }
                });
                System.out.println(cmd + ": success");
            } else if (cmd.equals("uhr")) {
                server.updateDatabase(ubyte(UNIT_ID), db -> {
                    for (int i = 0; i < POINT_COUNT; i++) {
                        db.updateHoldingRegister(ushort(i), increment(db.getHoldingRegister(ushort(i))));
                    }
                });
                System.out.println(cmd + ": success");
            } else if (cmd.equals("uir")) {
                server.updateDatabase(ubyte(UNIT_ID), db -> {
                    for (int i = 0; i < POINT_COUNT; i++) {
                        db.updateInputRegister(ushort(i), increment(db.getInputRegister(ushort(i))));
                    }
                });
                System.out.println(cmd + ": success");
            } else {
                System.out.println(cmd + ": unknown command, type help for a list");
            }
        } catch (ParamException ex) {
            System.out.println(cmd + ": " + ex.getMessage());
        }
    }

    private static class ConsoleLogger implements Logger {
        @Override
        public void onMessage(LogLevel level, String message) {
            // the message already carries the level and ends in a newline
            System.out.print(message);
        }
    }

    // ANCHOR: write_handler
    private static class ExampleWriteHandler implements WriteHandler {
        @Override
        public WriteResult writeSingleCoil(UShort index, boolean value, Database database) {
            if (!database.updateCoil(index, value)) {
                return WriteResult.exceptionInit(ModbusException.ILLEGAL_DATA_ADDRESS);
            }
            System.out.println("write coil " + index.intValue() + ": " + value);
            return WriteResult.successInit();
        }

        @Override
        public WriteResult writeSingleRegister(UShort index, UShort value, Database database) {
            if (!database.updateHoldingRegister(index, value)) {
                return WriteResult.exceptionInit(ModbusException.ILLEGAL_DATA_ADDRESS);
            }
            System.out.println("write holding register " + index.intValue() + ": " + value.intValue());
            return WriteResult.successInit();
        }

        @Override
        public WriteResult writeMultipleCoils(UShort start, List<BitValue> values, Database database) {
            // check every coil exists before changing any
            for (BitValue value : values) {
                if (!hasCoil(database, value.index)) {
                    return WriteResult.exceptionInit(ModbusException.ILLEGAL_DATA_ADDRESS);
                }
            }
            for (BitValue value : values) {
                database.updateCoil(value.index, value.value);
                System.out.println("write coil " + value.index.intValue() + ": " + value.value);
            }
            return WriteResult.successInit();
        }

        @Override
        public WriteResult writeMultipleRegisters(UShort start, List<RegisterValue> values, Database database) {
            // check every register exists before changing any
            for (RegisterValue value : values) {
                if (!hasHoldingRegister(database, value.index)) {
                    return WriteResult.exceptionInit(ModbusException.ILLEGAL_DATA_ADDRESS);
                }
            }
            for (RegisterValue value : values) {
                database.updateHoldingRegister(value.index, value.value);
                System.out.println("write holding register " + value.index.intValue() + ": " + value.value.intValue());
            }
            return WriteResult.successInit();
        }

        private static boolean hasCoil(Database database, UShort index) {
            try {
                database.getCoil(index);
                return true;
            } catch (ParamException ex) {
                return false;
            }
        }

        private static boolean hasHoldingRegister(Database database, UShort index) {
            try {
                database.getHoldingRegister(index);
                return true;
            } catch (ParamException ex) {
                return false;
            }
        }
    }
    // ANCHOR_END: write_handler

    // ANCHOR: auth_handler
    // allows any role to read, and only the "operator" role to write
    private static class ExampleAuthorizationHandler implements AuthorizationHandler {
        @Override
        public Authorization readCoils(UByte unitId, AddressRange range, String role) {
            return Authorization.ALLOW;
        }

        @Override
        public Authorization readDiscreteInputs(UByte unitId, AddressRange range, String role) {
            return Authorization.ALLOW;
        }

        @Override
        public Authorization readHoldingRegisters(UByte unitId, AddressRange range, String role) {
            return Authorization.ALLOW;
        }

        @Override
        public Authorization readInputRegisters(UByte unitId, AddressRange range, String role) {
            return Authorization.ALLOW;
        }

        @Override
        public Authorization writeSingleCoil(UByte unitId, UShort idx, String role) {
            return authorizeWrite(unitId, role);
        }

        @Override
        public Authorization writeSingleRegister(UByte unitId, UShort idx, String role) {
            return authorizeWrite(unitId, role);
        }

        @Override
        public Authorization writeMultipleCoils(UByte unitId, AddressRange range, String role) {
            return authorizeWrite(unitId, role);
        }

        @Override
        public Authorization writeMultipleRegisters(UByte unitId, AddressRange range, String role) {
            return authorizeWrite(unitId, role);
        }

        private static Authorization authorizeWrite(UByte unitId, String role) {
            if ("operator".equals(role)) {
                return Authorization.ALLOW;
            } else {
                System.out.println("denied write to unit " + unitId.intValue() + " for role '" + role + "'");
                return Authorization.DENY;
            }
        }
    }
    // ANCHOR_END: auth_handler

    // adds 1, wrapping 65535 to 0
    private static UShort increment(UShort value) {
        return ushort((value.intValue() + 1) & 0xFFFF);
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
