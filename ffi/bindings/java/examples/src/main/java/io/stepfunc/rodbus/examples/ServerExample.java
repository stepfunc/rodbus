package io.stepfunc.rodbus.examples;

import static org.joou.Unsigned.*;

import io.stepfunc.rodbus.*;
import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.time.Duration;
import java.util.Arrays;
import java.util.List;

import io.stepfunc.rodbus.Runtime;
import org.joou.UByte;
import org.joou.UShort;

class TestLogger implements Logger {

    @Override
    public void onMessage(LogLevel level, String message) {
        System.out.print(message);
    }
}

// ANCHOR: write_handler
class ExampleWriteHandler implements WriteHandler {
    @Override
    public WriteResult writeSingleCoil(UShort index, boolean value, Database database) {
        if(database.updateCoil(index, value)) {
            return WriteResult.successInit();
        } else {
            return WriteResult.exceptionInit(ModbusException.ILLEGAL_DATA_ADDRESS);
        }
    }

    @Override
    public WriteResult writeSingleRegister(UShort index, UShort value, Database database) {
        if(database.updateHoldingRegister(index, value)) {
            return WriteResult.successInit();
        } else {
            return WriteResult.exceptionInit(ModbusException.ILLEGAL_DATA_ADDRESS);
        }
    }

    @Override
    public WriteResult writeMultipleCoils(UShort start, List<BitValue> it, Database database) {
        WriteResult result = WriteResult.successInit();

        for(BitValue bit : it) {
            if(!database.updateCoil(bit.index, bit.value)) {
                result = WriteResult.exceptionInit(ModbusException.ILLEGAL_DATA_ADDRESS);
            }
        }

        return result;
    }

    @Override
    public WriteResult writeMultipleRegisters(UShort start, List<RegisterValue> it, Database database) {
        WriteResult result = WriteResult.successInit();

        for(RegisterValue reg : it) {
            if(!database.updateHoldingRegister(reg.index, reg.value)) {
                result = WriteResult.exceptionInit(ModbusException.ILLEGAL_DATA_ADDRESS);
            }
        }

        return result;
    }
}
// ANCHOR_END: write_handler

// ANCHOR: auth_handler
class TestAuthorizationHandler implements AuthorizationHandler
{
    public Authorization readCoils(UByte unitId, AddressRange range, String role) {
        return Authorization.ALLOW;
    }

    public Authorization readDiscreteInputs(UByte unitId, AddressRange range, String role) {
        return Authorization.ALLOW;
    }

    public Authorization readHoldingRegisters(UByte unitId, AddressRange range, String role) {
        return Authorization.ALLOW;
    }

    public Authorization readInputRegisters(UByte unitId, AddressRange range, String role) {
        return Authorization.ALLOW;
    }

    public Authorization writeSingleCoil(UByte unitId, UShort idx, String role) {
        return Authorization.ALLOW;
    }

    public Authorization writeSingleRegister(UByte unitId, UShort idx, String role) {
        return Authorization.DENY;
    }

    public Authorization writeMultipleCoils(UByte unitId, AddressRange range, String role) {
        return Authorization.DENY;
    }

    public Authorization writeMultipleRegisters(UByte unitId, AddressRange range, String role) {
        return Authorization.DENY;
    }
}
// ANCHOR_END: auth_handler

public class ServerExample {
    public static void main(String[] args) throws Exception {
        // initialize logging with the default configuration
        Logging.configure(new LoggingConfig(), new ConsoleLogger());

        // initialize the runtime
        RuntimeConfig runtimeConfig = new RuntimeConfig().withNumCoreThreads(ushort(4));
        Runtime runtime = new Runtime(runtimeConfig);

        // create the device map
        // ANCHOR: device_map_init
        DeviceMap map = new DeviceMap();
        map.addEndpoint(ubyte(1), new ExampleWriteHandler(), db -> {
            for(int i = 0; i < 10; i++) {
                db.addCoil(ushort(i), false);
                db.addDiscreteInput(ushort(i), false);
                db.addHoldingRegister(ushort(i), ushort(0));
                db.addInputRegister(ushort(i), ushort(0));
            }
        });
        // ANCHOR_END: device_map_init

        if (args.length != 1)
        {
            System.out.println("you must specify a transport type");
            System.out.println("usage: server_example <channel> (tcp, rtu, tls-ca, tls-self-signed)");
            System.exit(-1);
        }

        // initialize a Modbus client channel
        Server server = createServer(args[0], runtime, map);

        try {
            run(server);
        } finally {
            runtime.shutdown();
        }
    }

