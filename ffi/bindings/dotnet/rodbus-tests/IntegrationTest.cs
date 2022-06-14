using Microsoft.VisualStudio.TestTools.UnitTesting;

using rodbus;
using System;
using System.Collections.Generic;
using System.Linq;

namespace rodbus_tests
{
    class WriteHandler : IWriteHandler
    {
        public WriteResult WriteSingleCoil(ushort index, bool value, Database database)
        {
            if (database.UpdateCoil(index, value))
            {
                return WriteResult.SuccessInit();
            }
            else
            {
                return WriteResult.ExceptionInit(rodbus.ModbusException.IllegalDataAddress);
            }
        }

        public WriteResult WriteSingleRegister(ushort index, ushort value, Database database)
        {
            if (database.UpdateHoldingRegister(index, value))
            {
                return WriteResult.SuccessInit();
            }
            else
            {
                return WriteResult.ExceptionInit(rodbus.ModbusException.IllegalDataAddress);
            }
        }

        public WriteResult WriteMultipleCoils(ushort start, ICollection<BitValue> it, Database database)
        {
            foreach (var bit in it)
            {
                if (!database.UpdateCoil(bit.Index, bit.Value))
                {
                    return WriteResult.ExceptionInit(rodbus.ModbusException.IllegalDataAddress);
                }
            }

            return WriteResult.SuccessInit();
        }

        public WriteResult WriteMultipleRegisters(ushort start, ICollection<RegisterValue> it, Database database)
        {
            foreach (var bit in it)
            {
                if (!database.UpdateHoldingRegister(bit.Index, bit.Value))
                {
                    return WriteResult.ExceptionInit(rodbus.ModbusException.IllegalDataAddress);
                }
            }

            return WriteResult.SuccessInit();
        }
    }

    [TestClass]
    public class IntegrationTest
    {
        private static readonly byte UNIT_ID = 1;
        private static readonly ushort NUM_POINTS = 10;
        // we use 50001 here since it's a large enough port it doesn't require root on Linux
        private static readonly string ENDPOINT = "127.0.0.1";
        private static readonly ushort PORT = 50001;
        private static readonly RequestParam param = new RequestParam(UNIT_ID, TimeSpan.FromSeconds(1));

        static void TestReadDiscreteInputs(ClientChannel client)
        {
            var result = client.ReadDiscreteInputs(param, new AddressRange(2, 3)).Result.ToList();
            Assert.AreEqual(3, result.Count);
            Assert.AreEqual(2, result[0].Index);
            Assert.AreEqual(false, result[0].Value);
            Assert.AreEqual(3, result[1].Index);
            Assert.AreEqual(true, result[1].Value);
            Assert.AreEqual(4, result[2].Index);
            Assert.AreEqual(false, result[2].Value);

            try
            {
                client.ReadDiscreteInputs(param, new AddressRange(9, 2)).Wait();
                Assert.Fail("reading invalid address range did not fail");
            }
            catch (AggregateException ex)
            {
                Assert.AreEqual(RequestError.ModbusExceptionIllegalDataAddress, (ex.InnerException as RequestException).error);
            }
        }

        static void TestReadInputRegisters(ClientChannel client)
        {
            var result = client.ReadInputRegisters(param, new AddressRange(3, 3)).Result.ToList();
            Assert.AreEqual(3, result.Count);
            Assert.AreEqual(3, result[0].Index);
            Assert.AreEqual(0, result[0].Value);
            Assert.AreEqual(4, result[1].Index);
            Assert.AreEqual(42, result[1].Value);
            Assert.AreEqual(5, result[2].Index);
            Assert.AreEqual(0, result[2].Value);

            try
            {
                client.ReadInputRegisters(param, new AddressRange(10, 1)).Wait();
                Assert.Fail("reading invalid address range did not fail");
            }
            catch(AggregateException ex)
            {
                Assert.AreEqual(RequestError.ModbusExceptionIllegalDataAddress, (ex.InnerException as RequestException).error);
            }
        }

        static void TestWriteSingleCoil(ClientChannel client)
        {
            client.WriteSingleCoil(param, new BitValue(1, true)).Wait();

            var result = client.ReadCoils(param, new AddressRange(0, 2)).Result.ToList();
            Assert.AreEqual(2, result.Count);
            Assert.AreEqual(0, result[0].Index);
            Assert.AreEqual(false, result[0].Value);
            Assert.AreEqual(1, result[1].Index);
            Assert.AreEqual(true, result[1].Value);
        }

        static void TestWriteSingleRegister(ClientChannel client)
        {
            client.WriteSingleRegister(param, new RegisterValue(1, 22)).Wait();

            var result = client.ReadHoldingRegisters(param, new AddressRange(0, 2)).Result.ToList();
            Assert.AreEqual(2, result.Count);
            Assert.AreEqual(0, result[0].Index);
            Assert.AreEqual(0, result[0].Value);
            Assert.AreEqual(1, result[1].Index);
            Assert.AreEqual(22, result[1].Value);
        }

        static void TestWriteMultipeCoils(ClientChannel client)
        {
            client.WriteMultipleCoils(param, 0, new List<bool> { true, false, true }).Wait();

            var result = client.ReadCoils(param, new AddressRange(0, 3)).Result.ToList();
            Assert.AreEqual(3, result.Count);
            Assert.AreEqual(0, result[0].Index);
            Assert.AreEqual(true, result[0].Value);
            Assert.AreEqual(1, result[1].Index);
            Assert.AreEqual(false, result[1].Value);
            Assert.AreEqual(2, result[2].Index);
            Assert.AreEqual(true, result[2].Value);
        }

        static void TestWriteMultipeRegisters(ClientChannel client)
        {
            client.WriteMultipleRegisters(param, 0, new List<ushort> { 0xCAFE, 21, 0xFFFF }).Wait();

            var result = client.ReadHoldingRegisters(param, new AddressRange(0, 3)).Result.ToList();
            Assert.AreEqual(3, result.Count);
            Assert.AreEqual(0, result[0].Index);
            Assert.AreEqual(0xCAFE, result[0].Value);
            Assert.AreEqual(1, result[1].Index);
            Assert.AreEqual(21, result[1].Value);
            Assert.AreEqual(2, result[2].Index);
            Assert.AreEqual(0xFFFF, result[2].Value);
        }

        [TestMethod]
        public void ClientAndServerCommunication()
        {
            var runtime = new Runtime(new RuntimeConfig());
            var map = new DeviceMap();
            map.AddEndpoint(UNIT_ID, new WriteHandler(), db =>
            {
                for (ushort i = 0; i < NUM_POINTS; ++i)
                {
                    db.AddCoil(i, false);
                    db.AddDiscreteInput(i, false);
                    db.AddHoldingRegister(i, 0);
                    db.AddInputRegister(i, 0);
                }
            });

            var server = Server.CreateTcp(runtime, ENDPOINT, 100, map, DecodeLevel.Nothing());
            var client = ClientChannel.CreateTcp(runtime, ENDPOINT, PORT, 10, new RetryStrategy(), DecodeLevel.Nothing());

            // set a unique pattern to test reads
            server.UpdateDatabase(UNIT_ID, db =>
            {
                db.UpdateDiscreteInput(3, true);
                db.UpdateInputRegister(4, 42);
            });

            TestReadDiscreteInputs(client);
            TestReadInputRegisters(client);
            TestWriteSingleCoil(client);
            TestWriteSingleRegister(client);
            TestWriteMultipeCoils(client);
            TestWriteMultipeRegisters(client);
        }
    }
}
