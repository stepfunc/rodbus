using System;
using System.Collections.Generic;
using System.Threading;
using rodbus;

namespace example
{
    class Program
    {
        class ConsoleLogger : ILogger
        {
            public void OnMessage(LogLevel level, string message)
            {
                Console.Write($"{level}: {message}");
            }
        }

        class WriteHandler : IWriteHandler
        {
            public WriteResult WriteMultipleCoils(ushort start, ICollection<Bit> it, Database database)
            {
                foreach (var bit in it)
                {
                    if (!database.UpdateCoil(bit.Index, bit.Value))
                    {
                        return WriteResult.CreateException(rodbus.ModbusException.IllegalDataAddress);
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
                        return WriteResult.CreateException(rodbus.ModbusException.IllegalDataAddress);
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
                    return WriteResult.CreateException(rodbus.ModbusException.IllegalDataAddress);
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
                    return WriteResult.CreateException(rodbus.ModbusException.IllegalDataAddress);
                }
            }
        }

        static void Main(string[] args)
        {
            // initialize logging with the default configuration
            Logging.Configure(
                new LoggingConfig(),
                new ConsoleLogger()
            );

            // initialize the runtime
            var runtime = new Runtime(new RuntimeConfig { NumCoreThreads = 4 });

            // create the device map
            var map = new DeviceMap();
            map.AddEndpoint(1, new WriteHandler(), new DatabaseCallback((db) =>
            {
                for (ushort i = 0; i < 10; ++i)
                {
                    db.AddCoil(i, false);
                    db.AddDiscreteInput(i, false);
                    db.AddHoldingRegister(i, 0);
                    db.AddInputRegister(i, 0);
                }
            }));

            // create the TCP server
            var decodeLevel = new DecodeLevel();
            var server = Server.CreateTcpServer(runtime, "127.0.0.1:502", 10, map, decodeLevel);

            try
            {
                RunServer(server);
            }
            finally
            {
                runtime.Shutdown();
            }
        }

        private static void RunServer(Server server)
        {
            bool coilValue = false;
            bool discreteInputValue = false;
            ushort holdingRegisterValue = 0;
            ushort inputRegisterValue = 0;

            while (true)
            {
                switch(Console.ReadLine())
                {
                    case "x":
                        return;
                    case "uc":
                        server.Update(1, new DatabaseCallback((db) =>
                        {
                            coilValue = !coilValue;
                            for (ushort i = 0; i < 10; ++i)
                            {
                                db.UpdateCoil(i, coilValue);
                            }
                        }));
                        break;
                    case "udi":
                        server.Update(1, new DatabaseCallback((db) =>
                        {
                            discreteInputValue = !discreteInputValue;
                            for (ushort i = 0; i < 10; ++i)
                            {
                                db.UpdateDiscreteInput(i, discreteInputValue);
                            }
                        }));
                        break;
                    case "uhr":
                        server.Update(1, new DatabaseCallback((db) =>
                        {
                            ++holdingRegisterValue;
                            for (ushort i = 0; i < 10; ++i)
                            {
                                db.UpdateHoldingRegister(i, holdingRegisterValue);
                            }
                        }));
                        break;
                    case "uir":
                        server.Update(1, new DatabaseCallback((db) =>
                        {
                            ++inputRegisterValue;
                            for (ushort i = 0; i < 10; ++i)
                            {
                                db.UpdateInputRegister(i, inputRegisterValue);
                            }
                        }));
                        break;
                    default:
                        Console.WriteLine("Unknown command");
                        break;
                }
            }
        }
    }
}
