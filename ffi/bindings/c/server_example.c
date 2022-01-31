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
        return rodbus_write_result_success();
    }
    else {
        return rodbus_write_result_exception(RODBUS_MODBUS_EXCEPTION_ILLEGAL_DATA_ADDRESS);
    }
}

rodbus_write_result_t on_write_single_register(uint16_t index, uint16_t value, rodbus_database_t *db, void *ctx)
{
    if (rodbus_database_update_holding_register(db, index, value)) {
        return rodbus_write_result_success();
    }
    else {
        return rodbus_write_result_exception(RODBUS_MODBUS_EXCEPTION_ILLEGAL_DATA_ADDRESS);
    }
}

rodbus_write_result_t on_write_multiple_coils(uint16_t start, rodbus_bit_iterator_t *it, rodbus_database_t *db, void *ctx)
{
    rodbus_write_result_t result = rodbus_write_result_success();
    rodbus_bit_t *bit = NULL;
    while ((bit = rodbus_next_bit(it))) {
        if (!rodbus_database_update_coil(db, bit->index, bit->value)) {
            result = rodbus_write_result_exception(RODBUS_MODBUS_EXCEPTION_ILLEGAL_DATA_ADDRESS);
        }
    }
    return result;
}

rodbus_write_result_t on_write_multiple_registers(uint16_t start, rodbus_register_iterator_t *it, rodbus_database_t *db, void *ctx)
{
    rodbus_write_result_t result = rodbus_write_result_success();
    rodbus_register_t *reg = NULL;
    while ((reg = rodbus_next_register(it))) {
        if (!rodbus_database_update_holding_register(db, reg->index, reg->value)) {
            result = rodbus_write_result_exception(RODBUS_MODBUS_EXCEPTION_ILLEGAL_DATA_ADDRESS);
        }
    }
    return result;
}
// ANCHOR_END: write_handler

// ANCHOR: auth_handler
rodbus_authorization_result_t auth_read(uint8_t unit_id, rodbus_address_range_t range, const char* role, void* ctx)
{
    return RODBUS_AUTHORIZATION_RESULT_AUTHORIZED;
}

rodbus_authorization_result_t auth_single_write(uint8_t unit_id, uint16_t idx, const char* role, void* ctx)
{
    return RODBUS_AUTHORIZATION_RESULT_NOT_AUTHORIZED;
}

rodbus_authorization_result_t auth_multiple_writes(uint8_t unit_id, rodbus_address_range_t range, const char* role, void* ctx)
{
    return RODBUS_AUTHORIZATION_RESULT_NOT_AUTHORIZED;
}
// ANCHOR_END: auth_handler

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

int run_server(rodbus_server_t* server)
{
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
            break;
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

    rodbus_server_destroy(server);

    return 0;
}

rodbus_device_map_t* build_device_map()
{
    // ANCHOR: device_map_init
    rodbus_write_handler_t write_handler =
        rodbus_write_handler_init(&on_write_single_coil, &on_write_single_register, &on_write_multiple_coils, &on_write_multiple_registers, NULL, NULL);

    rodbus_device_map_t* map = rodbus_device_map_new();
    rodbus_map_add_endpoint(map,
                            1,                                                      // Unit ID
                            write_handler,                                          // Handler for write requests
                            rodbus_database_callback_init(configure_db, NULL, NULL) // Callback for the initial state of the database
    );
    // ANCHOR_END: device_map_init

    return map;
}

int run_tcp_channel(rodbus_runtime_t* runtime)
{
    // ANCHOR: tcp_server_create
    rodbus_server_t* server = NULL;
    rodbus_device_map_t* map = build_device_map();
    rodbus_decode_level_t decode_level = rodbus_decode_level_init();
    rodbus_param_error_t err = rodbus_create_tcp_server(runtime, "127.0.0.1:502", 100, map, decode_level, &server);
    rodbus_device_map_destroy(map);

    if (err) {
        printf("Unable to initialize server: %s\n", rodbus_param_error_to_string(err));
        return -1;
    }
    // ANCHOR_END: tcp_server_create

    return run_server(server);
}

