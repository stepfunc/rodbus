// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

// Client example for Rodbus library
//
// usage: client <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]
//
// Run it from the repository root so the TLS certificate paths resolve.

#include "rodbus.hpp"

#include <chrono>
#include <cstdint>
#include <cstdlib>
#include <exception>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

constexpr const char *USAGE = "usage: client <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]";
constexpr uint8_t UNIT_ID = 1;
constexpr const char *TCP_ENDPOINT = "127.0.0.1:502";
constexpr const char *TLS_ENDPOINT = "127.0.0.1:802";
constexpr const char *SERIAL_PORT = "/dev/ttySIM0";
constexpr uint16_t MAX_QUEUED_REQUESTS = 10;
constexpr std::chrono::seconds REQUEST_TIMEOUT(1);

// host and port of an endpoint, which the bindings take separately
struct Endpoint {
    std::string host;
    uint16_t port;
};

rodbus::ClientChannel create_channel(rodbus::Runtime &runtime, const std::string &transport, const std::string &argument);
rodbus::ClientChannel create_tcp_channel(rodbus::Runtime &runtime, const Endpoint &endpoint);
rodbus::ClientChannel create_rtu_channel(rodbus::Runtime &runtime, const std::string &path);
rodbus::ClientChannel create_tls_channel(rodbus::Runtime &runtime, const Endpoint &endpoint, const rodbus::TlsClientConfig &tls_config);
rodbus::TlsClientConfig get_ca_chain_config();
rodbus::TlsClientConfig get_self_signed_config();
void run_channel(rodbus::ClientChannel &channel);
void print_help();
void run_command(rodbus::ClientChannel &channel, const rodbus::RequestParam &param, const std::string &cmd);
std::unique_ptr<rodbus::Logger> create_logger();
std::unique_ptr<rodbus::ClientStateListener> create_client_state_listener();
std::unique_ptr<rodbus::PortStateListener> create_port_state_listener();
std::unique_ptr<rodbus::BitReadCallback> create_bit_read_callback(const char *cmd, const char *name);
std::unique_ptr<rodbus::RegisterReadCallback> create_register_read_callback(const char *cmd, const char *name);
std::unique_ptr<rodbus::WriteCallback> create_write_callback(const char *cmd);
const char *client_state_name(rodbus::ClientState state);
const char *port_state_name(rodbus::PortState state);
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
        // ANCHOR: logging_init
        // initialize logging with the default configuration
        rodbus::Logging::configure(rodbus::LoggingConfig(), create_logger());
        // ANCHOR_END: logging_init

        // ANCHOR: runtime_init
        auto runtime_config = rodbus::RuntimeConfig();
        runtime_config.num_core_threads = 4;
        auto runtime = rodbus::Runtime(runtime_config);
        // ANCHOR_END: runtime_init

        // destroyed before the runtime, in reverse order of declaration
        auto channel = create_channel(runtime, transport, argument);
        run_channel(channel);
    }
    catch (const std::exception &ex) {
        std::cout << std::string("error: ") + ex.what() + "\n";
        return EXIT_FAILURE;
    }

    return EXIT_SUCCESS;
}

rodbus::ClientChannel create_channel(rodbus::Runtime &runtime, const std::string &transport, const std::string &argument)
{
    if (transport == "tcp") {
        return create_tcp_channel(runtime, split_endpoint(argument));
    }
    else if (transport == "rtu") {
        return create_rtu_channel(runtime, argument);
    }
    else if (transport == "tls-ca") {
        return create_tls_channel(runtime, split_endpoint(argument), get_ca_chain_config());
    }
    else {
        // main only accepts known transports, so this is tls-self-signed
        return create_tls_channel(runtime, split_endpoint(argument), get_self_signed_config());
    }
}

rodbus::ClientChannel create_tcp_channel(rodbus::Runtime &runtime, const Endpoint &endpoint)
{
    // ANCHOR: create_tcp_channel
    auto channel = rodbus::ClientChannel::create_tcp(
        runtime,
        endpoint.host,
        endpoint.port,
        MAX_QUEUED_REQUESTS,
        rodbus::RetryStrategy(),
        rodbus::DecodeLevel(rodbus::AppDecodeLevel::data_headers, rodbus::FrameDecodeLevel::nothing, rodbus::PhysDecodeLevel::nothing),
        create_client_state_listener()
    );
    // ANCHOR_END: create_tcp_channel

    return channel;
}

rodbus::ClientChannel create_rtu_channel(rodbus::Runtime &runtime, const std::string &path)
{
    // ANCHOR: create_rtu_channel
    auto channel = rodbus::ClientChannel::create_rtu(
        runtime,
        path,
        rodbus::SerialPortSettings(),
        MAX_QUEUED_REQUESTS,
        rodbus::RetryStrategy(),
        rodbus::DecodeLevel(rodbus::AppDecodeLevel::data_headers, rodbus::FrameDecodeLevel::nothing, rodbus::PhysDecodeLevel::nothing),
        create_port_state_listener()
    );
    // ANCHOR_END: create_rtu_channel

    return channel;
}

