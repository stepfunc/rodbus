// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

// Server example for Rodbus library
//
// usage: server <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]
//
// Run it from the repository root so the TLS certificate paths resolve.

#include "rodbus.hpp"

#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

constexpr const char *USAGE = "usage: server <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]";
constexpr uint8_t UNIT_ID = 1;
constexpr const char *TCP_ENDPOINT = "127.0.0.1:502";
constexpr const char *TLS_ENDPOINT = "127.0.0.1:802";
constexpr const char *SERIAL_PORT = "/dev/ttySIM1";
constexpr uint16_t MAX_SESSIONS = 10;
constexpr uint16_t POINT_COUNT = 10;

// host and port of an endpoint, which the bindings take separately
struct Endpoint {
    std::string host;
    uint16_t port;
};

rodbus::Server create_server(rodbus::Runtime &runtime, const std::string &transport, const std::string &argument);
rodbus::DeviceMap create_device_map();
rodbus::Server create_tcp_server(rodbus::Runtime &runtime, const Endpoint &endpoint);
rodbus::Server create_rtu_server(rodbus::Runtime &runtime, const std::string &path);
rodbus::Server create_tls_server(rodbus::Runtime &runtime, const Endpoint &endpoint, const rodbus::TlsServerConfig &tls_config);
rodbus::TlsServerConfig get_ca_chain_config();
rodbus::TlsServerConfig get_self_signed_config();
void run_server(rodbus::Server &server);
void print_help();
void run_command(rodbus::Server &server, const std::string &cmd);
std::unique_ptr<rodbus::Logger> create_logger();
std::unique_ptr<rodbus::WriteHandler> create_write_handler();
std::unique_ptr<rodbus::AuthorizationHandler> create_auth_handler();
Endpoint split_endpoint(const std::string &endpoint);
const char *default_argument(const std::string &transport);
void print_usage();

int main(int argc, char *argv[])
{
    // the transport, then optionally the endpoint or serial port
    const char *default_arg = (argc == 2 || argc == 3) ? default_argument(argv[1]) : nullptr;
    if (!default_arg) {
        print_usage();
        return EXIT_FAILURE;
    }
    const std::string transport = argv[1];
    const std::string argument = (argc == 3) ? argv[2] : default_arg;

    try {
        // initialize logging with the default configuration
        rodbus::Logging::configure(rodbus::LoggingConfig(), create_logger());

        auto runtime_config = rodbus::RuntimeConfig();
        runtime_config.num_core_threads = 4;
        auto runtime = rodbus::Runtime(runtime_config);

        // destroyed before the runtime, in reverse order of declaration
        auto server = create_server(runtime, transport, argument);
        run_server(server);
    }
    catch (const std::exception &ex) {
        std::cout << std::string("error: ") + ex.what() + "\n";
        return EXIT_FAILURE;
    }

    return EXIT_SUCCESS;
}

rodbus::Server create_server(rodbus::Runtime &runtime, const std::string &transport, const std::string &argument)
{
    if (transport == "tcp") {
        return create_tcp_server(runtime, split_endpoint(argument));
    }
    else if (transport == "rtu") {
        return create_rtu_server(runtime, argument);
    }
    else if (transport == "tls-ca") {
        return create_tls_server(runtime, split_endpoint(argument), get_ca_chain_config());
    }
    else {
        // main only accepts known transports, so this is tls-self-signed
        return create_tls_server(runtime, split_endpoint(argument), get_self_signed_config());
    }
}

rodbus::DeviceMap create_device_map()
{
    // ANCHOR: device_map_init
    // POINT_COUNT points of each type, starting at index 0
    auto init_transaction = rodbus::functional::database_callback([](rodbus::Database &db) {
        for (uint16_t i = 0; i < POINT_COUNT; ++i) {
            db.add_coil(i, false);
            db.add_discrete_input(i, false);
            db.add_holding_register(i, 0);
            db.add_input_register(i, 0);
        }
    });
    auto map = rodbus::DeviceMap();
    map.add_endpoint(UNIT_ID, create_write_handler(), init_transaction);
    // ANCHOR_END: device_map_init

    return map;
}

