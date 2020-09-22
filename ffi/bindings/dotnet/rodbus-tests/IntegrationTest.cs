using Microsoft.VisualStudio.TestTools.UnitTesting;

using rodbus;
using System.Collections.Generic;
using System.Linq;

namespace rodbus_tests
{
    class WriteHandler : IWriteHandler
    {
        public WriteResult WriteMultipleCoils(ushort start, ICollection<Bit> it, Database database)
        {
            foreach (var bit in it)
            {
                if (!database.UpdateCoil(bit.Index, bit.Value))
                {
                    return WriteResult.CreateException(rodbus.Exception.IllegalDataAddress);
                }
            }

            return WriteResult.CreateSuccess();
        }

        public WriteResult WriteMultipleRegisters(ushort start, ICollection<Register> it, Database database)
        {
            foreach (var bit in it)
            {
                if (!database.UpdateHoldingRegister(bit.Index, bit.Value))
                {
                    return WriteResult.CreateException(rodbus.Exception.IllegalDataAddress);
                }
            }

            return WriteResult.CreateSuccess();
        }

        public WriteResult WriteSingleCoil(bool value, ushort index, Database database)
        {
            if (database.UpdateCoil(index, value))
            {
                return WriteResult.CreateSuccess();
            }
            else
            {
                return WriteResult.CreateException(rodbus.Exception.IllegalDataAddress);
            }
        }

        public WriteResult WriteSingleRegister(ushort value, ushort index, Database database)
        {
            if (database.UpdateHoldingRegister(index, value))
            {
                return WriteResult.CreateSuccess();
            }
            else
            {
                return WriteResult.CreateException(rodbus.Exception.IllegalDataAddress);
            }
        }
    }

    class DatabaseUpdate : IDatabaseCallback
    {
        readonly System.Action<Database> action;

        public DatabaseUpdate(System.Action<Database> action)
        {
            this.action = action;
        }

        public void Callback(Database database)
        {
            this.action.Invoke(database);
        }
    }

    [TestClass]
    public class IntegrationTest
    {
        private static readonly byte UNIT_ID = 1;
        private static readonly ushort NUM_POINTS = 10;
        private static readonly string ENDPOINT = "127.0.0.1:50000";
        private static readonly RequestParam param = new RequestParam { UnitId = UNIT_ID, TimeoutMs = 1000 };

        static void TestReadDiscreteInputs(Channel client)
        {
            var result = client.ReadDiscreteInputs(new AddressRange { Start = 2, Count = 3 }, param).Result;
            Assert.AreEqual(Status.Ok, result.Result.Summary);
            CollectionAssert.AreEqual(
                new List<Bit>
                {
                        new Bit { Index = 2, Value = false },
                        new Bit { Index = 3, Value = true },
                        new Bit { Index = 4, Value = false },
                },
                result.Iterator.ToList()
            );
            result = client.ReadDiscreteInputs(new AddressRange { Start = 9, Count = 2 }, param).Result;
            Assert.AreEqual(Status.Exception, result.Result.Summary);
            Assert.AreEqual(Exception.IllegalDataAddress, result.Result.Exception);
        }

        static void TestReadInputRegisters(Channel client)
        {
            var result = client.ReadInputRegisters(new AddressRange { Start = 3, Count = 3 }, param).Result;
            Assert.AreEqual(Status.Ok, result.Result.Summary);
            CollectionAssert.AreEqual(
                new List<Register>
                {
                        new Register { Index = 3, Value = 0 },
                        new Register { Index = 4, Value = 42 },
                        new Register { Index = 5, Value = 0 },
                },
                result.Iterator.ToList()
            );
            result = client.ReadInputRegisters(new AddressRange { Start = 10, Count = 1 }, param).Result;
            Assert.AreEqual(Status.Exception, result.Result.Summary);
            Assert.AreEqual(Exception.IllegalDataAddress, result.Result.Exception);
        }

