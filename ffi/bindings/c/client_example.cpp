#include "rodbus.hpp"

#include <chrono>
#include <cstring>
#include <iostream>
#include <string>

/// ANCHOR: logging_callback
class Logger : public rodbus::Logger {
    void on_message(rodbus::LogLevel level, std::string message) override
    {
        std::cout << message;
    }
};
/// ANCHOR_END: logging_callback

// ANCHOR: bit_read_callback
class BitReadCallback : public rodbus::BitReadCallback
{
    void on_complete(rodbus::BitValueIterator& it) override
    {
        std::cout << "success!" << std::endl;
        while (it.next()) {
            const auto value = it.get();
            std::cout << "index: " << value.index << "value: " << value.value << std::endl;
        }
    }
    void on_failure(rodbus::RequestError err) override
    {
        std::cout << "error: " << rodbus::to_string(err) << std::endl;
    }
};
// ANCHOR_END: bit_read_callback

class RegisterReadCallback : public rodbus::RegisterReadCallback
{
    void on_complete(rodbus::RegisterValueIterator& it) override
    {
        std::cout << "success!" << std::endl;
        while (it.next()) {
            const auto value = it.get();
            std::cout << "index: " << value.index << "value: " << value.value << std::endl;
        }
    }
    void on_failure(rodbus::RequestError err) override
    {
        std::cout << "error: " << rodbus::to_string(err) << std::endl;
    }
};

/// ANCHOR: write_callback
class WriteCallback : public rodbus::WriteCallback
{
    void on_complete(rodbus::Nothing result) override
    {
        std::cout << "success!" << std::endl;
    }
    void on_failure(rodbus::RequestError err) override
    {
        std::cout << "error: " << rodbus::to_string(err) << std::endl;
    }
};
/// ANCHOR_END: write_callback

int run_channel(rodbus::ClientChannel& channel)
{
    // request param that we will be reusing
    // ANCHOR: request_param
    const auto param = rodbus::RequestParam(1, // Unit ID
                                            std::chrono::seconds(1) // Timeout
    );
    // ANCHOR_END: request_param

    // address range that we will be reusing
    // ANCHOR: address_range
    const auto range = rodbus::AddressRange(0, // start
                                            5  // count
    );
    // ANCHOR_END: address_range

    while (true)
    {
        std::string cmd;
        std::getline(std::cin, cmd);

        if (cmd == "x") {
            return 0;
        }
        else if (cmd == "rc") {
            /// ANCHOR: read_coils
            channel.read_coils(param, range, std::make_unique<BitReadCallback>());
            /// ANCHOR_END: read_coils
        }
        else if (cmd == "rdi") {
            channel.read_discrete_inputs(param, range, std::make_unique<BitReadCallback>());
        }
        else if (cmd == "rhr") {
            channel.read_holding_registers(param, range, std::make_unique<RegisterReadCallback>());
        }
        else if (cmd == "rir") {
            channel.read_input_registers(param, range, std::make_unique<RegisterReadCallback>());
        }
        else if (cmd == "wsc") {
            const auto bit_value = rodbus::BitValue(0, true);
            channel.write_single_coil(param, bit_value, std::make_unique<WriteCallback>());
        }
        else if (cmd == "wsr") {
            /// ANCHOR: write_single_coil
            const auto register_value = rodbus::RegisterValue(0, 76);
            channel.write_single_register(param, register_value, std::make_unique<WriteCallback>());
            /// ANCHOR_END: write_single_coil
        }
        else if (cmd == "wmc") {
            // create the bitlist
            auto bit_list = std::vector<bool>();
            bit_list.emplace_back(true);
            bit_list.emplace_back(false);

            // send the request
            channel.write_multiple_coils(param, 0, bit_list, std::make_unique<WriteCallback>());
        }
        else if (cmd == "wmr") {
            // create the register list
            // ANCHOR: write_multiple_registers
            auto register_list = std::vector<uint16_t>();
            register_list.emplace_back(0xCA);
            register_list.emplace_back(0xFE);

            // send the request
            channel.write_multiple_registers(param, 0, register_list, std::make_unique<WriteCallback>());
            // ANCHOR_END: write_multiple_registers
        }
        else {
            std::cout << "unknown command: " << cmd << std::endl;
        }
    }
}

