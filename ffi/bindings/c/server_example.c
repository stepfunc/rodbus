// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

// Server example for Rodbus library
//
// usage: server <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]
//
// Run it from the repository root so the TLS certificate paths resolve.

#include "rodbus.h"

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define USAGE "usage: server <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]"
#define UNIT_ID 1
#define TCP_ENDPOINT "127.0.0.1:502"
#define TLS_ENDPOINT "127.0.0.1:802"
#define SERIAL_PORT "/dev/ttySIM1"
#define MAX_SESSIONS 10
#define POINT_COUNT 10

// size of the buffers for a console line and for the host of an endpoint
#define LINE_SIZE 256

rodbus_server_t *create_server(rodbus_runtime_t *runtime, const char *transport, const char *argument);
rodbus_device_map_t *create_device_map(void);
rodbus_server_t *create_tcp_server(rodbus_runtime_t *runtime, const char *host, uint16_t port);
rodbus_server_t *create_rtu_server(rodbus_runtime_t *runtime, const char *path);
rodbus_server_t *create_tls_server(rodbus_runtime_t *runtime, const char *host, uint16_t port, rodbus_tls_server_config_t tls_config);
rodbus_tls_server_config_t get_ca_chain_config(void);
rodbus_tls_server_config_t get_self_signed_config(void);
void run_server(rodbus_server_t *server);
void print_help(void);
void run_command(rodbus_server_t *server, const char *cmd);
rodbus_logger_t get_logger(void);
void configure_db(rodbus_database_t *db, void *ctx);
rodbus_write_handler_t get_write_handler(void);
rodbus_authorization_handler_t get_auth_handler(void);
void toggle_coils(rodbus_database_t *db, void *ctx);
void toggle_discrete_inputs(rodbus_database_t *db, void *ctx);
void increment_holding_registers(rodbus_database_t *db, void *ctx);
void increment_input_registers(rodbus_database_t *db, void *ctx);
void print_requested(const char *cmd, rodbus_param_error_t err);
void print_result(const char *cmd, rodbus_param_error_t err);
bool read_line(char *line, size_t size);
bool split_endpoint(const char *endpoint, char *host, size_t host_size, uint16_t *port);
const char *default_argument(const char *transport);
void print_usage(void);

int main(int argc, char *argv[])
{
    // the transport, then optionally the endpoint or serial port
    const char *argument = (argc == 2 || argc == 3) ? default_argument(argv[1]) : NULL;
    if (!argument) {
        print_usage();
        return EXIT_FAILURE;
    }
    const char *transport = argv[1];
    if (argc == 3) {
        argument = argv[2];
    }

    // initialize logging with the default configuration
    rodbus_param_error_t logging_err = rodbus_configure_logging(rodbus_logging_config_init(), get_logger());
    if (logging_err) {
        printf("unable to configure logging: %s\n", rodbus_param_error_to_string(logging_err));
        return EXIT_FAILURE;
    }

    rodbus_runtime_t *runtime = NULL;
    rodbus_runtime_config_t runtime_config = rodbus_runtime_config_init();
    runtime_config.num_core_threads = 4;
    rodbus_param_error_t err = rodbus_runtime_create(runtime_config, &runtime);
    if (err) {
        printf("unable to create runtime: %s\n", rodbus_param_error_to_string(err));
        return EXIT_FAILURE;
    }

    int result = EXIT_FAILURE;
    rodbus_server_t *server = create_server(runtime, transport, argument);
    if (server) {
        run_server(server);
        rodbus_server_destroy(server);
        result = EXIT_SUCCESS;
    }

    rodbus_runtime_destroy(runtime);

    return result;
}

// create the server for the transport, or print the error and return NULL
rodbus_server_t *create_server(rodbus_runtime_t *runtime, const char *transport, const char *argument)
{
    if (strcmp(transport, "rtu") == 0) {
        return create_rtu_server(runtime, argument);
    }

    // the other transports listen on an endpoint
    char host[LINE_SIZE];
    uint16_t port = 0;
    if (!split_endpoint(argument, host, sizeof(host), &port)) {
        printf("invalid endpoint: %s\n", argument);
        return NULL;
    }

    if (strcmp(transport, "tcp") == 0) {
        return create_tcp_server(runtime, host, port);
    }
    else if (strcmp(transport, "tls-ca") == 0) {
        return create_tls_server(runtime, host, port, get_ca_chain_config());
    }
    else {
        // main only accepts known transports, so this is tls-self-signed
        return create_tls_server(runtime, host, port, get_self_signed_config());
    }
}

