using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using rodbus;

namespace example
{
    class Program
    {
        // ANCHOR: logging_interface
        class ConsoleLogger : ILogger
        {
            public void OnMessage(LogLevel level, string message)
            {
                Console.Write($"{level}: {message}");
            }
        }
        // ANCHOR_END: logging_interface

        static void Main(string[] args)
        {
            // ANCHOR: logging_init
            // initialize logging with the default configuration
            Logging.Configure(
                new LoggingConfig(),
                new ConsoleLogger()
            );
            // ANCHOR_END: logging_init

            // initialize the runtime
            // ANCHOR: runtime_init
            var runtime = new Runtime(new RuntimeConfig { NumCoreThreads = 4 });
            // ANCHOR_END: runtime_init

            if (args.Length != 1)
            {
                Console.WriteLine("you must specify a transport type");
                Console.WriteLine("usage: client_example <channel> (tcp, rtu, tls-ca, tls-self-signed)");
                Environment.Exit(-1);
            }

            // initialize a Modbus client channel
            var channel = CreateChannel(args[0], runtime);

            try
            {
                RunChannel(channel).GetAwaiter().GetResult();
            }
            finally
            {
                // ANCHOR: runtime_shutdown
                runtime.Shutdown();
                // ANCHOR_END: runtime_shutdown
            }
        }

        private static ClientChannel CreateChannel(string type, Runtime runtime)
        {
            switch (type)
            {
                case "tcp":
                    return CreateTcpChannel(runtime);
                case "rtu":
                    return CreateRtuChannel(runtime);
                case "tls-ca":
                    return CreateTlsChannel(runtime, GetCaTlsConfig());
                case "tls-self-signed":
                    return CreateTlsChannel(runtime, GetSelfSignedTlsConfig());
                default:
                    Console.WriteLine($"unknown channel type: {type}");
                    Environment.Exit(-1);
                    return null;
            }
        }

        private static ClientChannel CreateTcpChannel(Runtime runtime)
        {
            // ANCHOR: create_tcp_channel
            var decodeLevel = new DecodeLevel();
            var channel = ClientChannel.CreateTcp(runtime, "127.0.0.1:502", 1, new RetryStrategy(), decodeLevel);
            // ANCHOR_END: create_tcp_channel

            return channel;
        }

        private static ClientChannel CreateRtuChannel(Runtime runtime)
        {
            // ANCHOR: create_rtu_channel
            var decodeLevel = new DecodeLevel();
            var channel = ClientChannel.CreateRtu(
                runtime, // runtime
                "/dev/ttySIM0", // path
                new SerialPortSettings(), // serial settings
                1, // max queued requests
                TimeSpan.FromSeconds(1), // retry delay
                decodeLevel // decode level
            );
            // ANCHOR_END: create_rtu_channel

            return channel;
        }

        private static ClientChannel CreateTlsChannel(Runtime runtime, TlsClientConfig tlsConfig)
        {
            // ANCHOR: create_tls_channel
            var decodeLevel = new DecodeLevel();
            var channel = ClientChannel.CreateTls(runtime, "127.0.0.1:802", 100, new RetryStrategy(), tlsConfig, decodeLevel);
            // ANCHOR_END: create_tls_channel

            return channel;
        }

        private static TlsClientConfig GetCaTlsConfig()
        {
            // ANCHOR: tls_ca_chain_config
            var tlsConfig = new TlsClientConfig(
                "test.com",
                "./certs/self_signed/ca_cert.pem",
                "./certs/self_signed/entity1_cert.pem",
                "./certs/self_signed/entity1_key.pem",
                "" // no password
            );
            // ANCHOR_END: tls_ca_chain_config

            return tlsConfig;
        }

        private static TlsClientConfig GetSelfSignedTlsConfig()
        {
            // ANCHOR: tls_self_signed_config
            var tlsConfig = new TlsClientConfig(
                "test.com",
                "./certs/self_signed/ca_cert.pem",
                "./certs/self_signed/entity1_cert.pem",
                "./certs/self_signed/entity1_key.pem",
                "" // no password
            ).WithCertificateMode(CertificateMode.SelfSigned);
            // ANCHOR_END: tls_self_signed_config

            return tlsConfig;
        }

