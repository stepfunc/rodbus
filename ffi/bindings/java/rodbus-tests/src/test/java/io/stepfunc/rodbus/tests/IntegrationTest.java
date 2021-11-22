package io.stepfunc.rodbus.tests;

import io.stepfunc.rodbus.*;
import io.stepfunc.rodbus.ModbusException;
import io.stepfunc.rodbus.Runtime;
import org.joou.*;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.parallel.Execution;

import java.time.Duration;
import java.util.Arrays;
import java.util.List;
import java.util.concurrent.ExecutionException;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.joou.Unsigned.*;

public class IntegrationTest {
    static final UByte UNIT_ID = ubyte(1);
    static final Duration TIMEOUT = Duration.ofSeconds(1);
    static final int NUM_POINTS = 10;
    static final String ENDPOINT = "127.0.0.1:20000";

    static class TestWriteHandler implements WriteHandler {
        @Override
        public WriteResult writeSingleCoil(UShort index, boolean value, Database database) {
            if (database.updateCoil(index, value)) {
                return WriteResult.successInit();
            } else {
                return WriteResult.exceptionInit(ModbusException.ILLEGAL_DATA_ADDRESS);
            }
        }

        @Override
        public WriteResult writeSingleRegister(UShort index, UShort value, Database database) {
            if (database.updateHoldingRegister(index, value)) {
                return WriteResult.successInit();
            } else {
                return WriteResult.exceptionInit(ModbusException.ILLEGAL_DATA_ADDRESS);
            }
        }

        @Override
        public WriteResult writeMultipleCoils(UShort start, List<BitValue> it, Database database) {
            for (BitValue bit : it) {
                if (!database.updateCoil(bit.index, bit.value)) {
                    return WriteResult.exceptionInit(ModbusException.ILLEGAL_DATA_ADDRESS);
                }
            }

            return WriteResult.successInit();
        }

        @Override
        public WriteResult writeMultipleRegisters(UShort start, List<RegisterValue> it, Database database) {
            for (RegisterValue register : it) {
                if (!database.updateHoldingRegister(register.index, register.value)) {
                    return WriteResult.exceptionInit(ModbusException.ILLEGAL_DATA_ADDRESS);
                }
            }

            return WriteResult.successInit();
        }
    }

    @Test
    public void clientAndServerCommunication() throws ExecutionException, InterruptedException {
        final RuntimeConfig runtimeConfig = new RuntimeConfig();
        Runtime runtime = new Runtime(runtimeConfig);

        final DeviceMap deviceMap = new DeviceMap();
        deviceMap.addEndpoint(UNIT_ID, new TestWriteHandler(), db -> {
            for(int i = 0; i < NUM_POINTS; i++) {
                db.addCoil(ushort(i), false);
                db.addDiscreteInput(ushort(i), false);
                db.addHoldingRegister(ushort(i), ushort(0));
                db.addInputRegister(ushort(i), ushort(0));
            }
        });

        final Server server = Server.tcpServerCreate(runtime, ENDPOINT, ushort(100), deviceMap, new DecodeLevel());
        final Channel client = Channel.createTcpClient(runtime, ENDPOINT, ushort(10), new RetryStrategy(), new DecodeLevel());

        // Set a unique pattern to test reads
        server.updateDatabase(UNIT_ID, db -> {
            db.updateDiscreteInput(ushort(3), true);
            db.updateInputRegister(ushort(4), ushort(42));
        });

        testReadDiscreteInputs(client);
        testReadInputRegisters(client);
        testWriteSingleCoil(client);
        testWriteSingleRegister(client);
        testWriteMultipleCoils(client);
    }

    private void testReadDiscreteInputs(Channel client) throws ExecutionException, InterruptedException {
        RequestParam param = new RequestParam(UNIT_ID, TIMEOUT);
        AddressRange range = new AddressRange(ushort(2), ushort(3));

        List<BitValue> result = client.readDiscreteInputs(param, range).toCompletableFuture().get();

        assertThat(result).hasSize(3);
        assertThat(result.get(0).index).isEqualTo(ushort(2));
        assertThat(result.get(0).value).isEqualTo(false);
        assertThat(result.get(1).index).isEqualTo(ushort(3));
        assertThat(result.get(1).value).isEqualTo(true);
        assertThat(result.get(2).index).isEqualTo(ushort(4));
        assertThat(result.get(2).value).isEqualTo(false);

        // ======

        assertThatThrownBy(() -> {
            range.start = ushort(9);
            range.count = ushort(2);
            client.readDiscreteInputs(param, range).toCompletableFuture().get();
        }).isInstanceOf(ExecutionException.class).extracting("getCause.error").isEqualTo(RequestError.MODBUS_EXCEPTION_ILLEGAL_DATA_ADDRESS);
    }