rodbus_device_map_t *create_device_map(void)
{
    // ANCHOR: device_map_init
    rodbus_device_map_t *map = rodbus_device_map_create();
    rodbus_device_map_add_endpoint(map,
        UNIT_ID,                                                 // unit id
        get_write_handler(),                                     // handler for write requests
        (rodbus_database_callback_t){.callback = &configure_db} // callback for the initial state of the database
    );
    // ANCHOR_END: device_map_init

    return map;
}

rodbus_server_t *create_tcp_server(rodbus_runtime_t *runtime, const char *host, uint16_t port)
{
    rodbus_device_map_t *map = create_device_map();

    // ANCHOR: tcp_server_create
    rodbus_server_t *server = NULL;
    rodbus_decode_level_t decode_level = rodbus_decode_level_build(RODBUS_APP_DECODE_LEVEL_DATA_HEADERS, RODBUS_FRAME_DECODE_LEVEL_NOTHING, RODBUS_PHYS_DECODE_LEVEL_NOTHING);
    rodbus_address_filter_t *filter = rodbus_address_filter_any();
    rodbus_param_error_t err = rodbus_server_create_tcp(runtime, host, port, filter, MAX_SESSIONS, map, decode_level, &server);
    rodbus_address_filter_destroy(filter);
    rodbus_device_map_destroy(map);
    if (err) {
        printf("unable to create server: %s\n", rodbus_param_error_to_string(err));
        return NULL;
    }
    // ANCHOR_END: tcp_server_create

    return server;
}

rodbus_server_t *create_rtu_server(rodbus_runtime_t *runtime, const char *path)
{
    rodbus_device_map_t *map = create_device_map();

    // ANCHOR: rtu_server_create
    rodbus_server_t *server = NULL;
    rodbus_decode_level_t decode_level = rodbus_decode_level_build(RODBUS_APP_DECODE_LEVEL_DATA_HEADERS, RODBUS_FRAME_DECODE_LEVEL_NOTHING, RODBUS_PHYS_DECODE_LEVEL_NOTHING);
    rodbus_param_error_t err = rodbus_server_create_rtu(runtime, path, rodbus_serial_port_settings_init(), rodbus_retry_strategy_init(), map, decode_level, &server);
    rodbus_device_map_destroy(map);
    if (err) {
        printf("unable to create server: %s\n", rodbus_param_error_to_string(err));
        return NULL;
    }
    // ANCHOR_END: rtu_server_create

    return server;
}

rodbus_server_t *create_tls_server(rodbus_runtime_t *runtime, const char *host, uint16_t port, rodbus_tls_server_config_t tls_config)
{
    rodbus_device_map_t *map = create_device_map();

    // ANCHOR: tls_server_create
    rodbus_server_t *server = NULL;
    rodbus_decode_level_t decode_level = rodbus_decode_level_build(RODBUS_APP_DECODE_LEVEL_DATA_HEADERS, RODBUS_FRAME_DECODE_LEVEL_NOTHING, RODBUS_PHYS_DECODE_LEVEL_NOTHING);
    rodbus_address_filter_t *filter = rodbus_address_filter_any();
    rodbus_param_error_t err = rodbus_server_create_tls_with_authz(runtime, host, port, filter, MAX_SESSIONS, map, tls_config, get_auth_handler(), decode_level, &server);
    rodbus_address_filter_destroy(filter);
    rodbus_device_map_destroy(map);
    if (err) {
        printf("unable to create server: %s\n", rodbus_param_error_to_string(err));
        return NULL;
    }
    // ANCHOR_END: tls_server_create

    return server;
}

rodbus_tls_server_config_t get_ca_chain_config(void)
{
    // ANCHOR: tls_ca_chain_config
    rodbus_tls_server_config_t tls_config = rodbus_tls_server_config_init(
        "./certs/ca_chain/ca_cert.pem",     // CA certificate
        "./certs/ca_chain/server_cert.pem", // server certificate
        "./certs/ca_chain/server_key.pem",  // server private key
        ""                                  // no password
    );
    tls_config.min_tls_version = RODBUS_MIN_TLS_VERSION_V12;
    // ANCHOR_END: tls_ca_chain_config

    return tls_config;
}

