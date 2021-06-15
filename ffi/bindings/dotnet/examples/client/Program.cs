using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
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

        static void Main(string[] args)
        {
            // initialize logging with the default configuration
            Logging.Configure(
                new LoggingConfig(),
                new ConsoleLogger()
            );

            // initialize the runtime
            var runtime = new Runtime(new RuntimeConfig { NumCoreThreads = 4 });

            // initialize a Modbus TCP client channel
            var decodeLevel = new DecodeLevel();
            var channel = Channel.CreateTcpClient(runtime, "127.0.0.1:502", 100, decodeLevel);

            try
            {
                RunChannel(channel).GetAwaiter().GetResult();
            }
            finally
            {
                runtime.Shutdown();
            }
        }

        private static async Task RunChannel(Channel channel)
        {
            var param = new RequestParam(1, 1000);
            var range = new AddressRange(0, 5);

            while (true)
            {
                switch(await GetInputAsync())
                {
                    case "x":
                        return;
                    case "rc":
                        {
                            var result = await channel.ReadCoils(range, param);
                            HandleBitResult(result);
                            break;
                        }
                    case "rdi":
                        {
                            var result = await channel.ReadDiscreteInputs(range, param);
                            HandleBitResult(result);
                            break;
                        }
                    case "rhr":
                        {
                            var result = await channel.ReadHoldingRegisters(range, param);
                            HandleRegisterResult(result);
                            break;
                        }
                    case "rir":
                        {
                            var result = await channel.ReadInputRegisters(range, param);
                            HandleRegisterResult(result);
                            break;
                        }
                    case "wsc":
                        {
                            var result = await channel.WriteSingleCoil(new Bit(0, true), param);
                            HandleWriteResult(result);
                            break;
                        }
                    case "wsr":
                        {
                            var result = await channel.WriteSingleRegister(new Register(0, 76), param);
                            HandleWriteResult(result);
                            break;
                        }
                    case "wmc":
                        {
                            var result = await channel.WriteMultipleCoils(0, new List<bool>() { true, false }, param);
                            HandleWriteResult(result);
                            break;
                        }
                    case "wmr":
                        {
                            var result = await channel.WriteMultipleRegisters(0, new List<ushort>() { 0xCA, 0xFE }, param);
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
        }

        private static void HandleRegisterResult(RegisterReadResult result)
        {
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
        }

        private static void HandleWriteResult(ErrorInfo result)
        {
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
        }

        private static Task<string> GetInputAsync()
        {
            return Task.Run(() => Console.ReadLine());
        }
    }
}