    private static Server createServer(String type, Runtime runtime, DeviceMap map) {
        switch (type)
        {
            case "tcp": return createTcpServer(runtime, map);
            case "rtu": return createRtuServer(runtime, map);
            case "tls-ca": return createTlsServer(runtime, map, getCaTlsConfig());
            case "tls-self-signed": return createTlsServer(runtime, map, getSelfSignedTlsConfig());
            default:
                System.out.println("unknown server type: " + type);
                System.exit(-1);
                return null;
        }
    }

    private static Server createTcpServer(Runtime runtime, DeviceMap map) {
        // ANCHOR: tcp_server_create
        Server server = Server.createTcp(runtime, "127.0.0.1", ushort(502), ushort(100), map, DecodeLevel.nothing());
        // ANCHOR_END: tcp_server_create

        return server;
    }

    private static Server createRtuServer(Runtime runtime, DeviceMap map) {
        // ANCHOR: rtu_server_create
        Server server = Server.createRtu(runtime, "/dev/ttySIM1", new SerialPortSettings(), map, DecodeLevel.nothing());
        // ANCHOR_END: rtu_server_create

        return server;
    }

    private static Server createTlsServer(Runtime runtime, DeviceMap map, TlsServerConfig tlsConfig) {
        // ANCHOR: tls_server_create
        Server server = Server.createTlsWithAuthz(runtime, "127.0.0.1", ushort(802), ushort(10), map, tlsConfig, new TestAuthorizationHandler(), DecodeLevel.nothing());
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
        );
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
        ).withCertificateMode(CertificateMode.SELF_SIGNED);
        // ANCHOR_END: tls_self_signed_config

        return tlsConfig;
    }

    public static void run(Server server) throws Exception {
        boolean coilValue = false;
        boolean discreteInputValue = false;
        int holdingRegisterValue = 0;
        int inputRegisterValue = 0;

        // Handle user input
        final BufferedReader reader = new BufferedReader(new InputStreamReader(System.in));
        while (true) {
            final String line = reader.readLine();
            switch (line) {
                case "x":
                    return;
                case "uc":
                {
                    // ANCHOR: update_coil
                    coilValue = !coilValue;
                    final boolean pointValue = coilValue;
                    server.updateDatabase(ubyte(1), db -> {
                        for(int i = 0; i < 10; i++) {
                            db.updateCoil(ushort(i), pointValue);
                        }
                    });
                    // ANCHOR_END: update_coil
                    break;
                }
                case "udi":
                {
                    discreteInputValue = !discreteInputValue;
                    final boolean pointValue = discreteInputValue;
                    server.updateDatabase(ubyte(1), db -> {
                        for(int i = 0; i < 10; i++) {
                            db.updateDiscreteInput(ushort(i), pointValue);
                        }
                    });
                    break;
                }
                case "uhr":
                {
                    holdingRegisterValue += 1;
                    final UShort pointValue = ushort(holdingRegisterValue);
                    server.updateDatabase(ubyte(1), db -> {
                        for(int i = 0; i < 10; i++) {
                            db.updateHoldingRegister(ushort(i), pointValue);
                        }
                    });
                    break;
                }
                case "uir":
                {
                    inputRegisterValue += 1;
                    final UShort pointValue = ushort(inputRegisterValue);
                    server.updateDatabase(ubyte(1), db -> {
                        for(int i = 0; i < 10; i++) {
                            db.updateInputRegister(ushort(i), pointValue);
                        }
                    });
                    break;
                }
                default:
                    System.out.println("Unknown command");
                    break;
            }
        }
    }
}