rodbus::Server create_tcp_server(rodbus::Runtime &runtime, const Endpoint &endpoint)
{
    auto map = create_device_map();

    // ANCHOR: tcp_server_create
    auto filter = rodbus::AddressFilter::any();
    auto server = rodbus::Server::create_tcp(
        runtime,
        endpoint.host,
        endpoint.port,
        filter,
        MAX_SESSIONS,
        map,
        rodbus::DecodeLevel(rodbus::AppDecodeLevel::data_headers, rodbus::FrameDecodeLevel::nothing, rodbus::PhysDecodeLevel::nothing)
    );
    // ANCHOR_END: tcp_server_create

    return server;
}

rodbus::Server create_rtu_server(rodbus::Runtime &runtime, const std::string &path)
{
    auto map = create_device_map();

    // ANCHOR: rtu_server_create
    auto server = rodbus::Server::create_rtu(
        runtime,
        path,
        rodbus::SerialPortSettings(),
        rodbus::RetryStrategy(),
        map,
        rodbus::DecodeLevel(rodbus::AppDecodeLevel::data_headers, rodbus::FrameDecodeLevel::nothing, rodbus::PhysDecodeLevel::nothing)
    );
    // ANCHOR_END: rtu_server_create

    return server;
}

rodbus::Server create_tls_server(rodbus::Runtime &runtime, const Endpoint &endpoint, const rodbus::TlsServerConfig &tls_config)
{
    auto map = create_device_map();

    // ANCHOR: tls_server_create
    auto filter = rodbus::AddressFilter::any();
    auto server = rodbus::Server::create_tls_with_authz(
        runtime,
        endpoint.host,
        endpoint.port,
        filter,
        MAX_SESSIONS,
        map,
        tls_config,
        create_auth_handler(),
        rodbus::DecodeLevel(rodbus::AppDecodeLevel::data_headers, rodbus::FrameDecodeLevel::nothing, rodbus::PhysDecodeLevel::nothing)
    );
    // ANCHOR_END: tls_server_create

    return server;
}

rodbus::TlsServerConfig get_ca_chain_config()
{
    // ANCHOR: tls_ca_chain_config
    auto tls_config = rodbus::TlsServerConfig(
        "./certs/ca_chain/ca_cert.pem",     // CA certificate
        "./certs/ca_chain/server_cert.pem", // server certificate
        "./certs/ca_chain/server_key.pem",  // server private key
        ""                                  // no password
    );
    tls_config.min_tls_version = rodbus::MinTlsVersion::v12;
    // ANCHOR_END: tls_ca_chain_config

    return tls_config;
}

rodbus::TlsServerConfig get_self_signed_config()
{
    // ANCHOR: tls_self_signed_config
    auto tls_config = rodbus::TlsServerConfig(
        "./certs/self_signed/entity1_cert.pem", // client certificate
        "./certs/self_signed/entity2_cert.pem", // server certificate
        "./certs/self_signed/entity2_key.pem",  // server private key
        ""                                      // no password
    );
    tls_config.min_tls_version = rodbus::MinTlsVersion::v12;
    tls_config.certificate_mode = rodbus::CertificateMode::self_signed;
    // ANCHOR_END: tls_self_signed_config

    return tls_config;
}

// run the console until "x" or the end of input
void run_server(rodbus::Server &server)
{
    print_help();

    std::string cmd;
    while (std::getline(std::cin, cmd)) {
        if (!cmd.empty() && cmd.back() == '\r') {
            cmd.pop_back();
        }

        if (cmd == "x") {
            return;
        }
        else if (cmd == "help") {
            print_help();
        }
        else {
            try {
                run_command(server, cmd);
            }
            catch (const std::exception &ex) {
                std::cout << cmd + ": " + ex.what() + "\n";
            }
        }
    }
}

void print_help()
{
    // a single write, so a callback's output can't land in the middle of the list
    std::cout << "Commands:\n"
                 "  help - print this list\n"
                 "  x - exit\n"
                 "  ed - enable protocol decoding\n"
                 "  dd - disable protocol decoding\n"
                 "  uc - toggle every coil\n"
                 "  udi - toggle every discrete input\n"
                 "  uhr - increment every holding register\n"
                 "  uir - increment every input register\n";
}

