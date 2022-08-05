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

        // ANCHOR: auth_handler
        class AuthorizationHandler : IAuthorizationHandler
        {
            public Authorization ReadCoils(byte unitId, AddressRange range, string role)
            {
                return Authorization.Allow;
            }

            public Authorization ReadDiscreteInputs(byte unitId, AddressRange range, string role)
            {
                return Authorization.Allow;
            }

            public Authorization ReadHoldingRegisters(byte unitId, AddressRange range, string role)
            {
                return Authorization.Allow;
            }

            public Authorization ReadInputRegisters(byte unitId, AddressRange range, string role)
            {
                return Authorization.Allow;
            }

            public Authorization WriteSingleCoil(byte unitId, ushort idx, string role)
            {
                return Authorization.Deny;
            }

            public Authorization WriteSingleRegister(byte unitId, ushort idx, string role)
            {
                return Authorization.Deny;
            }

            public Authorization WriteMultipleCoils(byte unitId, AddressRange range, string role)
            {
                return Authorization.Deny;
            }

            public Authorization WriteMultipleRegisters(byte unitId, AddressRange range, string role)
            {
                return Authorization.Deny;
            }
        }
        // ANCHOR_END: auth_handler

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

            if (args.Length != 1)
            {
                Console.WriteLine("you must specify a transport type");
                Console.WriteLine("usage: server_example <channel> (tcp, rtu, tls-ca, tls-self-signed)");
                Environment.Exit(-1);
            }

            // create the TCP server
            var server = CreateServer(args[0], runtime, map);

            try
            {
                RunServer(server);
            }
            finally
            {
                runtime.Shutdown();
            }
        }

        private static Server CreateServer(string type, Runtime runtime, DeviceMap map)
        {
            switch (type)
            {
                case "tcp":
                    return CreateTcpServer(runtime, map);
                case "rtu":
                    return CreateRtuServer(runtime, map);
                case "tls-ca":
                    return CreateTlsServer(runtime, map, GetCaTlsConfig());
                case "tls-self-signed":
                    return CreateTlsServer(runtime, map, GetSelfSignedTlsConfig());
                default:
                    Console.WriteLine($"unknown server type: {type}");
                    Environment.Exit(-1);
                    return null;
            }
        }

        private static Server CreateTcpServer(Runtime runtime, DeviceMap map)
        {
            // ANCHOR: tcp_server_create            
            var server = Server.CreateTcp(runtime, "127.0.0.1", 502, AddressFilter.Any(), 100, map, DecodeLevel.Nothing());
            // ANCHOR_END: tcp_server_create

            return server;
        }

        private static Server CreateRtuServer(Runtime runtime, DeviceMap map)
        {
            // ANCHOR: rtu_server_create            
            var server = Server.CreateRtu(runtime, "/dev/ttySIM1", new SerialPortSettings(), map, DecodeLevel.Nothing());
            // ANCHOR_END: rtu_server_create

            return server;
        }

        private static Server CreateTlsServer(Runtime runtime, DeviceMap map, TlsServerConfig tlsConfig)
        {
            // ANCHOR: tls_server_create            
            var server = Server.CreateTlsWithAuthz(runtime, "127.0.0.1", 802, AddressFilter.Any(), 10, map, tlsConfig, new AuthorizationHandler(), DecodeLevel.Nothing());
            // ANCHOR_END: tls_server_create

            return server;
        }

        private static TlsServerConfig GetCaTlsConfig()
        {
            // ANCHOR: tls_ca_chain_config
            var tlsConfig = new TlsServerConfig(
                "./certs/ca_chain/ca_cert.pem",
                "./certs/ca_chain/server_cert.pem",
                "./certs/ca_chain/server_key.pem",
                "" // no password
            );
            // ANCHOR_END: tls_ca_chain_config

            return tlsConfig;
        }

        private static TlsServerConfig GetSelfSignedTlsConfig()
        {
            // ANCHOR: tls_self_signed_config
            var tlsConfig = new TlsServerConfig(
                "./certs/self_signed/entity1.pem",
                "./certs/self_signed/entity2_cert.pem",
                "./certs/self_signed/entity2_key.pem",
                "" // no password
            ).WithCertificateMode(CertificateMode.SelfSigned);
            // ANCHOR_END: tls_self_signed_config

            return tlsConfig;
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
