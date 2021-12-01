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

int main()
{
    // ANCHOR: logging_init
    // initialize logging with the default configuration
    rodbus_logger_t logger = rodbus_logger_init(&on_log_message, NULL, NULL);
    rodbus_configure_logging(rodbus_logging_config_init(), logger);
    // ANCHOR_END: logging_init

    // ANCHOR: runtime_decl
    rodbus_runtime_t *runtime = NULL;
    // ANCHOR_END: runtime_decl
    // ANCHOR: channel_decl
    rodbus_client_channel_t *channel = NULL;
    // ANCHOR_END: channel_decl
    // ANCHOR: error_decl
    rodbus_param_error_t err = RODBUS_PARAM_ERROR_OK;
    // ANCHOR_END: error_decl

    // initialize the runtime
    // ANCHOR: runtime_create
    rodbus_runtime_config_t runtime_config = rodbus_runtime_config_init();
    runtime_config.num_core_threads = 4;
    err = rodbus_runtime_create(runtime_config, &runtime);
    // ANCHOR_END: runtime_create
    if (err) {
        printf("Unable to initialize runtime: %s\n", rodbus_param_error_to_string(err));
        goto cleanup;
    }

    // initialize a Modbus TCP client channel
    // ANCHOR: create_tcp_channel
    rodbus_decode_level_t decode_level = rodbus_decode_level_init();
    err = rodbus_client_channel_create_tcp(runtime, "127.0.0.1:502", 100, rodbus_retry_strategy_init(), decode_level, &channel);
    if (err) {
        printf("Unable to initialize channel: %s\n", rodbus_param_error_to_string(err));
        goto cleanup;
    }
    // ANCHOR_END: create_tcp_channel

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
            goto cleanup;
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

cleanup:
    rodbus_client_channel_destroy(channel);
    // ANCHOR: runtime_destroy
    rodbus_runtime_destroy(runtime);
    // ANCHOR_END: runtime_destroy

    return 0;
}
