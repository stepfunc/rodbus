package io.stepfunc.rodbus.examples;

import static org.joou.Unsigned.*;

import io.stepfunc.rodbus.*;
import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.util.List;

import io.stepfunc.rodbus.Runtime;
import org.joou.UByte;
import org.joou.UShort;

class TestAuthorizationHandler implements AuthorizationHandler
{
    public AuthorizationResult readCoils(UByte unitId, AddressRange range, String role) {
        return AuthorizationResult.AUTHORIZED;
    }

    public AuthorizationResult readDiscreteInputs(UByte unitId, AddressRange range, String role) {
        return AuthorizationResult.AUTHORIZED;
    }

    public AuthorizationResult readHoldingRegisters(UByte unitId, AddressRange range, String role) {
        return AuthorizationResult.AUTHORIZED;
    }

    public AuthorizationResult readInputRegisters(UByte unitId, AddressRange range, String role) {
        return AuthorizationResult.AUTHORIZED;
    }

    public AuthorizationResult writeSingleCoil(UByte unitId, UShort idx, String role) {
        return AuthorizationResult.NOT_AUTHORIZED;
    }

    public AuthorizationResult writeSingleRegister(UByte unitId, UShort idx, String role) {
        return AuthorizationResult.NOT_AUTHORIZED;
    }

    public AuthorizationResult writeMultipleCoils(UByte unitId, AddressRange range, String role) {
        return AuthorizationResult.NOT_AUTHORIZED;
    }

    public AuthorizationResult writeMultipleRegisters(UByte unitId, AddressRange range, String role) {
        return AuthorizationResult.NOT_AUTHORIZED;
    }
}

public class ServerTlsExample {
    public static void main(String[] args) throws Exception {
        // initialize logging with the default configuration
        Logging.configure(new LoggingConfig(), new ConsoleLogger());

        // initialize the runtime
        RuntimeConfig runtimeConfig = new RuntimeConfig();
        runtimeConfig.numCoreThreads = ushort(4);
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

        // ANCHOR: tls_self_signed_config
        TlsServerConfig selfSignedTlsConfig =
            new TlsServerConfig(
                "./certs/self_signed/entity1_cert.pem",
                "./certs/self_signed/entity2_cert.pem",
                "./certs/self_signed/entity2_key.pem");
        selfSignedTlsConfig.certificateMode = CertificateMode.SELF_SIGNED_CERTIFICATE;
        // ANCHOR_END: tls_self_signed_config

        // ANCHOR: tls_ca_chain_config
        TlsServerConfig caChainTlsConfig =
            new TlsServerConfig(
                "./certs/ca_chain/ca_cert.pem",
                "./certs/ca_chain/entity2_cert.pem",
                "./certs/ca_chain/entity2_key.pem");
        // ANCHOR_END: tls_ca_chain_config

        TlsServerConfig tlsConfig = caChainTlsConfig;

        // ANCHOR: tls_server_create
        DecodeLevel decodeLevel = new DecodeLevel();
        Server server = Server.createTlsServer(runtime, "127.0.0.1:802", ushort(10), map, tlsConfig, new TestAuthorizationHandler(), decodeLevel);
        // ANCHOR_END: tls_server_create

        try {
            run(server);
        } finally {
            runtime.shutdown();
        }
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
                    server.update(ubyte(1), db -> {
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
                    server.update(ubyte(1), db -> {
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
                    server.update(ubyte(1), db -> {
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
                    server.update(ubyte(1), db -> {
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
