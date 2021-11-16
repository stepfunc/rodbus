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
void on_read_bits_complete(rodbus_bit_read_result_t bits, void *ctx)
{
    switch (bits.result.summary) {
    case (RODBUS_STATUS_OK): {
        printf("success!\n");
        rodbus_bit_t *bit = NULL;
        while ((bit = rodbus_next_bit(bits.iterator))) {
            printf("index: %d value: %d\n", bit->index, bit->value);
        }
        break;
    }
    case (RODBUS_STATUS_EXCEPTION):
        printf("Modbus exception: %d\n", bits.result.exception);
        break;
    default:
        printf("error: %s \n", rodbus_status_to_string(bits.result.summary));
        break;
    }
}
// ANCHOR_END: bit_read_callback

void on_read_registers_complete(rodbus_register_read_result_t registers, void *ctx)
{
    // ANCHOR: error_handling
    switch (registers.result.summary) {
    case (RODBUS_STATUS_OK): {
        printf("success!\n");
        rodbus_register_t *reg = NULL;
        while ((reg = rodbus_next_register(registers.iterator))) {
            printf("index: %d value: %d\n", reg->index, reg->value);
        }
        break;
    }
    case (RODBUS_STATUS_EXCEPTION):
        printf("Modbus exception: %d\n", registers.result.exception);
        break;
    default:
        printf("error: %s \n", rodbus_status_to_string(registers.result.summary));
        break;
    }
    // ANCHOR_END: error_handling
}

/// ANCHOR: write_callback
void on_write_complete(rodbus_error_info_t result, void *ctx)
{
    switch (result.summary) {
    case (RODBUS_STATUS_OK): {
        printf("success!\n");
        break;
    }
    case (RODBUS_STATUS_EXCEPTION):
        printf("Modbus exception: %d\n", result.exception);
        break;
    default:
        printf("error: %s \n", rodbus_status_to_string(result.summary));
        break;
    }
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
    rodbus_channel_t *channel = NULL;
    // ANCHOR_END: channel_decl
    // ANCHOR: error_decl
    rodbus_param_error_t err = RODBUS_PARAM_ERROR_OK;
    // ANCHOR_END: error_decl

    // initialize the runtime
    // ANCHOR: runtime_create
    rodbus_runtime_config_t runtime_config = rodbus_runtime_config_init();
    runtime_config.num_core_threads = 4;
    err = rodbus_runtime_new(runtime_config, &runtime);
    // ANCHOR_END: runtime_create
    if (err) {
        printf("Unable to initialize runtime: %s\n", rodbus_param_error_to_string(err));
        goto cleanup;
    }

    // ANCHOR: tls_self_signed_config
    rodbus_tls_client_config_t self_signed_tls_config = rodbus_tls_client_config_init(
        "test.com",
        "./certs/self_signed/entity2_cert.pem",
        "./certs/self_signed/entity1_cert.pem",
        "./certs/self_signed/entity1_key.pem",
        "" // no password
    );
    self_signed_tls_config.certificate_mode = RODBUS_CERTIFICATE_MODE_SELF_SIGNED_CERTIFICATE;
    // ANCHOR_END: tls_self_signed_config

    // ANCHOR: tls_ca_chain_config
    rodbus_tls_client_config_t ca_chain_tls_config = rodbus_tls_client_config_init(
        "test.com",
        "./certs/ca_chain/ca_cert.pem",
        "./certs/ca_chain/entity1_cert.pem",
        "./certs/ca_chain/entity1_key.pem",
        "" // no password
    );
    // ANCHOR_END: tls_ca_chain_config

    rodbus_tls_client_config_t tls_config = ca_chain_tls_config;

    // initialize a Modbus TLS client channel
    // ANCHOR: create_tls_channel
    rodbus_decode_level_t decode_level = rodbus_decode_level_init();
    err = rodbus_create_tls_client(runtime, "127.0.0.1:502", 100, rodbus_retry_strategy_init(), tls_config, decode_level, &channel);
    if (err) {
        printf("Unable to initialize channel: %s\n", rodbus_param_error_to_string(err));
        goto cleanup;
    }
    // ANCHOR_END: create_tls_channel

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
    rodbus_bit_read_callback_t bit_callback = rodbus_bit_read_callback_init(on_read_bits_complete, NULL, NULL);
    // ANCHOR_END: bit_read_callback_init
    rodbus_register_read_callback_t register_callback = rodbus_register_read_callback_init(on_read_registers_complete, NULL, NULL);
    // ANCHOR: write_callback_init
    rodbus_write_callback_t write_callback = rodbus_write_callback_init(on_write_complete, NULL, NULL);
    /// ANCHOR_END: write_callback_init

    char cbuf[10];
    while (true) {
        fgets(cbuf, 10, stdin);

        if (strcmp(cbuf, "x\n") == 0) {
            goto cleanup;
        }
        else if (strcmp(cbuf, "rc\n") == 0) {
            // ANCHOR: read_coils
            rodbus_channel_read_coils(channel, param, range, bit_callback);
            // ANCHOR_END: read_coils
        }
        else if (strcmp(cbuf, "rdi\n") == 0) {
            rodbus_channel_read_discrete_inputs(channel, param, range, bit_callback);
        }
        else if (strcmp(cbuf, "rhr\n") == 0) {
            rodbus_channel_read_holding_registers(channel, param, range, register_callback);
        }
        else if (strcmp(cbuf, "rir\n") == 0) {
            rodbus_channel_read_input_registers(channel, param, range, register_callback);
        }
        else if (strcmp(cbuf, "wsc\n") == 0) {
            /// ANCHOR: write_single_coil
            rodbus_bit_t bit_value = rodbus_bit_init(0, true);
            rodbus_channel_write_single_coil(channel, param, bit_value, write_callback);
            /// ANCHOR_END: write_single_coil
        }
        else if (strcmp(cbuf, "wsr\n") == 0) {
            rodbus_register_t register_value = rodbus_register_init(0, 76);
            rodbus_channel_write_single_register(channel, param, register_value, write_callback);
        }
        else if (strcmp(cbuf, "wmc\n") == 0) {
            // create the bitlist
            rodbus_bit_list_t *bit_list = rodbus_bit_list_create(2);
            rodbus_bit_list_add(bit_list, true);
            rodbus_bit_list_add(bit_list, false);

            // send the request
            rodbus_channel_write_multiple_coils(channel, param, 0, bit_list, write_callback);

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
            rodbus_channel_write_multiple_registers(channel, param, 0, register_list, write_callback);

            // destroy the register list
            rodbus_register_list_destroy(register_list);
            // ANCHOR_END: write_multiple_registers
        }
        else {
            printf("Unknown command\n");
        }
    }

cleanup:
    rodbus_channel_destroy(channel);
    // ANCHOR: runtime_destroy
    rodbus_runtime_destroy(runtime);
    // ANCHOR_END: runtime_destroy

    return 0;
}
