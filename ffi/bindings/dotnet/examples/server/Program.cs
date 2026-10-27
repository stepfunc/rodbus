// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

// Server example for the Rodbus library
//
// usage: server <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]
//
// Run it from the repository root so the TLS certificate paths resolve.

using System;
using System.Collections.Generic;
using rodbus;

namespace example
{
    class Program
    {
        const string Usage = "usage: server <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]";
        const byte UnitId = 1;
        const string TcpEndpoint = "127.0.0.1:502";
        const string TlsEndpoint = "127.0.0.1:802";
        const string SerialPort = "/dev/ttySIM1";
        const ushort MaxSessions = 10;
        const ushort PointCount = 10;

        static int Main(string[] args)
        {
            // initialize logging with the default configuration
            Logging.Configure(
                new LoggingConfig(),
                new ConsoleLogger()
            );

            if (args.Length < 1 || args.Length > 2 || !IsTransport(args[0]))
            {
                return PrintUsage();
            }

            var runtime = new Runtime(new RuntimeConfig { NumCoreThreads = 4 });

            Server server = null;
            try
            {
                var map = CreateDeviceMap();
                server = CreateServer(runtime, args[0], args.Length == 2 ? args[1] : null, map);

                PrintHelp();

                // exit on "x" or at the end of input
                while (true)
                {
                    var line = Console.ReadLine();
                    if (line == null || line == "x")
                    {
                        return 0;
                    }
                    if (line == "help")
                    {
                        PrintHelp();
                    }
                    else
                    {
                        RunCommand(server, line);
                    }
                }
            }
            catch (Exception ex)
            {
                Console.WriteLine($"error: {ex.Message}");
                return -1;
            }
            finally
            {
                server?.Shutdown();
                runtime.Shutdown();
            }
        }

        private static DeviceMap CreateDeviceMap()
        {
            // ANCHOR: device_map_init
            var map = new DeviceMap();
            map.AddEndpoint(UnitId, new WriteHandler(), db =>
            {
                for (ushort i = 0; i < PointCount; ++i)
                {
                    db.AddCoil(i, false);
                    db.AddDiscreteInput(i, false);
                    db.AddHoldingRegister(i, 0);
                    db.AddInputRegister(i, 0);
                }
            });
            // ANCHOR_END: device_map_init

            return map;
        }

        private static Server CreateServer(Runtime runtime, string transport, string endpoint, DeviceMap map)
        {
            // Main has already checked the transport word
            if (transport == "tcp")
            {
                return CreateTcpServer(runtime, endpoint ?? TcpEndpoint, map);
            }
            if (transport == "rtu")
            {
                return CreateRtuServer(runtime, endpoint ?? SerialPort, map);
            }
            if (transport == "tls-ca")
            {
                return CreateTlsServer(runtime, endpoint ?? TlsEndpoint, map, GetCaChainConfig());
            }
            return CreateTlsServer(runtime, endpoint ?? TlsEndpoint, map, GetSelfSignedConfig());
        }

        private static Server CreateTcpServer(Runtime runtime, string endpoint, DeviceMap map)
        {
            var (address, port) = SplitEndpoint(endpoint);

            // ANCHOR: tcp_server_create
            var server = Server.CreateTcp(
                runtime,
                address,
                port,
                AddressFilter.Any(),
                MaxSessions,
                map,
                new DecodeLevel(AppDecodeLevel.DataHeaders, FrameDecodeLevel.Nothing, PhysDecodeLevel.Nothing)
            );
            // ANCHOR_END: tcp_server_create

            return server;
        }

        private static Server CreateRtuServer(Runtime runtime, string path, DeviceMap map)
        {
            // ANCHOR: rtu_server_create
            var server = Server.CreateRtu(
                runtime,
                path,
                new SerialPortSettings(), // 9600 baud, 8 data bits, no parity, 1 stop bit
                new RetryStrategy(),
                map,
                new DecodeLevel(AppDecodeLevel.DataHeaders, FrameDecodeLevel.Nothing, PhysDecodeLevel.Nothing)
            );
            // ANCHOR_END: rtu_server_create

            return server;
        }