        static void TestWriteSingleCoil(Channel client)
        {            
            var writeResult = client.WriteSingleCoil(new Bit { Index = 1, Value = true }, param).Result;
            Assert.AreEqual(Status.Ok, writeResult.Summary);
            var readResult = client.ReadCoils(new AddressRange { Start = 0, Count = 2 }, param).Result;
            Assert.AreEqual(Status.Ok, readResult.Result.Summary);
            CollectionAssert.AreEqual(
                new List<Bit>
                {
                        new Bit { Index = 0, Value = false },
                        new Bit { Index = 1, Value = true },
                },
                readResult.Iterator.ToList()
            );
        }

        static void TestWriteSingleRegister(Channel client)
        {            
            var writeResult = client.WriteSingleRegister(new Register { Index = 1, Value = 22 }, param).Result;
            Assert.AreEqual(Status.Ok, writeResult.Summary);
            var readResult = client.ReadHoldingRegisters(new AddressRange { Start = 0, Count = 2 }, param).Result;
            Assert.AreEqual(Status.Ok, readResult.Result.Summary);
            CollectionAssert.AreEqual(
                new List<Register>
                {
                        new Register { Index = 0, Value = 0 },
                        new Register { Index = 1, Value = 22 },
                },
                readResult.Iterator.ToList()
            );
        }

        static void TestWriteMultipeCoils(Channel client)
        {            
            var writeResult = client.WriteMultipleCoils(0, new List<bool> { true, false, true }, param).Result;
            Assert.AreEqual(Status.Ok, writeResult.Summary);
            var readResult = client.ReadCoils(new AddressRange { Start = 0, Count = 3 }, param).Result;
            Assert.AreEqual(Status.Ok, readResult.Result.Summary);
            CollectionAssert.AreEqual(
                new List<Bit>
                {
                        new Bit { Index = 0, Value = true },
                        new Bit { Index = 1, Value = false },
                        new Bit { Index = 2, Value = true },
                },
                readResult.Iterator.ToList()
            );
        }

        static void TestWriteMultipeRegisters(Channel client)
        {
            var writeResult = client.WriteMultipleRegisters(0, new List<ushort> { 0xCAFE, 21, 0xFFFF }, param).Result;
            Assert.AreEqual(Status.Ok, writeResult.Summary);
            var readResult = client.ReadHoldingRegisters(new AddressRange { Start = 0, Count = 3 }, param).Result;
            Assert.AreEqual(Status.Ok, readResult.Result.Summary);
            CollectionAssert.AreEqual(
                new List<Register>
                {
                        new Register { Index = 0, Value = 0xCAFE },
                        new Register { Index = 1, Value = 21 },
                        new Register { Index = 2, Value = 0xFFFF },
                },
                readResult.Iterator.ToList()
            );
        }

        [TestMethod]
        public void ClientAndServerCommunication()
        {
            using var runtime = new Runtime(new RuntimeConfig { NumCoreThreads = 2 });
            var map = new DeviceMap();
            map.AddEndpoint(UNIT_ID, new WriteHandler(), new DatabaseUpdate((db) =>
            {
                for (ushort i = 0; i < NUM_POINTS; ++i)
                {
                    db.AddCoil(i, false);
                    db.AddDiscreteInput(i, false);
                    db.AddHoldingRegister(i, 0);
                    db.AddInputRegister(i, 0);
                }
            }));

            var server = Server.CreateTcpServer(runtime, ENDPOINT, map);
            var client = Channel.CreateTcpClient(runtime, ENDPOINT, 10);

            // set a unique pattern to test reads
            server.Update(UNIT_ID, new DatabaseUpdate(db =>
            {
                db.UpdateDiscreteInput(3, true);
                db.UpdateInputRegister(4, 42);
            }));

            TestReadDiscreteInputs(client);
            TestReadInputRegisters(client);
            TestWriteSingleCoil(client);
            TestWriteSingleRegister(client);
            TestWriteMultipeCoils(client);
            TestWriteMultipeRegisters(client);
        }
    }
}
