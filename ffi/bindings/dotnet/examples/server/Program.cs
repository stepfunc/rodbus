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

        // ANCHOR: write_handler
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
                var result = WriteResult.SuccessInit();

                foreach (var bit in it)
                {
                    if (!database.UpdateCoil(bit.Index, bit.Value))
                    {
                        result = WriteResult.ExceptionInit(rodbus.ModbusException.IllegalDataAddress);
                    }
                }

                return result;
            }

            public WriteResult WriteMultipleRegisters(ushort start, ICollection<RegisterValue> it, Database database)
            {
                var result = WriteResult.SuccessInit();

                foreach (var bit in it)
                {
                    if (!database.UpdateHoldingRegister(bit.Index, bit.Value))
                    {
                        result = WriteResult.ExceptionInit(rodbus.ModbusException.IllegalDataAddress);
                    }
                }

                return result;
            }
        }
        // ANCHOR_END: write_handler

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
            // ANCHOR: device_map_init
            var map = new DeviceMap();
            map.AddEndpoint(1, new WriteHandler(), db =>
            {
                for (ushort i = 0; i < 10; ++i)
                {
                    db.AddCoil(i, false);
                    db.AddDiscreteInput(i, false);
                    db.AddHoldingRegister(i, 0);
                    db.AddInputRegister(i, 0);
                }
            });
            // ANCHOR_END: device_map_init

            // create the TCP server
            // ANCHOR: tcp_server_create
            var decodeLevel = new DecodeLevel();
            var server = Server.CreateTcp(runtime, "127.0.0.1:502", 10, map, decodeLevel);
            // ANCHOR_END: tcp_server_create

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
                        // ANCHOR: update_coil
                        server.UpdateDatabase(1, db =>
                        {
                            coilValue = !coilValue;
                            for (ushort i = 0; i < 10; ++i)
                            {
                                db.UpdateCoil(i, coilValue);
                            }
                        });
                        // ANCHOR_END: update_coil
                        break;
                    case "udi":
                        server.UpdateDatabase(1, db =>
                        {
                            discreteInputValue = !discreteInputValue;
                            for (ushort i = 0; i < 10; ++i)
                            {
                                db.UpdateDiscreteInput(i, discreteInputValue);
                            }
                        });
                        break;
                    case "uhr":
                        server.UpdateDatabase(1, db =>
                        {
                            ++holdingRegisterValue;
                            for (ushort i = 0; i < 10; ++i)
                            {
                                db.UpdateHoldingRegister(i, holdingRegisterValue);
                            }
                        });
                        break;
                    case "uir":
                        server.UpdateDatabase(1, db =>
                        {
                            ++inputRegisterValue;
                            for (ushort i = 0; i < 10; ++i)
                            {
                                db.UpdateInputRegister(i, inputRegisterValue);
                            }
                        });
                        break;
                    default:
                        Console.WriteLine("Unknown command");
                        break;
                }
            }
        }
    }
}