rodbus_tls_server_config_t get_self_signed_config(void)
{
    // ANCHOR: tls_self_signed_config
    rodbus_tls_server_config_t tls_config = rodbus_tls_server_config_init(
        "./certs/self_signed/entity1_cert.pem", // client certificate
        "./certs/self_signed/entity2_cert.pem", // server certificate
        "./certs/self_signed/entity2_key.pem",  // server private key
        ""                                      // no password
    );
    tls_config.min_tls_version = RODBUS_MIN_TLS_VERSION_V12;
    tls_config.certificate_mode = RODBUS_CERTIFICATE_MODE_SELF_SIGNED;
    // ANCHOR_END: tls_self_signed_config

    return tls_config;
}

// run the console until "x" or the end of input
void run_server(rodbus_server_t *server)
{
    print_help();

    char line[LINE_SIZE];
    while (read_line(line, sizeof(line))) {
        if (strcmp(line, "x") == 0) {
            return;
        }
        else if (strcmp(line, "help") == 0) {
            print_help();
        }
        else {
            run_command(server, line);
        }
    }
}

void print_help(void)
{
    printf("Commands:\n");
    printf("  help - print this list\n");
    printf("  x - exit\n");
    printf("  ed - enable protocol decoding\n");
    printf("  dd - disable protocol decoding\n");
    printf("  uc - toggle every coil\n");
    printf("  udi - toggle every discrete input\n");
    printf("  uhr - increment every holding register\n");
    printf("  uir - increment every input register\n");
}

void run_command(rodbus_server_t *server, const char *cmd)
{
    // the decode level commands only queue the change
    if (strcmp(cmd, "ed") == 0) {
        rodbus_decode_level_t level = rodbus_decode_level_build(RODBUS_APP_DECODE_LEVEL_DATA_VALUES, RODBUS_FRAME_DECODE_LEVEL_HEADER, RODBUS_PHYS_DECODE_LEVEL_LENGTH);
        print_requested(cmd, rodbus_server_set_decode_level(server, level));
    }
    else if (strcmp(cmd, "dd") == 0) {
        print_requested(cmd, rodbus_server_set_decode_level(server, rodbus_decode_level_nothing()));
    }
    else if (strcmp(cmd, "uc") == 0) {
        // ANCHOR: update_coil
        rodbus_database_callback_t transaction = {.callback = &toggle_coils};
        rodbus_param_error_t err = rodbus_server_update_database(server, UNIT_ID, transaction);
        // ANCHOR_END: update_coil
        print_result(cmd, err);
    }
    else if (strcmp(cmd, "udi") == 0) {
        rodbus_database_callback_t transaction = {.callback = &toggle_discrete_inputs};
        print_result(cmd, rodbus_server_update_database(server, UNIT_ID, transaction));
    }
    else if (strcmp(cmd, "uhr") == 0) {
        rodbus_database_callback_t transaction = {.callback = &increment_holding_registers};
        print_result(cmd, rodbus_server_update_database(server, UNIT_ID, transaction));
    }
    else if (strcmp(cmd, "uir") == 0) {
        rodbus_database_callback_t transaction = {.callback = &increment_input_registers};
        print_result(cmd, rodbus_server_update_database(server, UNIT_ID, transaction));
    }
    else {
        printf("%s: unknown command, type help for a list\n", cmd);
    }
}

// callback which will receive log messages: each one already ends in a newline
void on_log_message(rodbus_log_level_t level, const char *message, void *ctx)
{
    printf("%s", message);
}

rodbus_logger_t get_logger(void)
{
    return (rodbus_logger_t){
        .on_message = &on_log_message,
        .on_destroy = NULL,
        .ctx = NULL,
    };
}

// ANCHOR: configure_db
// POINT_COUNT points of each type, starting at index 0
void configure_db(rodbus_database_t *db, void *ctx)
{
    for (uint16_t i = 0; i < POINT_COUNT; ++i) {
        rodbus_database_add_coil(db, i, false);
        rodbus_database_add_discrete_input(db, i, false);
        rodbus_database_add_holding_register(db, i, 0);
        rodbus_database_add_input_register(db, i, 0);
    }
}
// ANCHOR_END: configure_db

// ANCHOR: write_handler
// the most values one Modbus request can write: the library rejects longer requests,
// so the multiple write buffers never fill
#define MAX_WRITE_COILS 1968
#define MAX_WRITE_REGISTERS 123

