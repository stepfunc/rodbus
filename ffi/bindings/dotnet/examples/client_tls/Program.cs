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

            // ANCHOR: tls_self_signed_config
            var selfSignedTlsConfig = new TlsClientConfig(
                "test.com",
                "./certs/self_signed/ca_cert.pem",
                "./certs/self_signed/entity1_cert.pem",
                "./certs/self_signed/entity1_key.pem",
                "" // no password
            );
            selfSignedTlsConfig.CertificateMode = CertificateMode.SelfSigned;
            // ANCHOR_END: tls_self_signed_config

            // ANCHOR: tls_ca_chain_config
            var caChainTlsConfig = new TlsClientConfig(
                "test.com",
                "./certs/ca_chain/ca_cert.pem",
                "./certs/ca_chain/entity1_cert.pem",
                "./certs/ca_chain/entity1_key.pem",
                "" // no password
            );
            // ANCHOR_END: tls_ca_chain_config

            var tlsConfig = caChainTlsConfig;

            // initialize a Modbus TLS client channel
            // ANCHOR: create_tls_channel
            var decodeLevel = new DecodeLevel();
            var channel = ClientChannel.CreateTls(runtime, "127.0.0.1:802", 100, new RetryStrategy(), tlsConfig, decodeLevel);
            // ANCHOR_END: create_tls_channel

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
                switch (await GetInputAsync())
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
