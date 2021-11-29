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
            var channel = Channel.CreateTlsClient(runtime, "127.0.0.1:802", 100, new RetryStrategy(), tlsConfig, decodeLevel);
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

        private static async Task RunChannel(Channel channel)
        {
            // ANCHOR: request_param
            var param = new RequestParam(1, 1000);
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
                            var result = await channel.ReadCoils(param, range);
                            // ANCHOR_END: read_coils
                            HandleBitResult(result);
                            break;
                        }
                    case "rdi":
                        {
                            var result = await channel.ReadDiscreteInputs(param, range);
                            HandleBitResult(result);
                            break;
                        }
                    case "rhr":
                        {
                            var result = await channel.ReadHoldingRegisters(param, range);
                            HandleRegisterResult(result);
                            break;
                        }
                    case "rir":
                        {
                            var result = await channel.ReadInputRegisters(param, range);
                            HandleRegisterResult(result);
                            break;
                        }
                    case "wsc":
                        {
                            /// ANCHOR: write_single_coil
                            var result = await channel.WriteSingleCoil(param, new Bit(0, true));
                            /// ANCHOR_END: write_single_coil
                            HandleWriteResult(result);
                            break;
                        }
                    case "wsr":
                        {
                            var result = await channel.WriteSingleRegister(param, new Register(0, 76));
                            HandleWriteResult(result);
                            break;
                        }
                    case "wmc":
                        {
                            var result = await channel.WriteMultipleCoils(param, 0, new List<bool>() { true, false });
                            HandleWriteResult(result);
                            break;
                        }
                    case "wmr":
                        {
                            // ANCHOR: write_multiple_registers
                            var result = await channel.WriteMultipleRegisters(param, 0, new List<ushort>() { 0xCA, 0xFE });
                            // ANCHOR_END: write_multiple_registers
                            HandleWriteResult(result);
                            break;
                        }
                    default:
                        Console.WriteLine("Unknown command");
                        break;
                }
            }
        }

        private static void HandleBitResult(BitReadResult result)
        {
            // ANCHOR: handle_bit_result
            if (result.Result.Summary == Status.Ok)
            {
                Console.WriteLine("success!");
                foreach (var bit in result.Iterator)
                {
                    Console.WriteLine($"index: {bit.Index} value: {bit.Value}");
                }
            }
            else if (result.Result.Summary == Status.Exception)
            {
                Console.WriteLine($"Modbus exception: {result.Result.Exception}");
            }
            else
            {
                Console.WriteLine($"error: {result.Result.Summary}");
            }
            // ANCHOR_END: handle_bit_result
        }

        private static void HandleRegisterResult(RegisterReadResult result)
        {
            // ANCHOR: error_handling
            if (result.Result.Summary == Status.Ok)
            {
                Console.WriteLine("success!");
                foreach (var bit in result.Iterator)
                {
                    Console.WriteLine($"index: {bit.Index} value: {bit.Value}");
                }
            }
            else if (result.Result.Summary == Status.Exception)
            {
                Console.WriteLine($"Modbus exception: {result.Result.Exception}");
            }
            else
            {
                Console.WriteLine($"error: {result.Result.Summary}");
            }
            // ANCHOR_END: error_handling
        }

        private static void HandleWriteResult(ErrorInfo result)
        {
            /// ANCHOR: handle_write_result
            if (result.Summary == Status.Ok)
            {
                Console.WriteLine("success!");
            }
            else if (result.Summary == Status.Exception)
            {
                Console.WriteLine($"Modbus exception: {result.Exception}");
            }
            else
            {
                Console.WriteLine($"error: {result.Summary}");
            }
            /// ANCHOR_END: handle_write_result
        }

        private static Task<string> GetInputAsync()
        {
            return Task.Run(() => Console.ReadLine());
        }
    }
}
