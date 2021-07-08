package io.stepfunc.rodbus.examples;

import static org.joou.Unsigned.*;

import io.stepfunc.rodbus.*;
import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.util.List;

import io.stepfunc.rodbus.Runtime;
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
            return WriteResult.createSuccess();
        } else {
            return WriteResult.createException(ModbusException.ILLEGAL_DATA_ADDRESS);
        }
    }

    @Override
    public WriteResult writeSingleRegister(UShort index, UShort value, Database database) {
        if(database.updateHoldingRegister(index, value)) {
            return WriteResult.createSuccess();
        } else {
            return WriteResult.createException(ModbusException.ILLEGAL_DATA_ADDRESS);
        }
    }

    @Override
    public WriteResult writeMultipleCoils(UShort start, List<Bit> it, Database database) {
        WriteResult result = WriteResult.createSuccess();

        for(Bit bit : it) {
            if(!database.updateCoil(bit.index, bit.value)) {
                result = WriteResult.createException(ModbusException.ILLEGAL_DATA_ADDRESS);
            }
        }

        return result;
    }

    @Override
    public WriteResult writeMultipleRegisters(UShort start, List<Register> it, Database database) {
        WriteResult result = WriteResult.createSuccess();

        for(Register reg : it) {
            if(!database.updateHoldingRegister(reg.index, reg.value)) {
                result = WriteResult.createException(ModbusException.ILLEGAL_DATA_ADDRESS);
            }
        }

        return result;
    }
}
// ANCHOR_END: write_handler

public class ServerExample {
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

        // ANCHOR: tcp_server_create
        DecodeLevel decodeLevel = new DecodeLevel();
        Server server = Server.createTcpServer(runtime, "127.0.0.1:502", ushort(10), map, decodeLevel);
        // ANCHOR_END: tcp_server_create

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
                    coilValue = !coilValue;
                    final boolean pointValue = coilValue;
                    // ANCHOR: update_coil
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