int run_tcp_channel(rodbus::Runtime& runtime)
{
    // ANCHOR: create_tcp_channel
    auto channel = rodbus::ClientChannel::create_tcp(
        runtime,
        "127.0.0.1:502",
        100,
        rodbus::RetryStrategy(),
        rodbus::DecodeLevel()
    );
    // ANCHOR_END: create_tcp_channel

    return run_channel(channel);
}

int run_rtu_channel(rodbus::Runtime& runtime)
{
    // ANCHOR: create_rtu_channel
    auto channel = rodbus::ClientChannel::create_rtu(
        runtime,
        "/dev/ttySIM0",
        rodbus::SerialPortSettings(),
        1,
        std::chrono::seconds(1),
        rodbus::DecodeLevel()
    );
    // ANCHOR_END: create_rtu_channel

    return run_channel(channel);
}

int run_tls_channel(rodbus::Runtime& runtime, const rodbus::TlsClientConfig& tls_config)
{
    // ANCHOR: create_tls_channel
    auto channel = rodbus::ClientChannel::create_tls(
        runtime,
        "127.0.0.1:802",
        100,
        rodbus::RetryStrategy(),
        tls_config,
        rodbus::DecodeLevel()
    );
    // ANCHOR_END: create_tls_channel

    return run_channel(channel);
}

rodbus::TlsClientConfig get_tls_ca_config()
{
    // ANCHOR: tls_ca_chain_config
    auto tls_config = rodbus::TlsClientConfig(
        "test.com",
        "./certs/ca_chain/ca_cert.pem",
        "./certs/ca_chain/entity1_cert.pem",
        "./certs/ca_chain/entity1_key.pem",
        "" // no password
    );
    // ANCHOR_END: tls_ca_chain_config

    return tls_config;
}

rodbus::TlsClientConfig get_tls_self_signed_config()
{
    // ANCHOR: tls_self_signed_config
    auto tls_config = rodbus::TlsClientConfig(
        "test.com",
        "./certs/self_signed/entity2_cert.pem",
        "./certs/self_signed/entity1_cert.pem",
        "./certs/self_signed/entity1_key.pem",
        "" // no password
    );
    tls_config.certificate_mode = rodbus::CertificateMode::self_signed;
    // ANCHOR_END: tls_self_signed_config

    return tls_config;
}

int main(int argc, char* argv[])
{
    // ANCHOR: logging_init
    // initialize logging with the default configuration
    rodbus::Logging::configure(rodbus::LoggingConfig(), std::make_unique<Logger>());
    // ANCHOR_END: logging_init

    // initialize the runtime
    // ANCHOR: runtime_create
    auto runtime_config = rodbus::RuntimeConfig();
    runtime_config.num_core_threads = 4;
    auto runtime = rodbus::Runtime(runtime_config);
    // ANCHOR_END: runtime_create

    if (argc != 2) {
        std::cout << "you must specify a transport type" << std::endl;
        std::cout << "usage: cpp_client_example <channel> (tcp, rtu, tls-ca, tls-self-signed)" << std::endl;
        return -1;
    }

    const auto type = argv[1];

    if (strcmp(type, "tcp") == 0) {
        return run_tcp_channel(runtime);
    }
    else if (strcmp(type, "rtu") == 0) {
        return run_rtu_channel(runtime);
    }
    else if (strcmp(type, "tls-ca") == 0) {
        return run_tls_channel(runtime, get_tls_ca_config());
    }
    else if (strcmp(type, "tls-self-signed") == 0) {
        return run_tls_channel(runtime, get_tls_self_signed_config());
    }
    else {
        std::cout << "unknown channel type: " << type << std::endl;
        return -1;
    }
}