rodbus::ClientChannel create_tls_channel(rodbus::Runtime &runtime, const Endpoint &endpoint, const rodbus::TlsClientConfig &tls_config)
{
    // ANCHOR: create_tls_channel
    auto channel = rodbus::ClientChannel::create_tls(
        runtime,
        endpoint.host,
        endpoint.port,
        MAX_QUEUED_REQUESTS,
        rodbus::RetryStrategy(),
        tls_config,
        rodbus::DecodeLevel(rodbus::AppDecodeLevel::data_headers, rodbus::FrameDecodeLevel::nothing, rodbus::PhysDecodeLevel::nothing),
        create_client_state_listener()
    );
    // ANCHOR_END: create_tls_channel

    return channel;
}

rodbus::TlsClientConfig get_ca_chain_config()
{
    // ANCHOR: tls_ca_chain_config
    auto tls_config = rodbus::TlsClientConfig(
        "test.com",                         // name expected in the server certificate
        "./certs/ca_chain/ca_cert.pem",     // CA certificate
        "./certs/ca_chain/client_cert.pem", // client certificate
        "./certs/ca_chain/client_key.pem",  // client private key
        ""                                  // no password
    );
    tls_config.min_tls_version = rodbus::MinTlsVersion::v12;
    // ANCHOR_END: tls_ca_chain_config

    return tls_config;
}

rodbus::TlsClientConfig get_self_signed_config()
{
    // ANCHOR: tls_self_signed_config
    auto tls_config = rodbus::TlsClientConfig(
        "test.com",                             // not checked in self-signed mode
        "./certs/self_signed/entity2_cert.pem", // server certificate
        "./certs/self_signed/entity1_cert.pem", // client certificate
        "./certs/self_signed/entity1_key.pem",  // client private key
        ""                                      // no password
    );
    tls_config.min_tls_version = rodbus::MinTlsVersion::v12;
    tls_config.certificate_mode = rodbus::CertificateMode::self_signed;
    // ANCHOR_END: tls_self_signed_config

    return tls_config;
}

// enable the channel and run the console until "x" or the end of input
void run_channel(rodbus::ClientChannel &channel)
{
    channel.enable();

    // ANCHOR: request_param
    const auto param = rodbus::RequestParam(UNIT_ID, REQUEST_TIMEOUT);
    // ANCHOR_END: request_param

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
                run_command(channel, param, cmd);
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
                 "  ec - enable the channel\n"
                 "  dc - disable the channel\n"
                 "  ed - enable protocol decoding\n"
                 "  dd - disable protocol decoding\n"
                 "  rc - read coils 0 to 4\n"
                 "  rdi - read discrete inputs 0 to 4\n"
                 "  rhr - read holding registers 0 to 4\n"
                 "  rir - read input registers 0 to 4\n"
                 "  wsc - write coil 0 = true\n"
                 "  wsr - write holding register 0 = 76\n"
                 "  wmc - write coils 0 and 1 = true, false\n"
                 "  wmr - write holding registers 0 and 1 = 0xCA, 0xFE\n";
}

// start one command: the request callbacks print the results
void run_command(rodbus::ClientChannel &channel, const rodbus::RequestParam &param, const std::string &cmd)
{
    // ANCHOR: address_range
    const auto range = rodbus::AddressRange(0, // start
                                            5  // count
    );
    // ANCHOR_END: address_range

    // the settings commands only queue the change; the listener shows the channel's state
    if (cmd == "ec") {
        channel.enable();
        std::cout << cmd + ": requested\n";
    }
    else if (cmd == "dc") {
        channel.disable();
        std::cout << cmd + ": requested\n";
    }
    else if (cmd == "ed") {
        channel.set_decode_level(rodbus::DecodeLevel(rodbus::AppDecodeLevel::data_values, rodbus::FrameDecodeLevel::header, rodbus::PhysDecodeLevel::length));
        std::cout << cmd + ": requested\n";
    }
    else if (cmd == "dd") {
        channel.set_decode_level(rodbus::DecodeLevel::nothing());
        std::cout << cmd + ": requested\n";
    }
    else if (cmd == "rc") {
        // ANCHOR: read_coils
        channel.read_coils(param, range, create_bit_read_callback("rc", "coil"));
        // ANCHOR_END: read_coils
    }
    else if (cmd == "rdi") {
        channel.read_discrete_inputs(param, range, create_bit_read_callback("rdi", "discrete input"));
    }
    else if (cmd == "rhr") {
        channel.read_holding_registers(param, range, create_register_read_callback("rhr", "holding register"));
    }
    else if (cmd == "rir") {
        channel.read_input_registers(param, range, create_register_read_callback("rir", "input register"));
    }
    else if (cmd == "wsc") {
        // ANCHOR: write_single_coil
        channel.write_single_coil(param, rodbus::BitValue(0, true), create_write_callback("wsc"));
        // ANCHOR_END: write_single_coil
    }
    else if (cmd == "wsr") {
        channel.write_single_register(param, rodbus::RegisterValue(0, 76), create_write_callback("wsr"));
    }
    else if (cmd == "wmc") {
        channel.write_multiple_coils(param, 0, std::vector<bool>{true, false}, create_write_callback("wmc"));
    }
    else if (cmd == "wmr") {
        // ANCHOR: write_multiple_registers
        channel.write_multiple_registers(param, 0, std::vector<uint16_t>{0xCA, 0xFE}, create_write_callback("wmr"));
        // ANCHOR_END: write_multiple_registers
    }
    else {
        std::cout << cmd + ": unknown command, type help for a list\n";
    }
}