// the update transactions run while the database is locked, so client writes in between
// aren't lost; every point below POINT_COUNT exists, so the getters don't throw
void run_command(rodbus::Server &server, const std::string &cmd)
{
    // the decode level commands only queue the change
    if (cmd == "ed") {
        server.set_decode_level(rodbus::DecodeLevel(rodbus::AppDecodeLevel::data_values, rodbus::FrameDecodeLevel::header, rodbus::PhysDecodeLevel::length));
        std::cout << cmd + ": requested\n";
    }
    else if (cmd == "dd") {
        server.set_decode_level(rodbus::DecodeLevel::nothing());
        std::cout << cmd + ": requested\n";
    }
    else if (cmd == "uc") {
        // ANCHOR: update_coil
        auto transaction = rodbus::functional::database_callback([](rodbus::Database &db) {
            for (uint16_t i = 0; i < POINT_COUNT; ++i) {
                db.update_coil(i, !db.get_coil(i));
            }
        });
        server.update_database(UNIT_ID, transaction);
        // ANCHOR_END: update_coil
        std::cout << cmd + ": success\n";
    }
    else if (cmd == "udi") {
        auto transaction = rodbus::functional::database_callback([](rodbus::Database &db) {
            for (uint16_t i = 0; i < POINT_COUNT; ++i) {
                db.update_discrete_input(i, !db.get_discrete_input(i));
            }
        });
        server.update_database(UNIT_ID, transaction);
        std::cout << cmd + ": success\n";
    }
    else if (cmd == "uhr") {
        auto transaction = rodbus::functional::database_callback([](rodbus::Database &db) {
            for (uint16_t i = 0; i < POINT_COUNT; ++i) {
                db.update_holding_register(i, static_cast<uint16_t>(db.get_holding_register(i) + 1)); // 65535 wraps to 0
            }
        });
        server.update_database(UNIT_ID, transaction);
        std::cout << cmd + ": success\n";
    }
    else if (cmd == "uir") {
        auto transaction = rodbus::functional::database_callback([](rodbus::Database &db) {
            for (uint16_t i = 0; i < POINT_COUNT; ++i) {
                db.update_input_register(i, static_cast<uint16_t>(db.get_input_register(i) + 1)); // 65535 wraps to 0
            }
        });
        server.update_database(UNIT_ID, transaction);
        std::cout << cmd + ": success\n";
    }
    else {
        std::cout << cmd + ": unknown command, type help for a list\n";
    }
}

// receives the log messages: each one already ends in a newline
class Logger : public rodbus::Logger {
    void on_message(rodbus::LogLevel level, const char *message) override
    {
        std::cout << message;
    }
};

std::unique_ptr<rodbus::Logger> create_logger()
{
    return std::make_unique<Logger>();
}

// ANCHOR: write_handler
class WriteHandler : public rodbus::WriteHandler {
    rodbus::WriteResult write_single_coil(uint16_t index, bool value, rodbus::Database &db) override
    {
        if (!db.update_coil(index, value)) {
            return rodbus::WriteResult::exception_init(rodbus::ModbusException::illegal_data_address);
        }
        std::cout << "write coil " + std::to_string(index) + ": " + (value ? "true" : "false") + "\n";
        return rodbus::WriteResult::success_init();
    }

    rodbus::WriteResult write_single_register(uint16_t index, uint16_t value, rodbus::Database &db) override
    {
        if (!db.update_holding_register(index, value)) {
            return rodbus::WriteResult::exception_init(rodbus::ModbusException::illegal_data_address);
        }
        std::cout << "write holding register " + std::to_string(index) + ": " + std::to_string(value) + "\n";
        return rodbus::WriteResult::success_init();
    }

