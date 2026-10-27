// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

// Client example for the Rodbus library
//
// usage: client <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]
//
// Run it from the repository root so the TLS certificate paths resolve.

using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using rodbus;

namespace example
{
    class Program
    {
        const string Usage = "usage: client <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]";
        const byte UnitId = 1;
        const string TcpEndpoint = "127.0.0.1:502";
        const string TlsEndpoint = "127.0.0.1:802";
        const string SerialPort = "/dev/ttySIM0";
        const ushort MaxQueuedRequests = 10;
        static readonly TimeSpan RequestTimeout = TimeSpan.FromSeconds(1);

        static async Task<int> Main(string[] args)
        {
            // ANCHOR: logging_init
            // initialize logging with the default configuration
            Logging.Configure(
                new LoggingConfig(),
                new ConsoleLogger()
            );
            // ANCHOR_END: logging_init

            if (args.Length < 1 || args.Length > 2 || !IsTransport(args[0]))
            {
                return PrintUsage();
            }

            // ANCHOR: runtime_init
            var runtime = new Runtime(new RuntimeConfig { NumCoreThreads = 4 });
            // ANCHOR_END: runtime_init

            ClientChannel channel = null;
            try
            {
                channel = CreateChannel(runtime, args[0], args.Length == 2 ? args[1] : null);
                channel.Enable();

                // ANCHOR: request_param
                var param = new RequestParam(UnitId, RequestTimeout);
                // ANCHOR_END: request_param
                // ANCHOR: address_range
                var range = new AddressRange(0, 5);
                // ANCHOR_END: address_range

                PrintHelp();

                // exit on "x" or at the end of input
                while (true)
                {
                    var line = await Console.In.ReadLineAsync();
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
                        await RunCommand(channel, param, range, line);
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
                channel?.Shutdown();
                // ANCHOR: runtime_shutdown
                runtime.Shutdown();
                // ANCHOR_END: runtime_shutdown
            }
        }

        private static ClientChannel CreateChannel(Runtime runtime, string transport, string endpoint)
        {
            // Main has already checked the transport word
            if (transport == "tcp")
            {
                return CreateTcpChannel(runtime, endpoint ?? TcpEndpoint);
            }
            if (transport == "rtu")
            {
                return CreateRtuChannel(runtime, endpoint ?? SerialPort);
            }
            if (transport == "tls-ca")
            {
                return CreateTlsChannel(runtime, endpoint ?? TlsEndpoint, GetCaChainConfig());
            }
            return CreateTlsChannel(runtime, endpoint ?? TlsEndpoint, GetSelfSignedConfig());
        }

        private static ClientChannel CreateTcpChannel(Runtime runtime, string endpoint)
        {
            var (host, port) = SplitEndpoint(endpoint);

            // ANCHOR: create_tcp_channel
            var channel = ClientChannel.CreateTcp(
                runtime,
                host,
                port,
                MaxQueuedRequests,
                new RetryStrategy(),
                new DecodeLevel(AppDecodeLevel.DataHeaders, FrameDecodeLevel.Nothing, PhysDecodeLevel.Nothing),
                new ClientStateListener()
            );
            // ANCHOR_END: create_tcp_channel

            return channel;
        }

        private static ClientChannel CreateRtuChannel(Runtime runtime, string path)
        {
            // ANCHOR: create_rtu_channel
            var channel = ClientChannel.CreateRtu(
                runtime,
                path,
                new SerialPortSettings(), // 9600 baud, 8 data bits, no parity, 1 stop bit
                MaxQueuedRequests,
                new RetryStrategy(),
                new DecodeLevel(AppDecodeLevel.DataHeaders, FrameDecodeLevel.Nothing, PhysDecodeLevel.Nothing),
                new PortStateListener()
            );
            // ANCHOR_END: create_rtu_channel

            return channel;
        }

        private static ClientChannel CreateTlsChannel(Runtime runtime, string endpoint, TlsClientConfig tlsConfig)
        {
            var (host, port) = SplitEndpoint(endpoint);

            // ANCHOR: create_tls_channel
            var channel = ClientChannel.CreateTls(
                runtime,
                host,
                port,
                MaxQueuedRequests,
                new RetryStrategy(),
                tlsConfig,
                new DecodeLevel(AppDecodeLevel.DataHeaders, FrameDecodeLevel.Nothing, PhysDecodeLevel.Nothing),
                new ClientStateListener()
            );
            // ANCHOR_END: create_tls_channel

            return channel;
        }

        private static TlsClientConfig GetCaChainConfig()
        {
            // ANCHOR: tls_ca_chain_config
            var tlsConfig = new TlsClientConfig(
                "test.com",
                "./certs/ca_chain/ca_cert.pem",
                "./certs/ca_chain/client_cert.pem",
                "./certs/ca_chain/client_key.pem",
                "" // no password
            ).WithMinTlsVersion(MinTlsVersion.V12);
            // ANCHOR_END: tls_ca_chain_config

            return tlsConfig;
        }

        private static TlsClientConfig GetSelfSignedConfig()
        {
            // ANCHOR: tls_self_signed_config
            var tlsConfig = new TlsClientConfig(
                "test.com",
                "./certs/self_signed/entity2_cert.pem",
                "./certs/self_signed/entity1_cert.pem",
                "./certs/self_signed/entity1_key.pem",
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
            Console.WriteLine("  ec - enable the channel");
            Console.WriteLine("  dc - disable the channel");
            Console.WriteLine("  ed - enable protocol decoding");
            Console.WriteLine("  dd - disable protocol decoding");
            Console.WriteLine("  rc - read coils 0 to 4");
            Console.WriteLine("  rdi - read discrete inputs 0 to 4");
            Console.WriteLine("  rhr - read holding registers 0 to 4");
            Console.WriteLine("  rir - read input registers 0 to 4");
            Console.WriteLine("  wsc - write coil 0 = true");
            Console.WriteLine("  wsr - write holding register 0 = 76");
            Console.WriteLine("  wmc - write coils 0 and 1 = true, false");
            Console.WriteLine("  wmr - write holding registers 0 and 1 = 0xCA, 0xFE");
        }

        private static async Task RunCommand(ClientChannel channel, RequestParam param, AddressRange range, string cmd)
        {
            // a failed request throws a RequestException, a rejected argument a ParamException
            try
            {
                // the settings commands only queue the change; the listener shows the channel's state
                if (cmd == "ec")
                {
                    channel.Enable();
                    Console.WriteLine($"{cmd}: requested");
                }
                else if (cmd == "dc")
                {
                    channel.Disable();
                    Console.WriteLine($"{cmd}: requested");
                }
                else if (cmd == "ed")
                {
                    channel.SetDecodeLevel(new DecodeLevel(AppDecodeLevel.DataValues, FrameDecodeLevel.Header, PhysDecodeLevel.Length));
                    Console.WriteLine($"{cmd}: requested");
                }
                else if (cmd == "dd")
                {
                    channel.SetDecodeLevel(DecodeLevel.Nothing());
                    Console.WriteLine($"{cmd}: requested");
                }
                else if (cmd == "rc")
                {
                    // ANCHOR: read_coils
                    var coils = await channel.ReadCoils(param, range);
                    // ANCHOR_END: read_coils
                    PrintValues(cmd, "coil", coils);
                }
                else if (cmd == "rdi")
                {
                    var inputs = await channel.ReadDiscreteInputs(param, range);
                    PrintValues(cmd, "discrete input", inputs);
                }
                else if (cmd == "rhr")
                {
                    var registers = await channel.ReadHoldingRegisters(param, range);
                    PrintValues(cmd, "holding register", registers);
                }
                else if (cmd == "rir")
                {
                    var registers = await channel.ReadInputRegisters(param, range);
                    PrintValues(cmd, "input register", registers);
                }
                else if (cmd == "wsc")
                {
                    // ANCHOR: write_single_coil
                    await channel.WriteSingleCoil(param, new BitValue(0, true));
                    // ANCHOR_END: write_single_coil
                    Console.WriteLine($"{cmd}: success");
                }
                else if (cmd == "wsr")
                {
                    await channel.WriteSingleRegister(param, new RegisterValue(0, 76));
                    Console.WriteLine($"{cmd}: success");
                }
                else if (cmd == "wmc")
                {
                    await channel.WriteMultipleCoils(param, 0, new List<bool>() { true, false });
                    Console.WriteLine($"{cmd}: success");
                }
                else if (cmd == "wmr")
                {
                    // ANCHOR: write_multiple_registers
                    await channel.WriteMultipleRegisters(param, 0, new List<ushort>() { 0xCA, 0xFE });
                    // ANCHOR_END: write_multiple_registers
                    Console.WriteLine($"{cmd}: success");
                }
                else
                {
                    Console.WriteLine($"{cmd}: unknown command, type help for a list");
                }
            }
            catch (RequestException ex)
            {
                Console.WriteLine($"{cmd}: {ex.Message}");
            }
            catch (ParamException ex)
            {
                Console.WriteLine($"{cmd}: {ex.Message}");
            }
        }

        // ANCHOR: logging_interface
        class ConsoleLogger : ILogger
        {
            public void OnMessage(LogLevel level, string message)
            {
                // the message already carries the level and ends in a newline
                Console.Write(message);
            }
        }
        // ANCHOR_END: logging_interface

        class ClientStateListener : IClientStateListener
        {
            public void OnChange(ClientState state)
            {
                var text = state switch
                {
                    ClientState.Disabled => "disabled",
                    ClientState.Connecting => "connecting",
                    ClientState.Connected => "connected",
                    ClientState.WaitAfterFailedConnect => "connect failed, waiting to retry",
                    ClientState.WaitAfterDisconnect => "disconnected, waiting to reconnect",
                    ClientState.Shutdown => "shutdown",
                    _ => "unknown",
                };
                Console.WriteLine($"client state: {text}");
            }
        }

        class PortStateListener : IPortStateListener
        {
            public void OnChange(PortState state)
            {
                var text = state switch
                {
                    PortState.Disabled => "disabled",
                    PortState.Wait => "open failed, waiting to retry",
                    PortState.Open => "open",
                    PortState.Shutdown => "shutdown",
                    _ => "unknown",
                };
                Console.WriteLine($"port state: {text}");
            }
        }

        private static void PrintValues(string cmd, string name, ICollection<BitValue> values)
        {
            Console.WriteLine($"{cmd}: success");
            foreach (var value in values)
            {
                Console.WriteLine($"{name} {value.Index}: {(value.Value ? "true" : "false")}");
            }
        }

        private static void PrintValues(string cmd, string name, ICollection<RegisterValue> values)
        {
            Console.WriteLine($"{cmd}: success");
            foreach (var value in values)
            {
                Console.WriteLine($"{name} {value.Index}: {value.Value}");
            }
        }

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
