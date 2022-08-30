#include "rodbus.h"

#include <inttypes.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>

// ANCHOR: logging_callback
// callback which will receive log messages
void on_log_message(rodbus_log_level_t level, const char *message, void *ctx) { printf("%s\n", message); }
// ANCHOR_END: logging_callback

// ANCHOR: bit_read_callback
void on_read_bits_complete(rodbus_bit_value_iterator_t *bits, void *ctx)
{
    printf("success!\n");
    rodbus_bit_value_t *bit = NULL;
    while ((bit = rodbus_bit_value_iterator_next(bits))) {
        printf("index: %d value: %d\n", bit->index, bit->value);
    }
}

void on_read_bits_failure(rodbus_request_error_t error, void *ctx)
{
    printf("error: %s\n", rodbus_request_error_to_string(error));
}
// ANCHOR_END: bit_read_callback

void on_read_registers_complete(rodbus_register_value_iterator_t *registers, void *ctx)
{
    printf("success!\n");
    rodbus_register_value_t *reg = NULL;
    while ((reg = rodbus_register_value_iterator_next(registers))) {
        printf("index: %d value: %d\n", reg->index, reg->value);
    }
}

void on_read_registers_failure(rodbus_request_error_t error, void *ctx)
{
    printf("error: %s\n", rodbus_request_error_to_string(error));
}

/// ANCHOR: write_callback
void on_write_complete(rodbus_nothing_t nothing, void *ctx)
{
    printf("success!\n");
}

void on_write_failure(rodbus_request_error_t error, void *ctx)
{
    printf("error: %s\n", rodbus_request_error_to_string(error));
}
/// ANCHOR_END: write_callback

void on_client_state_change(rodbus_client_state_t state, void *ctx)
{ 
    printf("client state: %s\n", rodbus_client_state_to_string(state)); 
}

void on_port_state_change(rodbus_port_state_t state, void *ctx)
{ 
    printf("port state: %s\n", rodbus_port_state_to_string(state));
}

rodbus_client_state_listener_t get_client_listener()
{
    return rodbus_client_state_listener_init(on_client_state_change, NULL, NULL);
}

rodbus_port_state_listener_t get_port_listener()
{ 
    return rodbus_port_state_listener_init(on_port_state_change, NULL, NULL);
}

