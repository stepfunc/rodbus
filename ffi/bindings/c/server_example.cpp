#include "rodbus.hpp"

#include <chrono>
#include <cstring>
#include <iostream>
#include <string>

class Logger : public rodbus::Logger {
    void on_message(rodbus::LogLevel level, const char* message) override
    {
        std::cout << message;
    }
};

// ANCHOR: write_handler
class WriterHandler : public rodbus::WriteHandler
{
    rodbus::WriteResult write_single_coil(uint16_t index, bool value, rodbus::Database& db) override
    {
        if(db.update_coil(index, value))
        {
            return rodbus::WriteResult::success_init();
        }
        else
        {
            return rodbus::WriteResult::exception_init(rodbus::ModbusException::illegal_data_address);
        }
    }

    rodbus::WriteResult write_single_register(uint16_t index, uint16_t value, rodbus::Database& db) override
    {
        if(db.update_holding_register(index, value))
        {
            return rodbus::WriteResult::success_init();
        }
        else
        {
            return rodbus::WriteResult::exception_init(rodbus::ModbusException::illegal_data_address);
        }
    }

    rodbus::WriteResult write_multiple_coils(uint16_t index, rodbus::BitValueIterator& it, rodbus::Database& db) override
    {
        auto result = rodbus::WriteResult::success_init();
        while (it.next())
        {
            const auto bit = it.get();
            if (!db.update_coil(bit.index, bit.value))
            {
                result = rodbus::WriteResult::exception_init(rodbus::ModbusException::illegal_data_address);
            }
        }
        return result;
    }

    rodbus::WriteResult write_multiple_registers(uint16_t index, rodbus::RegisterValueIterator& it, rodbus::Database& db) override
    {
        auto result = rodbus::WriteResult::success_init();
        while (it.next())
        {
            const auto register_value = it.get();
            if (!db.update_holding_register(register_value.index, register_value.value))
            {
                result = rodbus::WriteResult::exception_init(rodbus::ModbusException::illegal_data_address);
            }
        }
        return result;
    }
};
// ANCHOR_END: write_handler

// ANCHOR: auth_handler
class AuthorizationHandler : public rodbus::AuthorizationHandler
{
    rodbus::Authorization read_coils(uint8_t unit_id, const rodbus::AddressRange& range, const char* role) override
    {
        return rodbus::Authorization::allow;
    }

    rodbus::Authorization read_discrete_inputs(uint8_t unit_id, const rodbus::AddressRange &range, const char *role) override
    {
        return rodbus::Authorization::allow;
    }

    rodbus::Authorization read_holding_registers(uint8_t unit_id, const rodbus::AddressRange &range, const char *role) override
    {
        return rodbus::Authorization::allow;
    }

    rodbus::Authorization read_input_registers(uint8_t unit_id, const rodbus::AddressRange &range, const char *role) override
    {
        return rodbus::Authorization::allow;
    }

    rodbus::Authorization write_single_coil(uint8_t unit_id, uint16_t idx, const char *role) override
    {
        return rodbus::Authorization::deny;
    }

    rodbus::Authorization write_single_register(uint8_t unit_id, uint16_t idx, const char *role) override
    {
        return rodbus::Authorization::deny;
    }

    rodbus::Authorization write_multiple_coils(uint8_t unit_id, const rodbus::AddressRange &range, const char *role) override
    {
        return rodbus::Authorization::deny;
    }

    rodbus::Authorization write_multiple_registers(uint8_t unit_id, const rodbus::AddressRange &range, const char *role) override
    {
        return rodbus::Authorization::deny;
    }
};
// ANCHOR_END: auth_handler

int run_server(rodbus::Server& server)
{
    // state passed to the update callbacks
    auto coil_value = false;
    auto discrete_input_value = false;
    auto holding_register_value = 0;
    auto input_register_value = 0;

    while (true) {
        std::string cmd;
        std::getline(std::cin, cmd);

        if (cmd == "x") {
            return 0;
        }
        else if (cmd == "ed") {
            // enable decoding
            server.set_decode_level(
                rodbus::DecodeLevel(rodbus::AppDecodeLevel::data_values, rodbus::FrameDecodeLevel::header, rodbus::PhysDecodeLevel::length));
        }
        else if (cmd == "dd") {
            // disable decoding
            server.set_decode_level(rodbus::DecodeLevel::nothing());
        }
        else if (cmd == "uc") {
            // ANCHOR: update_coil
            auto transaction = rodbus::functional::database_callback([&](rodbus::Database& db) {
                coil_value = !coil_value;

                for (uint16_t i = 0; i < 10; ++i) {
                    db.update_coil(i, coil_value);
                }
            });
            server.update_database(1, transaction);
            // ANCHOR_END: update_coil
        }
        else if (cmd == "udi") {
            auto transaction = rodbus::functional::database_callback([&](rodbus::Database& db) {
                discrete_input_value = !discrete_input_value;

                for (uint16_t i = 0; i < 10; ++i) {
                    db.update_discrete_input(i, discrete_input_value);
                }
            });
            server.update_database(1, transaction);
        }
        else if (cmd == "uhr") {
            auto transaction = rodbus::functional::database_callback([&](rodbus::Database& db) {
                ++holding_register_value;

                for (uint16_t i = 0; i < 10; ++i) {
                    db.update_holding_register(i, holding_register_value);
                }
            });
            server.update_database(1, transaction);
        }
        else if (cmd == "uir") {
            auto transaction = rodbus::functional::database_callback([&](rodbus::Database& db) {
                ++input_register_value;

                for (uint16_t i = 0; i < 10; ++i) {
                    db.update_input_register(i, input_register_value);
                }
            });
            server.update_database(1, transaction);
        }
        else {
            std::cout << "unknown command: " << cmd << std::endl;
        }
    }
}

