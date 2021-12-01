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

int main()
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

    // ANCHOR: tls_self_signed_config
    auto self_signed_tls_config = rodbus::TlsClientConfig(
        "test.com",
        "./certs/self_signed/entity2_cert.pem",
        "./certs/self_signed/entity1_cert.pem",
        "./certs/self_signed/entity1_key.pem",
        "" // no password
    );
    self_signed_tls_config.certificate_mode = rodbus::CertificateMode::self_signed;
    // ANCHOR_END: tls_self_signed_config

    // ANCHOR: tls_ca_chain_config
    auto ca_chain_tls_config = rodbus::TlsClientConfig(
        "test.com",
        "./certs/ca_chain/ca_cert.pem",
        "./certs/ca_chain/entity1_cert.pem",
        "./certs/ca_chain/entity1_key.pem",
        "" // no password
    );
    // ANCHOR_END: tls_ca_chain_config

    const auto tls_config = ca_chain_tls_config;

    // initialize a Modbus TCP client channel
    // ANCHOR: create_tcp_channel
    auto channel = rodbus::ClientChannel::create_tls(
        runtime,
        "127.0.0.1:802",
        100,
        rodbus::RetryStrategy(),
        tls_config,
        rodbus::DecodeLevel()
    );
    // ANCHOR_END: create_tcp_channel

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
            channel.read_coils(param, range, std::make_unique<BitReadCallback>());
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
            const auto register_value = rodbus::RegisterValue(0, 76);
            channel.write_single_register(param, register_value, std::make_unique<WriteCallback>());
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
