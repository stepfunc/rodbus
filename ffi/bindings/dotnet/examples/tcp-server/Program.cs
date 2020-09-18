using System;
using System.Collections.Generic;
using System.Threading;
using rodbus;

namespace example
{
    class Program
    {

        class WriteHandler : IWriteHandler
        {
            public WriteResult WriteMultipleCoils(ushort start, ICollection<Bit> it, Database database)
            {
                return new WriteResult { Success = false, Exception = rodbus.Exception.IllegalFunction, RawException = 0 };
            }

            public WriteResult WriteMultipleRegisters(ushort start, ICollection<Register> it, Database database)
            {
                return new WriteResult { Success = false, Exception = rodbus.Exception.IllegalFunction, RawException = 0 };
            }

            public WriteResult WriteSingleCoil(bool value, ushort index, Database database)
            {
                return new WriteResult { Success = false, Exception = rodbus.Exception.IllegalFunction, RawException = 0 };
            }

            public WriteResult WriteSingleRegister(ushort value, ushort index, Database database)
            {
                return new WriteResult { Success = false, Exception = rodbus.Exception.IllegalFunction, RawException = 0 };
            }
        }

        class DatabaseInitialization : IDatabaseCallback
        {
            public void Callback(Database database)
            {
                for(ushort i = 0; i < 10; ++i)
                {
                    database.AddCoil(i, false);
                    database.AddDiscreteInput(i, false);
                    database.AddHoldingRegister(i, 0);
                    database.AddInputRegister(i, 0);
                }                
            }
        }

        class DatabaseUpdate : IDatabaseCallback
        {
            readonly Action<Database> action;
            public DatabaseUpdate(Action<Database> action)
            {
                this.action = action;
            }

            public void Callback(Database database)
            {
                this.action.Invoke(database);
            }
        }

        static void Main(string[] args)
        {
            using (var runtime = new Runtime(new RuntimeConfig { NumCoreThreads = 1 }))
            {
                run(runtime);
            }
        }

        static void run(Runtime runtime)
        {
            var map = new DeviceMap();
            map.AddEndpoint(1, new WriteHandler(), new DatabaseInitialization());
            var server = Server.CreateTcpServer(runtime, "127.0.0.1:502", map);

            ushort registerValue = 0;
            bool bitValue = false;
            
            while(true)
            {
                server.Update(1, new DatabaseUpdate((db) => {
                    registerValue += 1;
                    bitValue = !bitValue;
                    for(ushort i = 0; i < 10; ++i){
                        db.UpdateDiscreteInput(i, bitValue);
                        db.UpdateInputRegister(i, registerValue);
                    }
                }));
                Thread.Sleep(1000);
            }
        }
    }
}