rodbus::DeviceMap create_device_map()
{
    // create the device map
    // ANCHOR: device_map_init
    auto device_map = rodbus::DeviceMap();
    auto init_transaction = rodbus::functional::database_callback([](rodbus::Database& db) {
        for (uint16_t i = 0; i < 10; ++i)
        {
            db.add_coil(i, false);
            db.add_discrete_input(i, false);
            db.add_holding_register(i, 0);
            db.add_input_register(i, 0);
        }
    });
    device_map.add_endpoint(1, std::make_unique<WriterHandler>(), init_transaction);
    // ANCHOR_END: device_map_init

    return device_map;
}

int run_tcp_server(rodbus::Runtime& runtime)
{
    auto device_map = create_device_map();

    // ANCHOR: tcp_server_create
    auto server = rodbus::Server::create_tcp(runtime, "127.0.0.1", 502, rodbus::AddressFilter::any(), 100, device_map, rodbus::DecodeLevel::nothing());
    // ANCHOR_END: tcp_server_create

    return run_server(server);
}

int run_rtu_server(rodbus::Runtime& runtime)
{
    auto device_map = create_device_map();

    // ANCHOR: rtu_server_create
    auto server = rodbus::Server::create_rtu(runtime, "/dev/ttySIM1", rodbus::SerialPortSettings(), device_map, rodbus::DecodeLevel::nothing());
    // ANCHOR_END: rtu_server_create

    return run_server(server);
}

int run_tls_server(rodbus::Runtime& runtime, const rodbus::TlsServerConfig& tls_config)
{
    auto device_map = create_device_map();

    // ANCHOR: tls_server_create
    auto server = rodbus::Server::create_tls_with_authz(runtime, "127.0.0.1", 802, rodbus::AddressFilter::any(), 100, device_map, tls_config, std::make_unique<AuthorizationHandler>(), rodbus::DecodeLevel::nothing());
    // ANCHOR_END: tls_server_create

    return run_server(server);
}

rodbus::TlsServerConfig get_tls_ca_config()
{
    // ANCHOR: tls_ca_chain_config
    auto tls_config = rodbus::TlsServerConfig(
        "./certs/ca_chain/ca_cert.pem",
        "./certs/ca_chain/server_cert.pem",
        "./certs/ca_chain/server_key.pem",
        "" // no password
    );
    // ANCHOR_END: tls_ca_chain_config

    return tls_config;
}

rodbus::TlsServerConfig get_tls_self_signed_config()
{
    // ANCHOR: tls_self_signed_config
    auto tls_config = rodbus::TlsServerConfig(
        "./certs/self_signed/entity1_cert.pem",
        "./certs/self_signed/entity2_cert.pem",
        "./certs/self_signed/entity2_key.pem",
        "" // no password
    );
    tls_config.certificate_mode = rodbus::CertificateMode::self_signed;
    // ANCHOR_END: tls_self_signed_config

    return tls_config;
}

int main(int argc, char* argv[])
{
    // initialize logging with the default configuration
    rodbus::Logging::configure(rodbus::LoggingConfig(), std::make_unique<Logger>());

    // initialize the runtime
    auto runtime_config = rodbus::RuntimeConfig();
    runtime_config.num_core_threads = 4;
    auto runtime = rodbus::Runtime(runtime_config);

    if (argc != 2) {
        std::cout << "you must specify a transport type" << std::endl;
        std::cout << "usage: cpp_server_example <channel> (tcp, rtu, tls-ca, tls-self-signed)" << std::endl;
        return -1;
    }

    const auto type = argv[1];

    if (strcmp(type, "tcp") == 0) {
        return run_tcp_server(runtime);
    }
    else if (strcmp(type, "rtu") == 0) {
        return run_rtu_server(runtime);
    }
    else if (strcmp(type, "tls-ca") == 0) {
        return run_tls_server(runtime, get_tls_ca_config());
    }
    else if (strcmp(type, "tls-self-signed") == 0) {
        return run_tls_server(runtime, get_tls_self_signed_config());
    }
    else {
        std::cout << "unknown channel type: " << type << std::endl;
        return -1;
    }
}