run_channel(rodbus_client_channel_t* channel)
{
    // ANCHOR: enable_channel
    rodbus_client_channel_enable(channel);
    // ANCHOR_END: enable_channel
    
    // request param that we will be reusing
    // ANCHOR: request_param
    rodbus_request_param_t param = rodbus_request_param_init(1,   // Unit ID
                                                             1000 // Timeout in ms
    );
    // ANCHOR_END: request_param

    // address range that we will be reusing
    // ANCHOR: address_range
    rodbus_address_range_t range = rodbus_address_range_init(0, // start
                                                             5  // count
    );
    // ANCHOR_END: address_range

    // ANCHOR: bit_read_callback_init
    rodbus_bit_read_callback_t bit_callback = rodbus_bit_read_callback_init(
        on_read_bits_complete, // Success callback
        on_read_bits_failure, // Failure callback
        NULL, // Destroy callback
        NULL // Callback context
    );
    // ANCHOR_END: bit_read_callback_init
    rodbus_register_read_callback_t register_callback = rodbus_register_read_callback_init(
        on_read_registers_complete, // Success callback
        on_read_registers_failure, // Failure callback
        NULL, // Destroy callback
        NULL // Callback context
    );
    // ANCHOR: write_callback_init
    rodbus_write_callback_t write_callback = rodbus_write_callback_init(
        on_write_complete, // Success callback
        on_write_failure, // Failure callback
        NULL, // Destroy callback
        NULL // Callback context
    );
    /// ANCHOR_END: write_callback_init

    char cbuf[10];
    while (true) {
        fgets(cbuf, 10, stdin);

        if (strcmp(cbuf, "x\n") == 0) {
            break;
        }
        else if (strcmp(cbuf, "ec\n") == 0) {
            rodbus_client_channel_enable(channel);
        }
        else if (strcmp(cbuf, "dc\n") == 0) {
            rodbus_client_channel_disable(channel);
        }
        else if (strcmp(cbuf, "rc\n") == 0) {
            // ANCHOR: read_coils
            rodbus_client_channel_read_coils(channel, param, range, bit_callback);
            // ANCHOR_END: read_coils
        }
        else if (strcmp(cbuf, "rdi\n") == 0) {
            rodbus_client_channel_read_discrete_inputs(channel, param, range, bit_callback);
        }
        else if (strcmp(cbuf, "rhr\n") == 0) {
            rodbus_client_channel_read_holding_registers(channel, param, range, register_callback);
        }
        else if (strcmp(cbuf, "rir\n") == 0) {
            rodbus_client_channel_read_input_registers(channel, param, range, register_callback);
        }
        else if (strcmp(cbuf, "wsc\n") == 0) {
            /// ANCHOR: write_single_coil
            rodbus_bit_value_t bit_value = rodbus_bit_value_init(0, true);
            rodbus_client_channel_write_single_coil(channel, param, bit_value, write_callback);
            /// ANCHOR_END: write_single_coil
        }
        else if (strcmp(cbuf, "wsr\n") == 0) {
            rodbus_register_value_t register_value = rodbus_register_value_init(0, 76);
            rodbus_client_channel_write_single_register(channel, param, register_value, write_callback);
        }
        else if (strcmp(cbuf, "wmc\n") == 0) {
            // create the bitlist
            rodbus_bit_list_t *bit_list = rodbus_bit_list_create(2);
            rodbus_bit_list_add(bit_list, true);
            rodbus_bit_list_add(bit_list, false);

            // send the request
            rodbus_client_channel_write_multiple_coils(channel, param, 0, bit_list, write_callback);

            // destroy the bitlist
            rodbus_bit_list_destroy(bit_list);
        }
        else if (strcmp(cbuf, "wmr\n") == 0) {
            // create the register list
            // ANCHOR: write_multiple_registers
            rodbus_register_list_t *register_list = rodbus_register_list_create(2);
            rodbus_register_list_add(register_list, 0xCA);
            rodbus_register_list_add(register_list, 0xFE);

            // send the request
            rodbus_client_channel_write_multiple_registers(channel, param, 0, register_list, write_callback);

            // destroy the register list
            rodbus_register_list_destroy(register_list);
            // ANCHOR_END: write_multiple_registers
        }
        else {
            printf("Unknown command\n");
        }
    }

    rodbus_client_channel_destroy(channel);

    return 0;
}

int run_tcp_channel(rodbus_runtime_t* runtime)
{
    // ANCHOR: create_tcp_channel
    rodbus_client_channel_t* channel = NULL;
    rodbus_decode_level_t decode_level = rodbus_decode_level_nothing();
    rodbus_param_error_t err = rodbus_client_channel_create_tcp(runtime, "127.0.0.1", 502, 1, rodbus_retry_strategy_init(), decode_level, get_client_listener(), &channel);
    if (err) {
        printf("Unable to initialize channel: %s\n", rodbus_param_error_to_string(err));
        return -1;
    }
    // ANCHOR_END: create_tcp_channel

    return run_channel(channel);
}

int run_rtu_channel(rodbus_runtime_t* runtime)
{
    // ANCHOR: create_rtu_channel
    rodbus_client_channel_t* channel = NULL;
    rodbus_decode_level_t decode_level = rodbus_decode_level_nothing();
    rodbus_param_error_t err = rodbus_client_channel_create_rtu(runtime,
        "/dev/ttySIM0", // path
        rodbus_serial_port_settings_init(), // serial settings
        1, // max queued requests
        rodbus_retry_strategy_init(),
        decode_level, // decode level
        get_port_listener(),
        &channel
    );
    if (err) {
        printf("Unable to initialize channel: %s\n", rodbus_param_error_to_string(err));
        return -1;
    }
    // ANCHOR_END: create_rtu_channel

    return run_channel(channel);
}

