#include <stdio.h>

#include <rodbus.h>

#ifdef __unix__
#include <unistd.h>
#elif defined _WIN32
#include <windows.h>
#define sleep(x) Sleep(1000 * (x))
#endif

void on_log_message(rodbus_log_level_t level, const char *message, void *ctx) { printf("%s \n", message); }

rodbus_logger_t get_logger()
{
    return (rodbus_logger_t){
        // function pointer where log messages will be sent
        .on_message = &on_log_message,
        // no context to free
        .on_destroy = NULL,
        // optional context argument applied to all log callbacks
        .ctx = NULL,
    };
}

void on_read_bits_complete(rodbus_bit_read_result_t bits, void *ctx)
{
    switch (bits.result.summary) {
    case (RODBUS_STATUS_OK): {
        printf("success!\n");
        rodbus_bit_t *bit = NULL;
        while (bit = rodbus_next_bit(bits.iterator)) {
            printf("value: %d index: %d\n", bit->value, bit->index);
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
        rodbus_register_t *bit = NULL;
        while (bit = rodbus_next_register(registers.iterator)) {
            printf("value: %d index: %d\n", bit->value, bit->index);
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
    rodbus_configure_logging(rodbus_logging_config_init(), get_logger());

    rodbus_runtime_t *runtime = NULL;
    rodbus_channel_t *channel = NULL;
    rodbus_param_error_t err = RODBUS_PARAM_ERROR_OK;

    rodbus_runtime_config_t runtime_config = rodbus_runtime_config_init();
    runtime_config.num_core_threads = 4;
    err = rodbus_runtime_new(runtime_config, &runtime);
    if (!err) {
        printf("Unable to initialize runtime \n");
        goto cleanup;
    }

    err = rodbus_create_tcp_client(runtime, "127.0.0.1:502", 100, &channel);
    if (!channel) {
        printf("Unable to initialize channel \n");
        goto cleanup;
    }

    rodbus_request_param_t params = {
        .unit_id = 1,
        .timeout_ms = 1000,
    };

    for (int i = 0; i < 3; ++i) {
        rodbus_bit_read_callback_t bit_callback = {
            .on_complete = on_read_bits_complete,
            .ctx = NULL,
        };
        rodbus_register_read_callback_t register_callback = {
            .on_complete = on_read_registers_complete,
            .ctx = NULL,
        };
        rodbus_result_callback_t result_callback = {
            .on_complete = on_write_complete,
            .ctx = NULL,
        };

        rodbus_address_range_t range = {
            .start = 0,
            .count = 5,
        };

        /*
                printf("reading coils\n");
                channel_read_coils_async(channel, range, params, bit_callback);
                sleep(1);

                printf("reading discrete inputs\n");
                channel_read_discrete_inputs_async(channel, range, params, bit_callback);
                sleep(1);


                printf("reading holding registers\n");
                channel_read_holding_registers_async(channel, range, params, register_callback);
                sleep(1);

                printf("reading input registers\n");
                channel_read_input_registers_async(channel, range, params, register_callback);
                sleep(1);

                printf("writing single coil\n");
                bit_t bit = { .index = 0, .value = true };
                channel_write_single_coil_async(channel, bit, params, result_callback);
                sleep(1);
                */

        printf("writing multiple coils\n");
        rodbus_bit_list_t *bits = rodbus_bit_list_create(0);
        rodbus_bit_list_add(bits, true);
        rodbus_bit_list_add(bits, false);
        rodbus_channel_write_multiple_coils_async(channel, 0, bits, params, result_callback);
        rodbus_bit_list_destroy(bits);
        sleep(1);
    }

cleanup:
    rodbus_channel_destroy(channel);
    rodbus_runtime_destroy(runtime);

    return 0;
}