// ANCHOR: logging_interface
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
// ANCHOR_END: logging_interface

class ClientStateListener : public rodbus::ClientStateListener {
    void on_change(rodbus::ClientState state) override
    {
        std::cout << std::string("client state: ") + client_state_name(state) + "\n";
    }
};

std::unique_ptr<rodbus::ClientStateListener> create_client_state_listener()
{
    return std::make_unique<ClientStateListener>();
}

class PortStateListener : public rodbus::PortStateListener {
    void on_change(rodbus::PortState state) override
    {
        std::cout << std::string("port state: ") + port_state_name(state) + "\n";
    }
};

std::unique_ptr<rodbus::PortStateListener> create_port_state_listener()
{
    return std::make_unique<PortStateListener>();
}

// ANCHOR: bit_read_callback
// prints the command's result, then each value under the name
class BitReadCallback : public rodbus::BitReadCallback {
public:
    BitReadCallback(const char *cmd, const char *name) : cmd(cmd), name(name) {}

    void on_complete(rodbus::BitValueIterator &it) override
    {
        std::cout << std::string(cmd) + ": success\n";
        while (it.next()) {
            const auto value = it.get();
            std::cout << std::string(name) + " " + std::to_string(value.index) + ": " + (value.value ? "true" : "false") + "\n";
        }
    }

    void on_failure(rodbus::RequestError error) override
    {
        std::cout << std::string(cmd) + ": " + rodbus::to_string(error) + "\n";
    }

private:
    const char *cmd;
    const char *name;
};

std::unique_ptr<rodbus::BitReadCallback> create_bit_read_callback(const char *cmd, const char *name)
{
    return std::make_unique<BitReadCallback>(cmd, name);
}
// ANCHOR_END: bit_read_callback

class RegisterReadCallback : public rodbus::RegisterReadCallback {
public:
    RegisterReadCallback(const char *cmd, const char *name) : cmd(cmd), name(name) {}

    void on_complete(rodbus::RegisterValueIterator &it) override
    {
        std::cout << std::string(cmd) + ": success\n";
        while (it.next()) {
            const auto value = it.get();
            std::cout << std::string(name) + " " + std::to_string(value.index) + ": " + std::to_string(value.value) + "\n";
        }
    }

    void on_failure(rodbus::RequestError error) override
    {
        std::cout << std::string(cmd) + ": " + rodbus::to_string(error) + "\n";
    }

private:
    const char *cmd;
    const char *name;
};

std::unique_ptr<rodbus::RegisterReadCallback> create_register_read_callback(const char *cmd, const char *name)
{
    return std::make_unique<RegisterReadCallback>(cmd, name);
}

// ANCHOR: write_callback
// prints "<cmd>: success" or "<cmd>: <error>"
class WriteCallback : public rodbus::WriteCallback {
public:
    explicit WriteCallback(const char *cmd) : cmd(cmd) {}

    void on_complete(rodbus::Nothing result) override
    {
        std::cout << std::string(cmd) + ": success\n";
    }

    void on_failure(rodbus::RequestError error) override
    {
        std::cout << std::string(cmd) + ": " + rodbus::to_string(error) + "\n";
    }

private:
    const char *cmd;
};

std::unique_ptr<rodbus::WriteCallback> create_write_callback(const char *cmd)
{
    return std::make_unique<WriteCallback>(cmd);
}
// ANCHOR_END: write_callback

const char *client_state_name(rodbus::ClientState state)
{
    switch (state) {
    case rodbus::ClientState::disabled:
        return "disabled";
    case rodbus::ClientState::connecting:
        return "connecting";
    case rodbus::ClientState::connected:
        return "connected";
    case rodbus::ClientState::wait_after_failed_connect:
        return "connect failed, waiting to retry";
    case rodbus::ClientState::wait_after_disconnect:
        return "disconnected, waiting to reconnect";
    case rodbus::ClientState::shutdown:
        return "shutdown";
    default:
        return "unknown";
    }
}

const char *port_state_name(rodbus::PortState state)
{
    switch (state) {
    case rodbus::PortState::disabled:
        return "disabled";
    case rodbus::PortState::wait:
        return "open failed, waiting to retry";
    case rodbus::PortState::open:
        return "open";
    case rodbus::PortState::shutdown:
        return "shutdown";
    default:
        return "unknown";
    }
}

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