rodbus_write_result_t on_write_single_coil(uint16_t index, bool value, rodbus_database_t *db, void *ctx)
{
    if (!rodbus_database_update_coil(db, index, value)) {
        return rodbus_write_result_exception_init(RODBUS_MODBUS_EXCEPTION_ILLEGAL_DATA_ADDRESS);
    }
    printf("write coil %u: %s\n", index, value ? "true" : "false");
    return rodbus_write_result_success_init();
}

rodbus_write_result_t on_write_single_register(uint16_t index, uint16_t value, rodbus_database_t *db, void *ctx)
{
    if (!rodbus_database_update_holding_register(db, index, value)) {
        return rodbus_write_result_exception_init(RODBUS_MODBUS_EXCEPTION_ILLEGAL_DATA_ADDRESS);
    }
    printf("write holding register %u: %u\n", index, value);
    return rodbus_write_result_success_init();
}

rodbus_write_result_t on_write_multiple_coils(uint16_t start, rodbus_bit_value_iterator_t *it, rodbus_database_t *db, void *ctx)
{
    // buffer the values, checking every coil exists, before changing any
    rodbus_bit_value_t values[MAX_WRITE_COILS];
    size_t count = 0;
    rodbus_bit_value_t *bit = NULL;
    while ((bit = rodbus_bit_value_iterator_next(it))) {
        bool current = false;
        if (count == MAX_WRITE_COILS || rodbus_database_get_coil(db, bit->index, &current) != RODBUS_PARAM_ERROR_OK) {
            return rodbus_write_result_exception_init(RODBUS_MODBUS_EXCEPTION_ILLEGAL_DATA_ADDRESS);
        }
        values[count++] = *bit;
    }

    for (size_t i = 0; i < count; ++i) {
        rodbus_database_update_coil(db, values[i].index, values[i].value);
        printf("write coil %u: %s\n", values[i].index, values[i].value ? "true" : "false");
    }
    return rodbus_write_result_success_init();
}

rodbus_write_result_t on_write_multiple_registers(uint16_t start, rodbus_register_value_iterator_t *it, rodbus_database_t *db, void *ctx)
{
    // buffer the values, checking every register exists, before changing any
    rodbus_register_value_t values[MAX_WRITE_REGISTERS];
    size_t count = 0;
    rodbus_register_value_t *reg = NULL;
    while ((reg = rodbus_register_value_iterator_next(it))) {
        uint16_t current = 0;
        if (count == MAX_WRITE_REGISTERS || rodbus_database_get_holding_register(db, reg->index, &current) != RODBUS_PARAM_ERROR_OK) {
            return rodbus_write_result_exception_init(RODBUS_MODBUS_EXCEPTION_ILLEGAL_DATA_ADDRESS);
        }
        values[count++] = *reg;
    }

    for (size_t i = 0; i < count; ++i) {
        rodbus_database_update_holding_register(db, values[i].index, values[i].value);
        printf("write holding register %u: %u\n", values[i].index, values[i].value);
    }
    return rodbus_write_result_success_init();
}

rodbus_write_handler_t get_write_handler(void)
{
    return (rodbus_write_handler_t){
        .write_single_coil = &on_write_single_coil,
        .write_single_register = &on_write_single_register,
        .write_multiple_coils = &on_write_multiple_coils,
        .write_multiple_registers = &on_write_multiple_registers,
        .on_destroy = NULL,
        .ctx = NULL,
    };
}
// ANCHOR_END: write_handler

// ANCHOR: auth_handler
// allows any role to read, and only the "operator" role to write
rodbus_authorization_t authorize_read(uint8_t unit_id, rodbus_address_range_t range, const char *role, void *ctx)
{
    return RODBUS_AUTHORIZATION_ALLOW;
}

rodbus_authorization_t authorize_write(uint8_t unit_id, const char *role)
{
    if (strcmp(role, "operator") == 0) {
        return RODBUS_AUTHORIZATION_ALLOW;
    }
    printf("denied write to unit %u for role '%s'\n", unit_id, role);
    return RODBUS_AUTHORIZATION_DENY;
}

rodbus_authorization_t authorize_single_write(uint8_t unit_id, uint16_t index, const char *role, void *ctx)
{
    return authorize_write(unit_id, role);
}

rodbus_authorization_t authorize_multiple_write(uint8_t unit_id, rodbus_address_range_t range, const char *role, void *ctx)
{
    return authorize_write(unit_id, role);
}
// ANCHOR_END: auth_handler