int run_rtu_channel(rodbus_runtime_t* runtime)
{
    // ANCHOR: rtu_server_create
    rodbus_server_t* server = NULL;
    rodbus_device_map_t* map = build_device_map();
    rodbus_decode_level_t decode_level = rodbus_decode_level_init();
    rodbus_param_error_t err = rodbus_create_rtu_server(runtime, "/dev/ttySIM1", rodbus_serial_port_settings_init(), map, decode_level, &server);
    rodbus_device_map_destroy(map);

    if (err) {
        printf("Unable to initialize server: %s\n", rodbus_param_error_to_string(err));
        return -1;
    }
    // ANCHOR_END: rtu_server_create

    return run_server(server);
}

rodbus_authorization_handler_t get_auth_handler()
{
    // ANCHOR: auth_handler_init
    rodbus_authorization_handler_t auth_handler = rodbus_authorization_handler_init(
        &auth_read, &auth_read, &auth_read, &auth_read,
        &auth_single_write, &auth_single_write, &auth_multiple_writes, &auth_multiple_writes,
        NULL, NULL
    );
    // ANCHOR_END: auth_handler_init

    return auth_handler;
}

rodbus_tls_server_config_t get_self_signed_tls_config()
{
    // ANCHOR: tls_self_signed_config
    rodbus_tls_server_config_t tls_config = rodbus_tls_server_config_init(
        "./certs/self_signed/entity1_cert.pem",
        "./certs/self_signed/entity2_cert.pem",
        "./certs/self_signed/entity2_key.pem",
        "" // no password
    );
    tls_config.certificate_mode = RODBUS_CERTIFICATE_MODE_SELF_SIGNED;
    // ANCHOR_END: tls_self_signed_config

    return tls_config;
}

rodbus_tls_server_config_t get_ca_tls_config()
{
    // ANCHOR: tls_ca_chain_config
    rodbus_tls_server_config_t tls_config = rodbus_tls_server_config_init(
        "./certs/ca_chain/ca_cert.pem",
        "./certs/ca_chain/entity2_cert.pem",
        "./certs/ca_chain/entity2_key.pem",
        "" // no password
    );
    // ANCHOR_END: tls_ca_chain_config

    return tls_config;
}

int run_tls_channel(rodbus_runtime_t* runtime, rodbus_tls_server_config_t tls_config)
{
    // ANCHOR: tls_server_create
    rodbus_server_t* server = NULL;
    rodbus_device_map_t* map = build_device_map();
    rodbus_authorization_handler_t auth_handler = get_auth_handler();
    rodbus_decode_level_t decode_level = rodbus_decode_level_init();
    rodbus_param_error_t err = rodbus_create_tls_server(runtime, "127.0.0.1:802", 100, map, tls_config, auth_handler, decode_level, &server);
    rodbus_device_map_destroy(map);

    if (err) {
        printf("Unable to initialize server: %s\n", rodbus_param_error_to_string(err));
        return -1;
    }
    // ANCHOR_END: tls_server_create

    return run_server(server);
}

// create a channel based on the command line arguments
int create_and_run_channel(int argc, char *argv[], rodbus_runtime_t *runtime)
{
    if(argc != 2) {
        printf("you must specify a transport type\n");
        printf("usage: server_example <channel> (tcp, rtu, tls-ca, tls-self-signed)\n");
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
    // initialize logging with the default configuration
    rodbus_logger_t logger = rodbus_logger_init(&on_log_message, NULL, NULL);
    rodbus_configure_logging(rodbus_logging_config_init(), logger);

    // Create runtime
    rodbus_runtime_t *runtime = NULL;
    rodbus_runtime_config_t runtime_config = rodbus_runtime_config_init();
    runtime_config.num_core_threads = 4;
    rodbus_param_error_t err = rodbus_runtime_new(runtime_config, &runtime);
    if (err) {
        printf("Unable to initialize runtime: %s\n", rodbus_param_error_to_string(err));
        return -1;
    }

    // create a channel based on the cmd line arguments and run it
    int res = create_and_run_channel(argc, argv, runtime);

    rodbus_runtime_destroy(runtime);

    return res;
}
