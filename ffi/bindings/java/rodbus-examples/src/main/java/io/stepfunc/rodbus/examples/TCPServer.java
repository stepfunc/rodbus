package io.stepfunc.rodbus.examples;

import io.stepfunc.rodbus.Exception;
import io.stepfunc.rodbus.Runtime;
import io.stepfunc.rodbus.*;
import org.joou.UShort;

import java.util.Collection;

import static org.joou.Unsigned.ubyte;
import static org.joou.Unsigned.ushort;

public class TCPServer {

    static class ConsoleLogger implements LogHandler {
        @Override
        public void onMessage(LogLevel level, String message) {
            System.out.println(String.format("%s - %s", level, message));
        }
    }

    static class SimpleWriteHandler implements WriteHandler {
        @Override
        public WriteResult writeSingleCoil(Boolean value, UShort index, Database database) {
            if (database.updateCoil(index, value)) {
                return WriteResult.createSuccess();
            } else {
                return WriteResult.createException(Exception.ILLEGAL_DATA_ADDRESS);
            }
        }

        @Override
        public WriteResult writeSingleRegister(UShort value, UShort index, Database database) {
            if (database.updateHoldingRegister(index, value)) {
                return WriteResult.createSuccess();
            } else {
                return WriteResult.createException(Exception.ILLEGAL_DATA_ADDRESS);
            }
        }

        @Override
        public WriteResult writeMultipleCoils(UShort start, Collection<Bit> it, Database database) {
            for (Bit bit : it) {
                if (!database.updateCoil(bit.index, bit.value)) {
                    return WriteResult.createException(Exception.ILLEGAL_DATA_ADDRESS);
                }
            }
            return WriteResult.createSuccess();
        }

        @Override
        public WriteResult writeMultipleRegisters(UShort start, Collection<Register> it, Database database) {
            for (Register reg : it) {
                if (!database.updateHoldingRegister(reg.index, reg.value)) {
                    return WriteResult.createException(Exception.ILLEGAL_DATA_ADDRESS);
                }
            }
            return WriteResult.createSuccess();
        }
    }

    static Runtime createRuntime() {
        RuntimeConfig runtimeConfig = new RuntimeConfig();
        runtimeConfig.numCoreThreads = ushort(1);
        return new Runtime(runtimeConfig);
    }

    static DeviceMap buildDeviceMap() {
        DeviceMap map = new DeviceMap();
        map.addEndpoint(ubyte(1), new SimpleWriteHandler(), (Database db) -> {
            for (int i = 0; i < 10; ++i) {
                db.addCoil(ushort(i), false);
                db.addDiscreteInput(ushort(i), false);
                db.addHoldingRegister(ushort(i), ushort(0));
                db.addInputRegister(ushort(i), ushort(0));
            }
        });
        return map;
    }

    static class State {
        int registerValue = 0;
        boolean bitValue = false;

        void next() {
            this.registerValue += 1;
            if (this.registerValue > UShort.MAX_VALUE) {
                this.registerValue = 0;
            }
            this.bitValue = !this.bitValue;
        }

        UShort getRegisterValue() {
            return ushort(this.registerValue);
        }

        boolean getBitValue() {
            return bitValue;
        }
    }

    static void run(Runtime runtime) throws InterruptedException {
        final Server server = Server.createTcpServer(runtime, "127.0.0.1:502", buildDeviceMap());

        final State state = new State();

        while (true) {
            server.update(ubyte(1), (Database db) -> {
                state.next();
                for (int i = 0; i < 10; ++i) {
                    db.updateInputRegister(ushort(i), state.getRegisterValue());
                    db.updateDiscreteInput(ushort(i), state.getBitValue());
                }
            });
            Thread.sleep(1000);
        }
    }

    public static void main(String[] args) throws InterruptedException {
        Logging.setMaxLogLevel(LogLevel.INFO);
        Logging.setHandler(new ConsoleLogger());

        try (final Runtime runtime = createRuntime()) {
            run(runtime);
        }
    }
}