rodbus_tls_client_config_t get_self_signed_tls_config()
{
    // ANCHOR: tls_self_signed_config
    rodbus_tls_client_config_t tls_config = rodbus_tls_client_config_init(
        "test.com",
        "./certs/self_signed/entity2_cert.pem",
        "./certs/self_signed/entity1_cert.pem",
        "./certs/self_signed/entity1_key.pem",
        "" // no password
    );
    tls_config.certificate_mode = RODBUS_CERTIFICATE_MODE_SELF_SIGNED;
    // ANCHOR_END: tls_self_signed_config

    return tls_config;
}

rodbus_tls_client_config_t get_ca_tls_config()
{
    // ANCHOR: tls_ca_chain_config
    rodbus_tls_client_config_t tls_config = rodbus_tls_client_config_init(
        "test.com",
        "./certs/ca_chain/ca_cert.pem",
        "./certs/ca_chain/client_cert.pem",
        "./certs/ca_chain/client_key.pem",
        "" // no password
    );
    // ANCHOR_END: tls_ca_chain_config

    return tls_config;
}

int run_tls_channel(rodbus_runtime_t* runtime, rodbus_tls_client_config_t tls_config)
{
    // ANCHOR: create_tls_channel
    rodbus_client_channel_t* channel = NULL;
    rodbus_decode_level_t decode_level = rodbus_decode_level_nothing();
    rodbus_param_error_t err = rodbus_client_channel_create_tls(runtime, "127.0.0.1", 802, 100, rodbus_retry_strategy_init(), tls_config, decode_level,
                                                                get_client_listener(), & channel);
    if (err) {
        printf("Unable to initialize channel: %s\n", rodbus_param_error_to_string(err));
        return -1;
    }
    // ANCHOR_END: create_tls_channel

    return run_channel(channel);
}

// create a channel based on the command line arguments
int create_and_run_channel(int argc, char *argv[], rodbus_runtime_t *runtime)
{
    if(argc != 2) {
        printf("you must specify a transport type\n");
        printf("usage: client_example <channel> (tcp, rtu, tls-ca, tls-self-signed)\n");
        return -1;
    }

    if (strcmp(argv[1], "tcp") == 0) {
        return run_tcp_channel(runtime);
    }
    else if (strcmp(argv[1], "rtu") == 0) {
        return run_rtu_channel(runtime);
    }
    else if (strcmp(argv[1], "tls-ca") == 0) {
        return run_tls_channel(runtime, get_ca_tls_config());
    }
    else if (strcmp(argv[1], "tls-self-signed") == 0) {
        return run_tls_channel(runtime, get_self_signed_tls_config());
    }
    else {
        printf("unknown channel type: %s\n", argv[1]);
        return -1;
    }
}

int main(int argc, char* argv[])
{
    // ANCHOR: logging_init
    // initialize logging with the default configuration
    rodbus_logger_t logger = rodbus_logger_init(&on_log_message, NULL, NULL);
    rodbus_configure_logging(rodbus_logging_config_init(), logger);
    // ANCHOR_END: logging_init

    // initialize the runtime
    // ANCHOR: runtime_create
    rodbus_runtime_t *runtime = NULL;
    rodbus_runtime_config_t runtime_config = rodbus_runtime_config_init();
    runtime_config.num_core_threads = 4;
    rodbus_param_error_t err = rodbus_runtime_create(runtime_config, &runtime);
    if (err) {
        printf("Unable to initialize runtime: %s\n", rodbus_param_error_to_string(err));
        return -1;
    }
    // ANCHOR_END: runtime_create

    // create a channel based on the cmd line arguments and run it
    int res = create_and_run_channel(argc, argv, runtime);

    // ANCHOR: runtime_destroy
    rodbus_runtime_destroy(runtime);
    // ANCHOR_END: runtime_destroy

    return res;
}