// ANCHOR: auth_handler_init
rodbus_authorization_handler_t get_auth_handler(void)
{
    return (rodbus_authorization_handler_t){
        .read_coils = &authorize_read,
        .read_discrete_inputs = &authorize_read,
        .read_holding_registers = &authorize_read,
        .read_input_registers = &authorize_read,
        .write_single_coil = &authorize_single_write,
        .write_single_register = &authorize_single_write,
        .write_multiple_coils = &authorize_multiple_write,
        .write_multiple_registers = &authorize_multiple_write,
        .on_destroy = NULL,
        .ctx = NULL,
    };
}
// ANCHOR_END: auth_handler_init

// ANCHOR: update_coil_callback
// runs while the database is locked, so client writes in between aren't lost
void toggle_coils(rodbus_database_t *db, void *ctx)
{
    for (uint16_t i = 0; i < POINT_COUNT; ++i) {
        bool value = false;
        if (rodbus_database_get_coil(db, i, &value) == RODBUS_PARAM_ERROR_OK) {
            rodbus_database_update_coil(db, i, !value);
        }
    }
}
// ANCHOR_END: update_coil_callback

void toggle_discrete_inputs(rodbus_database_t *db, void *ctx)
{
    for (uint16_t i = 0; i < POINT_COUNT; ++i) {
        bool value = false;
        if (rodbus_database_get_discrete_input(db, i, &value) == RODBUS_PARAM_ERROR_OK) {
            rodbus_database_update_discrete_input(db, i, !value);
        }
    }
}

void increment_holding_registers(rodbus_database_t *db, void *ctx)
{
    for (uint16_t i = 0; i < POINT_COUNT; ++i) {
        uint16_t value = 0;
        if (rodbus_database_get_holding_register(db, i, &value) == RODBUS_PARAM_ERROR_OK) {
            rodbus_database_update_holding_register(db, i, (uint16_t)(value + 1)); // 65535 wraps to 0
        }
    }
}

void increment_input_registers(rodbus_database_t *db, void *ctx)
{
    for (uint16_t i = 0; i < POINT_COUNT; ++i) {
        uint16_t value = 0;
        if (rodbus_database_get_input_register(db, i, &value) == RODBUS_PARAM_ERROR_OK) {
            rodbus_database_update_input_register(db, i, (uint16_t)(value + 1)); // 65535 wraps to 0
        }
    }
}

void print_requested(const char *cmd, rodbus_param_error_t err)
{
    if (err) {
        printf("%s: %s\n", cmd, rodbus_param_error_to_string(err));
    }
    else {
        printf("%s: requested\n", cmd);
    }
}

void print_result(const char *cmd, rodbus_param_error_t err)
{
    if (err) {
        printf("%s: %s\n", cmd, rodbus_param_error_to_string(err));
    }
    else {
        printf("%s: success\n", cmd);
    }
}

// read one line into the buffer without its line ending, or return false at the end of input;
// a line longer than the buffer is truncated and the rest skipped, so it stays one command
bool read_line(char *line, size_t size)
{
    if (!fgets(line, (int)size, stdin)) {
        return false;
    }
    if (!strchr(line, '\n')) {
        int c = 0;
        while ((c = getchar()) != EOF && c != '\n') {
            // skip the rest of the line
        }
    }
    line[strcspn(line, "\r\n")] = '\0';
    return true;
}

// split "host:port" at the last ':', since the bindings take the host and port separately
bool split_endpoint(const char *endpoint, char *host, size_t host_size, uint16_t *port)
{
    const char *colon = strrchr(endpoint, ':');
    if (!colon || (size_t)(colon - endpoint) >= host_size) {
        return false;
    }

    char *end = NULL;
    unsigned long value = strtoul(colon + 1, &end, 10);
    if (end == colon + 1 || *end != '\0' || value > UINT16_MAX) {
        return false;
    }

    memcpy(host, endpoint, (size_t)(colon - endpoint));
    host[colon - endpoint] = '\0';
    *port = (uint16_t)value;
    return true;
}

// the default endpoint or serial port of a transport, or NULL if the transport is unknown
const char *default_argument(const char *transport)
{
    if (strcmp(transport, "tcp") == 0) {
        return TCP_ENDPOINT;
    }
    else if (strcmp(transport, "rtu") == 0) {
        return SERIAL_PORT;
    }
    else if (strcmp(transport, "tls-ca") == 0 || strcmp(transport, "tls-self-signed") == 0) {
        return TLS_ENDPOINT;
    }
    else {
        return NULL;
    }
}

void print_usage(void)
{
    fprintf(stderr, "%s\n", USAGE);
}