        private static Server CreateTlsServer(Runtime runtime, string endpoint, DeviceMap map, TlsServerConfig tlsConfig)
        {
            var (address, port) = SplitEndpoint(endpoint);

            // ANCHOR: tls_server_create
            var server = Server.CreateTlsWithAuthz(
                runtime,
                address,
                port,
                AddressFilter.Any(),
                MaxSessions,
                map,
                tlsConfig,
                new AuthorizationHandler(),
                new DecodeLevel(AppDecodeLevel.DataHeaders, FrameDecodeLevel.Nothing, PhysDecodeLevel.Nothing)
            );
            // ANCHOR_END: tls_server_create

            return server;
        }

        private static TlsServerConfig GetCaChainConfig()
        {
            // ANCHOR: tls_ca_chain_config
            var tlsConfig = new TlsServerConfig(
                "./certs/ca_chain/ca_cert.pem",
                "./certs/ca_chain/server_cert.pem",
                "./certs/ca_chain/server_key.pem",
                "" // no password
            ).WithMinTlsVersion(MinTlsVersion.V12);
            // ANCHOR_END: tls_ca_chain_config

            return tlsConfig;
        }

        private static TlsServerConfig GetSelfSignedConfig()
        {
            // ANCHOR: tls_self_signed_config
            var tlsConfig = new TlsServerConfig(
                "./certs/self_signed/entity1_cert.pem",
                "./certs/self_signed/entity2_cert.pem",
                "./certs/self_signed/entity2_key.pem",
                "" // no password
            ).WithMinTlsVersion(MinTlsVersion.V12).WithCertificateMode(CertificateMode.SelfSigned);
            // ANCHOR_END: tls_self_signed_config

            return tlsConfig;
        }

        private static void PrintHelp()
        {
            Console.WriteLine("Commands:");
            Console.WriteLine("  help - print this list");
            Console.WriteLine("  x - exit");
            Console.WriteLine("  ed - enable protocol decoding");
            Console.WriteLine("  dd - disable protocol decoding");
            Console.WriteLine("  uc - toggle every coil");
            Console.WriteLine("  udi - toggle every discrete input");
            Console.WriteLine("  uhr - increment every holding register");
            Console.WriteLine("  uir - increment every input register");
        }

        private static void RunCommand(Server server, string cmd)
        {
            // a rejected call throws a ParamException
            try
            {
                // the decode level commands only queue the change
                if (cmd == "ed")
                {
                    server.SetDecodeLevel(new DecodeLevel(AppDecodeLevel.DataValues, FrameDecodeLevel.Header, PhysDecodeLevel.Length));
                    Console.WriteLine($"{cmd}: requested");
                }
                else if (cmd == "dd")
                {
                    server.SetDecodeLevel(DecodeLevel.Nothing());
                    Console.WriteLine($"{cmd}: requested");
                }
                else if (cmd == "uc")
                {
                    // ANCHOR: update_coil
                    server.UpdateDatabase(UnitId, db =>
                    {
                        for (ushort i = 0; i < PointCount; ++i)
                        {
                            db.UpdateCoil(i, !db.GetCoil(i));
                        }
                    });
                    // ANCHOR_END: update_coil
                    Console.WriteLine($"{cmd}: success");
                }
                else if (cmd == "udi")
                {
                    server.UpdateDatabase(UnitId, db =>
                    {
                        for (ushort i = 0; i < PointCount; ++i)
                        {
                            db.UpdateDiscreteInput(i, !db.GetDiscreteInput(i));
                        }
                    });
                    Console.WriteLine($"{cmd}: success");
                }
                else if (cmd == "uhr")
                {
                    server.UpdateDatabase(UnitId, db =>
                    {
                        for (ushort i = 0; i < PointCount; ++i)
                        {
                            db.UpdateHoldingRegister(i, unchecked((ushort)(db.GetHoldingRegister(i) + 1)));
                        }
                    });
                    Console.WriteLine($"{cmd}: success");
                }
                else if (cmd == "uir")
                {
                    server.UpdateDatabase(UnitId, db =>
                    {
                        for (ushort i = 0; i < PointCount; ++i)
                        {
                            db.UpdateInputRegister(i, unchecked((ushort)(db.GetInputRegister(i) + 1)));
                        }
                    });
                    Console.WriteLine($"{cmd}: success");
                }
                else
                {
                    Console.WriteLine($"{cmd}: unknown command, type help for a list");
                }
            }
            catch (ParamException ex)
            {
                Console.WriteLine($"{cmd}: {ex.Message}");
            }
        }

