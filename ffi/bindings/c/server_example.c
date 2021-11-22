#include "rodbus.h"

#include <inttypes.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>

void on_log_message(rodbus_log_level_t level, const char *message, void *ctx) { printf("%s \n", message); }

// ANCHOR: write_handler
rodbus_write_result_t on_write_single_coil(uint16_t index, bool value, rodbus_database_t *db, void *ctx)
{
    if (rodbus_database_update_coil(db, index, value)) {
        return rodbus_write_result_success_init();
    }
    else {
        return rodbus_write_result_exception_init(RODBUS_MODBUS_EXCEPTION_ILLEGAL_DATA_ADDRESS);
    }
}

rodbus_write_result_t on_write_single_register(uint16_t index, uint16_t value, rodbus_database_t *db, void *ctx)
{
    if (rodbus_database_update_holding_register(db, index, value)) {
        return rodbus_write_result_success_init();
    }
    else {
        return rodbus_write_result_exception_init(RODBUS_MODBUS_EXCEPTION_ILLEGAL_DATA_ADDRESS);
    }
}

rodbus_write_result_t on_write_multiple_coils(uint16_t start, rodbus_bit_value_iterator_t *it, rodbus_database_t *db, void *ctx)
{
    rodbus_write_result_t result = rodbus_write_result_success_init();
    rodbus_bit_value_t *bit = NULL;
    while ((bit = rodbus_bit_value_iterator_next(it))) {
        if (!rodbus_database_update_coil(db, bit->index, bit->value)) {
            result = rodbus_write_result_exception_init(RODBUS_MODBUS_EXCEPTION_ILLEGAL_DATA_ADDRESS);
        }
    }
    return result;
}

rodbus_write_result_t on_write_multiple_registers(uint16_t start, rodbus_register_value_iterator_t *it, rodbus_database_t *db, void *ctx)
{
    rodbus_write_result_t result = rodbus_write_result_success_init();
    rodbus_register_value_t *reg = NULL;
    while ((reg = rodbus_register_value_iterator_next(it))) {
        if (!rodbus_database_update_holding_register(db, reg->index, reg->value)) {
            result = rodbus_write_result_exception_init(RODBUS_MODBUS_EXCEPTION_ILLEGAL_DATA_ADDRESS);
        }
    }
    return result;
}
// ANCHOR_END: write_handler

typedef struct state_t {
    bool coil_value;
    bool discrete_input_value;
    uint16_t holding_register_value;
    uint16_t input_register_value;
} state_t;

// ANCHOR: configure_db
void configure_db(rodbus_database_t *db, void *ctx)
{
    for (uint16_t i = 0; i < 10; ++i) {
        rodbus_database_add_coil(db, i, false);
        rodbus_database_add_discrete_input(db, i, false);
        rodbus_database_add_holding_register(db, i, 0);
        rodbus_database_add_input_register(db, i, 0);
    }
}
// ANCHOR_END: configure_db

// ANCHOR: update_coil_callback
void update_coil(rodbus_database_t *db, void *ctx)
{
    state_t *state = (state_t *)ctx;

    state->coil_value = !state->coil_value;

    for (uint16_t i = 0; i < 10; ++i) {
        rodbus_database_update_coil(db, i, state->coil_value);
    }
}
// ANCHOR_END: update_coil_callback

void update_discrete_input(rodbus_database_t *db, void *ctx)
{
    state_t *state = (state_t *)ctx;

    state->discrete_input_value = !state->discrete_input_value;

    for (uint16_t i = 0; i < 10; ++i) {
        rodbus_database_update_discrete_input(db, i, state->discrete_input_value);
    }
}

void update_holding_register(rodbus_database_t *db, void *ctx)
{
    state_t *state = (state_t *)ctx;

    ++state->holding_register_value;

    for (uint16_t i = 0; i < 10; ++i) {
        rodbus_database_update_holding_register(db, i, state->holding_register_value);
    }
}

void update_input_register(rodbus_database_t *db, void *ctx)
{
    state_t *state = (state_t *)ctx;

    ++state->input_register_value;

    for (uint16_t i = 0; i < 10; ++i) {
        rodbus_database_update_input_register(db, i, state->input_register_value);
    }
}

int main()
{
    // initialize logging with the default configuration
    rodbus_logger_t logger = rodbus_logger_init(&on_log_message, NULL, NULL);
    rodbus_configure_logging(rodbus_logging_config_init(), logger);

    rodbus_runtime_t *runtime = NULL;
    rodbus_server_t *server = NULL;
    rodbus_param_error_t err = RODBUS_PARAM_ERROR_OK;

    // initialize the runtime
    rodbus_runtime_config_t runtime_config = rodbus_runtime_config_init();
    runtime_config.num_core_threads = 4;
    err = rodbus_runtime_create(runtime_config, &runtime);
    if (err) {
        printf("Unable to initialize runtime: %s\n", rodbus_param_error_to_string(err));
        goto cleanup;
    }

    // create the device map
    // ANCHOR: device_map_init
    rodbus_write_handler_t write_handler =
        rodbus_write_handler_init(&on_write_single_coil, &on_write_single_register, &on_write_multiple_coils, &on_write_multiple_registers, NULL, NULL);

    rodbus_device_map_t *map = rodbus_device_map_create();
    rodbus_device_map_add_endpoint(map,
                            1,                                                      // Unit ID
                            write_handler,                                          // Handler for write requests
                            rodbus_database_callback_init(configure_db, NULL, NULL) // Callback for the initial state of the database
    );
    // ANCHOR_END: device_map_init

    // create the TCP server
    // ANCHOR: tcp_server_create
    rodbus_decode_level_t decode_level = rodbus_decode_level_init();
    err = rodbus_tcp_server_create(runtime, "127.0.0.1:502", 100, map, decode_level, &server);
    rodbus_device_map_destroy(map);

    if (err) {
        printf("Unable to initialize server: %s\n", rodbus_param_error_to_string(err));
        goto cleanup;
    }
    // ANCHOR_END: tcp_server_create

    // state passed to the update callbacks
    state_t state = {
        .coil_value = false,
        .discrete_input_value = false,
        .holding_register_value = 0,
        .input_register_value = 0,
    };

    char cbuf[10];
    while (true) {
        fgets(cbuf, 10, stdin);

        if (strcmp(cbuf, "x\n") == 0) {
            goto cleanup;
        }
        else if (strcmp(cbuf, "uc\n") == 0) {
            // ANCHOR: update_coil
            rodbus_server_update_database(server, 1, rodbus_database_callback_init(update_coil, NULL, &state));
            // ANCHOR_END: update_coil
        }
        else if (strcmp(cbuf, "udi\n") == 0) {
            rodbus_server_update_database(server, 1, rodbus_database_callback_init(update_discrete_input, NULL, &state));
        }
        else if (strcmp(cbuf, "uhr\n") == 0) {
            rodbus_server_update_database(server, 1, rodbus_database_callback_init(update_holding_register, NULL, &state));
        }
        else if (strcmp(cbuf, "uir\n") == 0) {
            rodbus_server_update_database(server, 1, rodbus_database_callback_init(update_input_register, NULL, &state));
        }
        else {
            printf("Unknown command\n");
        }
    }

cleanup:
    rodbus_server_destroy(server);
    rodbus_runtime_destroy(runtime);

    return 0;
}