        private static async Task RunChannel(ClientChannel channel)
        {
            // ANCHOR: request_param
            var param = new RequestParam(1, TimeSpan.FromSeconds(1));
            // ANCHOR_END: request_param
            // ANCHOR: address_range
            var range = new AddressRange(0, 5);
            // ANCHOR_END: address_range

            while (true)
            {
                switch(await GetInputAsync())
                {
                    case "x":
                        return;
                    case "rc":
                        {
                            // ANCHOR: read_coils
                            try
                            {
                                var bits = await channel.ReadCoils(param, range);
                                Console.WriteLine("success!");
                                foreach (var bit in bits)
                                {
                                    Console.WriteLine($"index: {bit.Index} value: {bit.Value}");
                                }
                            }
                            catch (Exception ex)
                            {
                                Console.WriteLine($"error: {ex}");
                            }
                            // ANCHOR_END: read_coils
                            break;
                        }
                    case "rdi":
                        {
                            try
                            {
                                var bits = await channel.ReadDiscreteInputs(param, range);
                                Console.WriteLine("success!");
                                foreach (var bit in bits)
                                {
                                    Console.WriteLine($"index: {bit.Index} value: {bit.Value}");
                                }
                            }
                            catch (Exception ex)
                            {
                                Console.WriteLine($"error: {ex}");
                            }
                            break;
                        }
                    case "rhr":
                        {
                            try
                            {
                                var registers = await channel.ReadHoldingRegisters(param, range);
                                Console.WriteLine("success!");
                                foreach (var bit in registers)
                                {
                                    Console.WriteLine($"index: {bit.Index} value: {bit.Value}");
                                }
                            }
                            catch (Exception ex)
                            {
                                Console.WriteLine($"error: {ex}");
                            }
                            break;
                        }
                    case "rir":
                        {
                            try
                            {
                                var registers = await channel.ReadInputRegisters(param, range);
                                Console.WriteLine("success!");
                                foreach (var bit in registers)
                                {
                                    Console.WriteLine($"index: {bit.Index} value: {bit.Value}");
                                }
                            }
                            catch (Exception ex)
                            {
                                Console.WriteLine($"error: {ex}");
                            }
                            break;
                        }
                    case "wsc":
                        {
                            /// ANCHOR: write_single_coil
                            try
                            {
                                await channel.WriteSingleCoil(param, new BitValue(0, true));
                                Console.WriteLine("success!");
                            }
                            catch (Exception ex)
                            {
                                Console.WriteLine($"error: {ex}");
                            }
                            /// ANCHOR_END: write_single_coil
                            break;
                        }
                    case "wsr":
                        {
                            try
                            {
                                await channel.WriteSingleRegister(param, new RegisterValue(0, 76));
                                Console.WriteLine("success!");
                            }
                            catch (Exception ex)
                            {
                                Console.WriteLine($"error: {ex}");
                            }
                            break;
                        }
                    case "wmc":
                        {
                            try
                            {
                                await channel.WriteMultipleCoils(param, 0, new List<bool>() { true, false });
                                Console.WriteLine("success!");
                            }
                            catch (Exception ex)
                            {
                                Console.WriteLine($"error: {ex}");
                            }
                            break;
                        }
                    case "wmr":
                        {
                            // ANCHOR: write_multiple_registers
                            try
                            {
                                await channel.WriteMultipleRegisters(param, 0, new List<ushort>() { 0xCA, 0xFE });
                                Console.WriteLine("success!");
                            }
                            catch (Exception ex)
                            {
                                Console.WriteLine($"error: {ex}");
                            }
                            // ANCHOR_END: write_multiple_registers
                            break;
                        }
                    default:
                        Console.WriteLine("Unknown command");
                        break;
                }
            }
        }

        private static Task<string> GetInputAsync()
        {
            return Task.Run(() => Console.ReadLine());
        }
    }
}
