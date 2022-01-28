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

            // initialize a Modbus client channel
            Channel channel = null;
            if (Array.IndexOf(args, "--serial") == -1)
            {
                // ANCHOR: create_tcp_channel
                var decodeLevel = new DecodeLevel();
                channel = Channel.CreateTcpClient(runtime, "127.0.0.1:502", 1, new RetryStrategy(), decodeLevel);
                // ANCHOR_END: create_tcp_channel
            }
            else
            {
                // ANCHOR: create_rtu_channel
                var decodeLevel = new DecodeLevel();
                channel = Channel.CreateRtuClient(
                    runtime, // runtime
                    "/dev/ttySIM0", // path
                    new SerialPortSettings(), // serial settings
                    1, // max queued requests
                    TimeSpan.FromSeconds(1), // retry delay
                    decodeLevel // decode level
                );
                // ANCHOR_END: create_rtu_channel
            }

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
