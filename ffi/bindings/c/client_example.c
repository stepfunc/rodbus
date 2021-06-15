#include "rodbus.h"

#include <inttypes.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>

void on_log_message(rodbus_log_level_t level, const char *message, void *ctx) { printf("%s\n", message); }

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

void on_read_registers_complete(rodbus_register_read_result_t registers, void *ctx)
{
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
}

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

int main()
{
    // initialize logging with the default configuration
    rodbus_logger_t logger = rodbus_logger_init(&on_log_message, NULL, NULL);
    rodbus_configure_logging(rodbus_logging_config_init(), logger);

    rodbus_runtime_t *runtime = NULL;
    rodbus_channel_t *channel = NULL;
    rodbus_param_error_t err = RODBUS_PARAM_ERROR_OK;

    // initialize the runtime
    rodbus_runtime_config_t runtime_config = rodbus_runtime_config_init();
    runtime_config.num_core_threads = 4;
    err = rodbus_runtime_new(runtime_config, &runtime);
    if (err) {
        printf("Unable to initialize runtime: %s\n", rodbus_param_error_to_string(err));
        goto cleanup;
    }

    // initialize a Modbus TCP client channel
    rodbus_decode_level_t decode_level = rodbus_decode_level_init();
    err = rodbus_create_tcp_client(runtime, "127.0.0.1:502", 100, decode_level, &channel);
    if (err) {
        printf("Unable to initialize channel: %s\n", rodbus_param_error_to_string(err));
        goto cleanup;
    }

    // request param that we will be reusing
    rodbus_request_param_t param = rodbus_request_param_init(1,   // Unit ID
                                                             1000 // Timeout in ms
    );

    // address range that we will be reusing
    rodbus_address_range_t range = rodbus_address_range_init(0, // start
                                                             5  // count
    );

    rodbus_bit_read_callback_t bit_callback = rodbus_bit_read_callback_init(on_read_bits_complete, NULL, NULL);
    rodbus_register_read_callback_t register_callback = rodbus_register_read_callback_init(on_read_registers_complete, NULL, NULL);
    rodbus_result_callback_t result_callback = rodbus_result_callback_init(on_write_complete, NULL, NULL);

    char cbuf[10];
    while (true) {
        fgets(cbuf, 10, stdin);

        if (strcmp(cbuf, "x\n") == 0) {
            goto cleanup;
        }
        else if (strcmp(cbuf, "rc\n") == 0) {
            rodbus_channel_read_coils(channel, range, param, bit_callback);
        }
        else if (strcmp(cbuf, "rdi\n") == 0) {
            rodbus_channel_read_discrete_inputs(channel, range, param, bit_callback);
        }
        else if (strcmp(cbuf, "rhr\n") == 0) {
            rodbus_channel_read_holding_registers(channel, range, param, register_callback);
        }
        else if (strcmp(cbuf, "rir\n") == 0) {
            rodbus_channel_read_input_registers(channel, range, param, register_callback);
        }
        else if (strcmp(cbuf, "wsc\n") == 0) {
            rodbus_bit_t bit_value = rodbus_bit_init(0, true);
            rodbus_channel_write_single_coil(channel, bit_value, param, result_callback);
        }
        else if (strcmp(cbuf, "wsr\n") == 0) {
            rodbus_register_t register_value = rodbus_register_init(0, 76);
            rodbus_channel_write_single_register(channel, register_value, param, result_callback);
        }
        else if (strcmp(cbuf, "wmc\n") == 0) {
            // create the bitlist
            rodbus_bit_list_t *bit_list = rodbus_bit_list_create(2);
            rodbus_bit_list_add(bit_list, true);
            rodbus_bit_list_add(bit_list, false);

            // send the request
            rodbus_channel_write_multiple_coils(channel, 0, bit_list, param, result_callback);

            // destroy the bitlist
            rodbus_bit_list_destroy(bit_list);
        }
        else if (strcmp(cbuf, "wmr\n") == 0) {
            // create the register list
            rodbus_register_list_t *register_list = rodbus_register_list_create(2);
            rodbus_register_list_add(register_list, 0xCA);
            rodbus_register_list_add(register_list, 0xFE);

            // send the request
            rodbus_channel_write_multiple_registers(channel, 0, register_list, param, result_callback);

            // destroy the register list
            rodbus_register_list_destroy(register_list);
        }
        else {
            printf("Unknown command\n");
        }
    }

cleanup:
    rodbus_channel_destroy(channel);
    rodbus_runtime_destroy(runtime);

    return 0;
}
