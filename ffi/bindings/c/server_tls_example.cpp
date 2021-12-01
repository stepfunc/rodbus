#include "rodbus.hpp"

#include <chrono>
#include <iostream>
#include <string>

class Logger : public rodbus::Logger {
    void on_message(rodbus::LogLevel level, std::string message) override
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
    rodbus::AuthorizationResult read_coils(uint8_t unit_id, const rodbus::AddressRange& range, std::string role) override
    {
        return rodbus::AuthorizationResult::authorized;
    }

    rodbus::AuthorizationResult read_discrete_inputs(uint8_t unit_id, const rodbus::AddressRange& range, std::string role) override
    {
        return rodbus::AuthorizationResult::authorized;
    }

    rodbus::AuthorizationResult read_holding_registers(uint8_t unit_id, const rodbus::AddressRange& range, std::string role) override
    {
        return rodbus::AuthorizationResult::authorized;
    }

    rodbus::AuthorizationResult read_input_registers(uint8_t unit_id, const rodbus::AddressRange& range, std::string role) override
    {
        return rodbus::AuthorizationResult::authorized;
    }

    rodbus::AuthorizationResult write_single_coil(uint8_t unit_id, uint16_t idx, std::string role) override
    {
        return rodbus::AuthorizationResult::not_authorized;
    }

    rodbus::AuthorizationResult write_single_register(uint8_t unit_id, uint16_t idx, std::string role) override
    {
        return rodbus::AuthorizationResult::not_authorized;
    }

    rodbus::AuthorizationResult write_multiple_coils(uint8_t unit_id, const rodbus::AddressRange& range, std::string role) override
    {
        return rodbus::AuthorizationResult::not_authorized;
    }

    rodbus::AuthorizationResult write_multiple_registers(uint8_t unit_id, const rodbus::AddressRange& range, std::string role) override
    {
        return rodbus::AuthorizationResult::not_authorized;
    }
};
// ANCHOR_END: auth_handler

int main()
{
    // initialize logging with the default configuration
    rodbus::Logging::configure(rodbus::LoggingConfig(), std::make_unique<Logger>());

    // initialize the runtime
    auto runtime_config = rodbus::RuntimeConfig();
    runtime_config.num_core_threads = 4;
    auto runtime = rodbus::Runtime(runtime_config);

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

    // ANCHOR: tls_self_signed_config
    auto self_signed_tls_config = rodbus::TlsServerConfig(
        "./certs/self_signed/entity1_cert.pem",
        "./certs/self_signed/entity2_cert.pem",
        "./certs/self_signed/entity2_key.pem",
        "" // no password
    );
    self_signed_tls_config.certificate_mode = rodbus::CertificateMode::self_signed;
    // ANCHOR_END: tls_self_signed_config

    // ANCHOR: tls_ca_chain_config
    auto ca_chain_tls_config = rodbus::TlsServerConfig(
        "./certs/ca_chain/ca_cert.pem",
        "./certs/ca_chain/entity2_cert.pem",
        "./certs/ca_chain/entity2_key.pem",
        "" // no password
    );
    // ANCHOR_END: tls_ca_chain_config

    const auto tls_config = ca_chain_tls_config;

    // create the TLS server
    // ANCHOR: tls_server_create
    auto server = rodbus::Server::create_tls(runtime, "127.0.0.1:802", 100, device_map, tls_config, std::make_unique<AuthorizationHandler>(), rodbus::DecodeLevel());
    // ANCHOR_END: tls_server_create

    // state passed to the update callbacks
    auto coil_value = false;
    auto discrete_input_value = false;
    auto holding_register_value = 0;
    auto input_register_value = 0;

    char cbuf[10];
    while (true) {
        std::string cmd;
        std::getline(std::cin, cmd);

        if (cmd == "x") {
            return 0;
        }
        else if (cbuf == "uc") {
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
        else if (cbuf == "udi") {
            auto transaction = rodbus::functional::database_callback([&](rodbus::Database& db) {
                discrete_input_value = !discrete_input_value;

                for (uint16_t i = 0; i < 10; ++i) {
                    db.update_discrete_input(i, discrete_input_value);
                }
            });
            server.update_database(1, transaction);
        }
        else if (cbuf == "uhr") {
            auto transaction = rodbus::functional::database_callback([&](rodbus::Database& db) {
                ++holding_register_value;

                for (uint16_t i = 0; i < 10; ++i) {
                    db.update_holding_register(i, holding_register_value);
                }
            });
            server.update_database(1, transaction);
        }
        else if (cbuf == "uir") {
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