    rodbus::WriteResult write_multiple_coils(uint16_t start, rodbus::BitValueIterator &it, rodbus::Database &db) override
    {
        // buffer the values, checking every coil exists, before changing any
        std::vector<rodbus::BitValue> values;
        try {
            while (it.next()) {
                const auto value = it.get();
                db.get_coil(value.index); // throws if the coil doesn't exist
                values.push_back(value);
            }
        }
        catch (const std::exception &) { // a missing point, or no memory for the buffer
            return rodbus::WriteResult::exception_init(rodbus::ModbusException::illegal_data_address);
        }

        for (const auto &value : values) {
            db.update_coil(value.index, value.value);
            std::cout << "write coil " + std::to_string(value.index) + ": " + (value.value ? "true" : "false") + "\n";
        }
        return rodbus::WriteResult::success_init();
    }

    rodbus::WriteResult write_multiple_registers(uint16_t start, rodbus::RegisterValueIterator &it, rodbus::Database &db) override
    {
        // buffer the values, checking every register exists, before changing any
        std::vector<rodbus::RegisterValue> values;
        try {
            while (it.next()) {
                const auto value = it.get();
                db.get_holding_register(value.index); // throws if the register doesn't exist
                values.push_back(value);
            }
        }
        catch (const std::exception &) { // a missing point, or no memory for the buffer
            return rodbus::WriteResult::exception_init(rodbus::ModbusException::illegal_data_address);
        }

        for (const auto &value : values) {
            db.update_holding_register(value.index, value.value);
            std::cout << "write holding register " + std::to_string(value.index) + ": " + std::to_string(value.value) + "\n";
        }
        return rodbus::WriteResult::success_init();
    }
};

std::unique_ptr<rodbus::WriteHandler> create_write_handler()
{
    return std::make_unique<WriteHandler>();
}
// ANCHOR_END: write_handler

// ANCHOR: auth_handler
// allows any role to read, and only the "operator" role to write
class AuthorizationHandler : public rodbus::AuthorizationHandler {
    rodbus::Authorization read_coils(uint8_t unit_id, const rodbus::AddressRange &range, const char *role) override
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

    rodbus::Authorization write_single_coil(uint8_t unit_id, uint16_t index, const char *role) override
    {
        return authorize_write(unit_id, role);
    }

    rodbus::Authorization write_single_register(uint8_t unit_id, uint16_t index, const char *role) override
    {
        return authorize_write(unit_id, role);
    }

    rodbus::Authorization write_multiple_coils(uint8_t unit_id, const rodbus::AddressRange &range, const char *role) override
    {
        return authorize_write(unit_id, role);
    }

    rodbus::Authorization write_multiple_registers(uint8_t unit_id, const rodbus::AddressRange &range, const char *role) override
    {
        return authorize_write(unit_id, role);
    }

    static rodbus::Authorization authorize_write(uint8_t unit_id, const char *role)
    {
        if (std::strcmp(role, "operator") == 0) {
            return rodbus::Authorization::allow;
        }
        std::cout << "denied write to unit " + std::to_string(unit_id) + " for role '" + role + "'\n";
        return rodbus::Authorization::deny;
    }
};

std::unique_ptr<rodbus::AuthorizationHandler> create_auth_handler()
{
    return std::make_unique<AuthorizationHandler>();
}
// ANCHOR_END: auth_handler

// split "host:port" at the last ':', since the bindings take the host and port separately
Endpoint split_endpoint(const std::string &endpoint)
{
    const auto colon = endpoint.rfind(':');
    const auto port = (colon == std::string::npos) ? std::string() : endpoint.substr(colon + 1);
    if (port.empty() || port.size() > 5 || port.find_first_not_of("0123456789") != std::string::npos || std::stoul(port) > UINT16_MAX) {
        throw std::invalid_argument("invalid endpoint: " + endpoint);
    }
    return Endpoint{endpoint.substr(0, colon), static_cast<uint16_t>(std::stoul(port))};
}

// the default endpoint or serial port of a transport, or nullptr if the transport is unknown
const char *default_argument(const std::string &transport)
{
    if (transport == "tcp") {
        return TCP_ENDPOINT;
    }
    else if (transport == "rtu") {
        return SERIAL_PORT;
    }
    else if (transport == "tls-ca" || transport == "tls-self-signed") {
        return TLS_ENDPOINT;
    }
    else {
        return nullptr;
    }
}

void print_usage()
{
    std::cerr << std::string(USAGE) + "\n";
}
