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
                    return WriteResult.CreateSuccess();
                }
                else
                {
                    return WriteResult.CreateException(rodbus.ModbusException.IllegalDataAddress);
                }
            }

            public WriteResult WriteSingleRegister(ushort index, ushort value, Database database)
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

            public WriteResult WriteMultipleCoils(ushort start, ICollection<Bit> it, Database database)
            {
                var result = WriteResult.CreateSuccess();

                foreach (var bit in it)
                {
                    if (!database.UpdateCoil(bit.Index, bit.Value))
                    {
                        result = WriteResult.CreateException(rodbus.ModbusException.IllegalDataAddress);
                    }
                }

                return result;
            }

            public WriteResult WriteMultipleRegisters(ushort start, ICollection<Register> it, Database database)
            {
                var result = WriteResult.CreateSuccess();

                foreach (var bit in it)
                {
                    if (!database.UpdateHoldingRegister(bit.Index, bit.Value))
                    {
                        result = WriteResult.CreateException(rodbus.ModbusException.IllegalDataAddress);
                    }
                }

                return result;
            }
        }
        // ANCHOR_END: write_handler

        // ANCHOR: auth_handler
        class AuthorizationHandler : IAuthorizationHandler
        {
            public AuthorizationResult ReadCoils(byte unitId, AddressRange range, string role)
            {
                return AuthorizationResult.Authorized;
            }

            public AuthorizationResult ReadDiscreteInputs(byte unitId, AddressRange range, string role)
            {
                return AuthorizationResult.Authorized;
            }

            public AuthorizationResult ReadHoldingRegisters(byte unitId, AddressRange range, string role)
            {
                return AuthorizationResult.Authorized;
            }

            public AuthorizationResult ReadInputRegisters(byte unitId, AddressRange range, string role)
            {
                return AuthorizationResult.Authorized;
            }

            public AuthorizationResult WriteSingleCoil(byte unitId, ushort idx, string role)
            {
                return AuthorizationResult.NotAuthorized;
            }

            public AuthorizationResult WriteSingleRegister(byte unitId, ushort idx, string role)
            {
                return AuthorizationResult.NotAuthorized;
            }

            public AuthorizationResult WriteMultipleCoils(byte unitId, AddressRange range, string role)
            {
                return AuthorizationResult.NotAuthorized;
            }

            public AuthorizationResult WriteMultipleRegisters(byte unitId, AddressRange range, string role)
            {
                return AuthorizationResult.NotAuthorized;
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
            // ANCHOR_END: device_map_init

            // ANCHOR: tls_self_signed_config
            var selfSignedTlsConfig = new TlsServerConfig(
                "./certs/self_signed/entity1.pem",
                "./certs/self_signed/entity2_cert.pem",
                "./certs/self_signed/entity2_key.pem",
                "" // no password
            );
            selfSignedTlsConfig.CertificateMode = CertificateMode.SelfSignedCertificate;
            // ANCHOR_END: tls_self_signed_config

            // ANCHOR: tls_ca_chain_config
            var caChainTlsConfig = new TlsServerConfig(
                "./certs/ca_chain/ca_cert.pem",
                "./certs/ca_chain/entity2_cert.pem",
                "./certs/ca_chain/entity2_key.pem",
                "" // no password
            );
            // ANCHOR_END: tls_ca_chain_config

            var tlsConfig = caChainTlsConfig;

            // create the TLS server
            // ANCHOR: tls_server_create
            var decodeLevel = new DecodeLevel();
            var server = Server.CreateTlsServer(runtime, "127.0.0.1:802", 10, map, tlsConfig, new AuthorizationHandler(), decodeLevel);
            // ANCHOR_END: tls_server_create

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
                        server.Update(1, new DatabaseCallback((db) =>
                        {
                            coilValue = !coilValue;
                            for (ushort i = 0; i < 10; ++i)
                            {
                                db.UpdateCoil(i, coilValue);
                            }
                        }));
                        // ANCHOR_END: update_coil
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