        class ConsoleLogger : ILogger
        {
            public void OnMessage(LogLevel level, string message)
            {
                // the message already carries the level and ends in a newline
                Console.Write(message);
            }
        }

        // ANCHOR: write_handler
        class WriteHandler : IWriteHandler
        {
            public WriteResult WriteSingleCoil(ushort index, bool value, Database database)
            {
                if (!database.UpdateCoil(index, value))
                {
                    return WriteResult.ExceptionInit(ModbusException.IllegalDataAddress);
                }
                Console.WriteLine($"write coil {index}: {(value ? "true" : "false")}");
                return WriteResult.SuccessInit();
            }

            public WriteResult WriteSingleRegister(ushort index, ushort value, Database database)
            {
                if (!database.UpdateHoldingRegister(index, value))
                {
                    return WriteResult.ExceptionInit(ModbusException.IllegalDataAddress);
                }
                Console.WriteLine($"write holding register {index}: {value}");
                return WriteResult.SuccessInit();
            }

            public WriteResult WriteMultipleCoils(ushort start, ICollection<BitValue> values, Database database)
            {
                // check every coil exists before changing any
                foreach (var value in values)
                {
                    if (!HasCoil(database, value.Index))
                    {
                        return WriteResult.ExceptionInit(ModbusException.IllegalDataAddress);
                    }
                }
                foreach (var value in values)
                {
                    database.UpdateCoil(value.Index, value.Value);
                    Console.WriteLine($"write coil {value.Index}: {(value.Value ? "true" : "false")}");
                }
                return WriteResult.SuccessInit();
            }

            public WriteResult WriteMultipleRegisters(ushort start, ICollection<RegisterValue> values, Database database)
            {
                // check every register exists before changing any
                foreach (var value in values)
                {
                    if (!HasHoldingRegister(database, value.Index))
                    {
                        return WriteResult.ExceptionInit(ModbusException.IllegalDataAddress);
                    }
                }
                foreach (var value in values)
                {
                    database.UpdateHoldingRegister(value.Index, value.Value);
                    Console.WriteLine($"write holding register {value.Index}: {value.Value}");
                }
                return WriteResult.SuccessInit();
            }

            private static bool HasCoil(Database database, ushort index)
            {
                try
                {
                    database.GetCoil(index);
                    return true;
                }
                catch (ParamException)
                {
                    return false;
                }
            }

            private static bool HasHoldingRegister(Database database, ushort index)
            {
                try
                {
                    database.GetHoldingRegister(index);
                    return true;
                }
                catch (ParamException)
                {
                    return false;
                }
            }
        }
        // ANCHOR_END: write_handler

        // ANCHOR: auth_handler
        // Allows any role to read, and only the "operator" role to write
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
                return AuthorizeWrite(unitId, role);
            }

            public Authorization WriteSingleRegister(byte unitId, ushort idx, string role)
            {
                return AuthorizeWrite(unitId, role);
            }

            public Authorization WriteMultipleCoils(byte unitId, AddressRange range, string role)
            {
                return AuthorizeWrite(unitId, role);
            }

            public Authorization WriteMultipleRegisters(byte unitId, AddressRange range, string role)
            {
                return AuthorizeWrite(unitId, role);
            }

            private static Authorization AuthorizeWrite(byte unitId, string role)
            {
                if (role == "operator")
                {
                    return Authorization.Allow;
                }
                Console.WriteLine($"denied write to unit {unitId} for role '{role}'");
                return Authorization.Deny;
            }
        }
        // ANCHOR_END: auth_handler

        // split "host:port" at the last ':'
        private static (string, ushort) SplitEndpoint(string endpoint)
        {
            var colon = endpoint.LastIndexOf(':');
            if (colon < 0 || !ushort.TryParse(endpoint.Substring(colon + 1), out var port))
            {
                throw new FormatException($"invalid endpoint '{endpoint}', expected host:port");
            }
            return (endpoint.Substring(0, colon), port);
        }

        private static bool IsTransport(string transport)
        {
            return transport == "tcp" || transport == "rtu" || transport == "tls-ca" || transport == "tls-self-signed";
        }

        private static int PrintUsage()
        {
            Console.Error.WriteLine(Usage);
            return -1;
        }
    }
}