    private void testReadInputRegisters(Channel client) throws ExecutionException, InterruptedException {
        RequestParam param = new RequestParam(UNIT_ID, TIMEOUT);
        AddressRange range = new AddressRange(ushort(3), ushort(3));

        List<RegisterValue> result = client.readInputRegisters(param, range).toCompletableFuture().get();

        assertThat(result).hasSize(3);
        assertThat(result.get(0).index).isEqualTo(ushort(3));
        assertThat(result.get(0).value).isEqualTo(ushort(0));
        assertThat(result.get(1).index).isEqualTo(ushort(4));
        assertThat(result.get(1).value).isEqualTo(ushort(42));
        assertThat(result.get(2).index).isEqualTo(ushort(5));
        assertThat(result.get(2).value).isEqualTo(ushort(0));

        // ======

        assertThatThrownBy(() -> {
            range.start = ushort(10);
            range.count = ushort(1);
            client.readDiscreteInputs(param, range).toCompletableFuture().get();
        }).isInstanceOf(ExecutionException.class).extracting("getCause.error").isEqualTo(RequestError.MODBUS_EXCEPTION_ILLEGAL_DATA_ADDRESS);
    }

    private void testWriteSingleCoil(Channel client) throws ExecutionException, InterruptedException {
        RequestParam param = new RequestParam(UNIT_ID, TIMEOUT);
        BitValue bit = new BitValue(ushort(1), true);

        client.writeSingleCoil(param, bit).toCompletableFuture().get();

        // ======

        AddressRange range = new AddressRange(ushort(0), ushort(2));

        List<BitValue> readResult = client.readCoils(param, range).toCompletableFuture().get();

        assertThat(readResult).hasSize(2);
        assertThat(readResult.get(0).index).isEqualTo(ushort(0));
        assertThat(readResult.get(0).value).isEqualTo(false);
        assertThat(readResult.get(1).index).isEqualTo(ushort(1));
        assertThat(readResult.get(1).value).isEqualTo(true);
    }

    private void testWriteSingleRegister(Channel client) throws ExecutionException, InterruptedException {
        RequestParam param = new RequestParam(UNIT_ID, TIMEOUT);
        RegisterValue register = new RegisterValue(ushort(1), ushort(22));

        client.writeSingleRegister(param, register).toCompletableFuture().get();

        // ======

        AddressRange range = new AddressRange(ushort(0), ushort(2));

        List<RegisterValue> readResult = client.readHoldingRegisters(param, range).toCompletableFuture().get();

        assertThat(readResult).hasSize(2);
        assertThat(readResult.get(0).index).isEqualTo(ushort(0));
        assertThat(readResult.get(0).value).isEqualTo(ushort(0));
        assertThat(readResult.get(1).index).isEqualTo(ushort(1));
        assertThat(readResult.get(1).value).isEqualTo(ushort(22));
    }

    private void testWriteMultipleCoils(Channel client) throws ExecutionException, InterruptedException {
        RequestParam param = new RequestParam(UNIT_ID, TIMEOUT);

        client.writeMultipleCoils(param, ushort(0), Arrays.asList(true, false, true)).toCompletableFuture().get();

        // ======

        AddressRange range = new AddressRange(ushort(0), ushort(3));

        List<BitValue> readResult = client.readCoils(param, range).toCompletableFuture().get();

        assertThat(readResult).hasSize(3);
        assertThat(readResult.get(0).index).isEqualTo(ushort(0));
        assertThat(readResult.get(0).value).isEqualTo(true);
        assertThat(readResult.get(1).index).isEqualTo(ushort(1));
        assertThat(readResult.get(1).value).isEqualTo(false);
        assertThat(readResult.get(2).index).isEqualTo(ushort(2));
        assertThat(readResult.get(2).value).isEqualTo(true);
    }

    private void testWriteMultipleRegisters(Channel client) throws ExecutionException, InterruptedException {
        RequestParam param = new RequestParam(UNIT_ID, TIMEOUT);

        client.writeMultipleRegisters(param, ushort(0), Arrays.asList(ushort(0xCAFE), ushort(21), ushort(0xFFFF))).toCompletableFuture().get();

        // ======

        AddressRange range = new AddressRange(ushort(0), ushort(3));

        List<RegisterValue> readResult = client.readHoldingRegisters(param, range).toCompletableFuture().get();

        assertThat(readResult).hasSize(3);
        assertThat(readResult.get(0).index).isEqualTo(ushort(0));
        assertThat(readResult.get(0).value).isEqualTo(ushort(0xCAFE));
        assertThat(readResult.get(1).index).isEqualTo(ushort(1));
        assertThat(readResult.get(1).value).isEqualTo(ushort(21));
        assertThat(readResult.get(2).index).isEqualTo(ushort(2));
        assertThat(readResult.get(2).value).isEqualTo(ushort(0xFFFF));
    }
}
