// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

// Client example for Rodbus library
//
// usage: client <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]
//
// Run it from the repository root so the TLS certificate paths resolve.

#include "rodbus.h"

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define USAGE "usage: client <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]"
#define UNIT_ID 1
#define TCP_ENDPOINT "127.0.0.1:502"
#define TLS_ENDPOINT "127.0.0.1:802"
#define SERIAL_PORT "/dev/ttySIM0"
#define MAX_QUEUED_REQUESTS 10
#define REQUEST_TIMEOUT 1000 // milliseconds

// size of the buffers for a console line and for the host of an endpoint
#define LINE_SIZE 256

// ANCHOR: read_context
// what a read callback prints: the command, then each value under this name
typedef struct read_context_t {
    const char *cmd;
    const char *name;
} read_context_t;
// ANCHOR_END: read_context

rodbus_client_channel_t *create_channel(rodbus_runtime_t *runtime, const char *transport, const char *argument);
rodbus_client_channel_t *create_tcp_channel(rodbus_runtime_t *runtime, const char *host, uint16_t port);
rodbus_client_channel_t *create_rtu_channel(rodbus_runtime_t *runtime, const char *path);
rodbus_client_channel_t *create_tls_channel(rodbus_runtime_t *runtime, const char *host, uint16_t port, rodbus_tls_client_config_t tls_config);
rodbus_tls_client_config_t get_ca_chain_config(void);
rodbus_tls_client_config_t get_self_signed_config(void);
int run_channel(rodbus_client_channel_t *channel);
void print_help(void);
void run_command(rodbus_client_channel_t *channel, rodbus_request_param_t param, const char *cmd);
rodbus_logger_t get_logger(void);
rodbus_client_state_listener_t get_client_state_listener(void);
rodbus_port_state_listener_t get_port_state_listener(void);
rodbus_bit_read_callback_t get_bit_read_callback(const read_context_t *context);
rodbus_register_read_callback_t get_register_read_callback(const read_context_t *context);
rodbus_write_callback_t get_write_callback(const char *cmd);
void print_requested(const char *cmd, rodbus_param_error_t err);
void print_if_error(const char *cmd, rodbus_param_error_t err);
const char *client_state_name(rodbus_client_state_t state);
const char *port_state_name(rodbus_port_state_t state);
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

    // ANCHOR: logging_init
    // initialize logging with the default configuration
    rodbus_param_error_t logging_err = rodbus_configure_logging(rodbus_logging_config_init(), get_logger());
    if (logging_err) {
        printf("unable to configure logging: %s\n", rodbus_param_error_to_string(logging_err));
        return EXIT_FAILURE;
    }
    // ANCHOR_END: logging_init

    // ANCHOR: runtime_init
    rodbus_runtime_t *runtime = NULL;
    rodbus_runtime_config_t runtime_config = rodbus_runtime_config_init();
    runtime_config.num_core_threads = 4;
    rodbus_param_error_t err = rodbus_runtime_create(runtime_config, &runtime);
    if (err) {
        printf("unable to create runtime: %s\n", rodbus_param_error_to_string(err));
        return EXIT_FAILURE;
    }
    // ANCHOR_END: runtime_init

    int result = EXIT_FAILURE;
    rodbus_client_channel_t *channel = create_channel(runtime, transport, argument);
    if (channel) {
        result = run_channel(channel);
        rodbus_client_channel_destroy(channel);
    }

    // ANCHOR: runtime_destroy
    rodbus_runtime_destroy(runtime);
    // ANCHOR_END: runtime_destroy

    return result;
}

// create the channel for the transport, or print the error and return NULL
rodbus_client_channel_t *create_channel(rodbus_runtime_t *runtime, const char *transport, const char *argument)
{
    if (strcmp(transport, "rtu") == 0) {
        return create_rtu_channel(runtime, argument);
    }

    // the other transports connect to an endpoint
    char host[LINE_SIZE];
    uint16_t port = 0;
    if (!split_endpoint(argument, host, sizeof(host), &port)) {
        printf("invalid endpoint: %s\n", argument);
        return NULL;
    }

    if (strcmp(transport, "tcp") == 0) {
        return create_tcp_channel(runtime, host, port);
    }
    else if (strcmp(transport, "tls-ca") == 0) {
        return create_tls_channel(runtime, host, port, get_ca_chain_config());
    }
    else {
        // main only accepts known transports, so this is tls-self-signed
        return create_tls_channel(runtime, host, port, get_self_signed_config());
    }
}

rodbus_client_channel_t *create_tcp_channel(rodbus_runtime_t *runtime, const char *host, uint16_t port)
{
    // ANCHOR: create_tcp_channel
    rodbus_client_channel_t *channel = NULL;
    rodbus_decode_level_t decode_level = rodbus_decode_level_build(RODBUS_APP_DECODE_LEVEL_DATA_HEADERS, RODBUS_FRAME_DECODE_LEVEL_NOTHING, RODBUS_PHYS_DECODE_LEVEL_NOTHING);
    rodbus_param_error_t err = rodbus_client_channel_create_tcp(runtime,
        host,                         // host
        port,                         // port
        MAX_QUEUED_REQUESTS,          // max queued requests
        rodbus_retry_strategy_init(), // retry delays
        decode_level,                 // decode level
        get_client_state_listener(),  // listener
        &channel
    );
    if (err) {
        printf("unable to create channel: %s\n", rodbus_param_error_to_string(err));
        return NULL;
    }
    // ANCHOR_END: create_tcp_channel

    return channel;
}

rodbus_client_channel_t *create_rtu_channel(rodbus_runtime_t *runtime, const char *path)
{
    // ANCHOR: create_rtu_channel
    rodbus_client_channel_t *channel = NULL;
    rodbus_decode_level_t decode_level = rodbus_decode_level_build(RODBUS_APP_DECODE_LEVEL_DATA_HEADERS, RODBUS_FRAME_DECODE_LEVEL_NOTHING, RODBUS_PHYS_DECODE_LEVEL_NOTHING);
    rodbus_param_error_t err = rodbus_client_channel_create_rtu(runtime,
        path,                               // path
        rodbus_serial_port_settings_init(), // serial settings
        MAX_QUEUED_REQUESTS,                // max queued requests
        rodbus_retry_strategy_init(),       // retry delays
        decode_level,                       // decode level
        get_port_state_listener(),          // listener
        &channel
    );
    if (err) {
        printf("unable to create channel: %s\n", rodbus_param_error_to_string(err));
        return NULL;
    }
    // ANCHOR_END: create_rtu_channel

    return channel;
}

rodbus_client_channel_t *create_tls_channel(rodbus_runtime_t *runtime, const char *host, uint16_t port, rodbus_tls_client_config_t tls_config)
{
    // ANCHOR: create_tls_channel
    rodbus_client_channel_t *channel = NULL;
    rodbus_decode_level_t decode_level = rodbus_decode_level_build(RODBUS_APP_DECODE_LEVEL_DATA_HEADERS, RODBUS_FRAME_DECODE_LEVEL_NOTHING, RODBUS_PHYS_DECODE_LEVEL_NOTHING);
    rodbus_param_error_t err = rodbus_client_channel_create_tls(runtime,
        host,                         // host
        port,                         // port
        MAX_QUEUED_REQUESTS,          // max queued requests
        rodbus_retry_strategy_init(), // retry delays
        tls_config,                   // TLS configuration
        decode_level,                 // decode level
        get_client_state_listener(),  // listener
        &channel
    );
    if (err) {
        printf("unable to create channel: %s\n", rodbus_param_error_to_string(err));
        return NULL;
    }
    // ANCHOR_END: create_tls_channel

    return channel;
}

rodbus_tls_client_config_t get_ca_chain_config(void)
{
    // ANCHOR: tls_ca_chain_config
    rodbus_tls_client_config_t tls_config = rodbus_tls_client_config_init(
        "test.com",                          // name expected in the server certificate
        "./certs/ca_chain/ca_cert.pem",      // CA certificate
        "./certs/ca_chain/client_cert.pem",  // client certificate
        "./certs/ca_chain/client_key.pem",   // client private key
        ""                                   // no password
    );
    tls_config.min_tls_version = RODBUS_MIN_TLS_VERSION_V12;
    // ANCHOR_END: tls_ca_chain_config

    return tls_config;
}

rodbus_tls_client_config_t get_self_signed_config(void)
{
    // ANCHOR: tls_self_signed_config
    rodbus_tls_client_config_t tls_config = rodbus_tls_client_config_init(
        "test.com",                              // not checked in self-signed mode
        "./certs/self_signed/entity2_cert.pem",  // server certificate
        "./certs/self_signed/entity1_cert.pem",  // client certificate
        "./certs/self_signed/entity1_key.pem",   // client private key
        ""                                       // no password
    );
    tls_config.min_tls_version = RODBUS_MIN_TLS_VERSION_V12;
    tls_config.certificate_mode = RODBUS_CERTIFICATE_MODE_SELF_SIGNED;
    // ANCHOR_END: tls_self_signed_config

    return tls_config;
}

// enable the channel and run the console until "x" or the end of input
int run_channel(rodbus_client_channel_t *channel)
{
    rodbus_param_error_t err = rodbus_client_channel_enable(channel);
    if (err) {
        printf("unable to enable channel: %s\n", rodbus_param_error_to_string(err));
        return EXIT_FAILURE;
    }

    // ANCHOR: request_param
    rodbus_request_param_t param = rodbus_request_param_init(UNIT_ID, REQUEST_TIMEOUT);
    // ANCHOR_END: request_param

    print_help();

    char line[LINE_SIZE];
    while (read_line(line, sizeof(line))) {
        if (strcmp(line, "x") == 0) {
            return EXIT_SUCCESS;
        }
        else if (strcmp(line, "help") == 0) {
            print_help();
        }
        else {
            run_command(channel, param, line);
        }
    }
    return EXIT_SUCCESS;
}

void print_help(void)
{
    printf("Commands:\n");
    printf("  help - print this list\n");
    printf("  x - exit\n");
    printf("  ec - enable the channel\n");
    printf("  dc - disable the channel\n");
    printf("  ed - enable protocol decoding\n");
    printf("  dd - disable protocol decoding\n");
    printf("  rc - read coils 0 to 4\n");
    printf("  rdi - read discrete inputs 0 to 4\n");
    printf("  rhr - read holding registers 0 to 4\n");
    printf("  rir - read input registers 0 to 4\n");
    printf("  wsc - write coil 0 = true\n");
    printf("  wsr - write holding register 0 = 76\n");
    printf("  wmc - write coils 0 and 1 = true, false\n");
    printf("  wmr - write holding registers 0 and 1 = 0xCA, 0xFE\n");
}

// start one command: the request callbacks print the results
void run_command(rodbus_client_channel_t *channel, rodbus_request_param_t param, const char *cmd)
{
    // ANCHOR: address_range
    rodbus_address_range_t range = rodbus_address_range_init(0, // start
                                                             5  // count
    );
    // ANCHOR_END: address_range

    // the settings commands only queue the change; the listener shows the channel's state
    if (strcmp(cmd, "ec") == 0) {
        print_requested(cmd, rodbus_client_channel_enable(channel));
    }
    else if (strcmp(cmd, "dc") == 0) {
        print_requested(cmd, rodbus_client_channel_disable(channel));
    }
    else if (strcmp(cmd, "ed") == 0) {
        rodbus_decode_level_t level = rodbus_decode_level_build(RODBUS_APP_DECODE_LEVEL_DATA_VALUES, RODBUS_FRAME_DECODE_LEVEL_HEADER, RODBUS_PHYS_DECODE_LEVEL_LENGTH);
        print_requested(cmd, rodbus_client_channel_set_decode_level(channel, level));
    }
    else if (strcmp(cmd, "dd") == 0) {
        print_requested(cmd, rodbus_client_channel_set_decode_level(channel, rodbus_decode_level_nothing()));
    }
    else if (strcmp(cmd, "rc") == 0) {
        // ANCHOR: read_coils
        static const read_context_t context = {"rc", "coil"}; // static, so it outlives the request
        rodbus_param_error_t err = rodbus_client_channel_read_coils(channel, param, range, get_bit_read_callback(&context));
        // ANCHOR_END: read_coils
        print_if_error(cmd, err);
    }
    else if (strcmp(cmd, "rdi") == 0) {
        static const read_context_t context = {"rdi", "discrete input"};
        rodbus_param_error_t err = rodbus_client_channel_read_discrete_inputs(channel, param, range, get_bit_read_callback(&context));
        print_if_error(cmd, err);
    }
    else if (strcmp(cmd, "rhr") == 0) {
        static const read_context_t context = {"rhr", "holding register"};
        rodbus_param_error_t err = rodbus_client_channel_read_holding_registers(channel, param, range, get_register_read_callback(&context));
        print_if_error(cmd, err);
    }
    else if (strcmp(cmd, "rir") == 0) {
        static const read_context_t context = {"rir", "input register"};
        rodbus_param_error_t err = rodbus_client_channel_read_input_registers(channel, param, range, get_register_read_callback(&context));
        print_if_error(cmd, err);
    }
    else if (strcmp(cmd, "wsc") == 0) {
        // ANCHOR: write_single_coil
        rodbus_bit_value_t bit_value = rodbus_bit_value_init(0, true);
        rodbus_param_error_t err = rodbus_client_channel_write_single_coil(channel, param, bit_value, get_write_callback("wsc"));
        // ANCHOR_END: write_single_coil
        print_if_error(cmd, err);
    }
    else if (strcmp(cmd, "wsr") == 0) {
        rodbus_register_value_t register_value = rodbus_register_value_init(0, 76);
        rodbus_param_error_t err = rodbus_client_channel_write_single_register(channel, param, register_value, get_write_callback("wsr"));
        print_if_error(cmd, err);
    }
    else if (strcmp(cmd, "wmc") == 0) {
        rodbus_bit_list_t *bit_list = rodbus_bit_list_create(2);
        rodbus_bit_list_add(bit_list, true);
        rodbus_bit_list_add(bit_list, false);

        rodbus_param_error_t err = rodbus_client_channel_write_multiple_coils(channel, param, 0, bit_list, get_write_callback("wmc"));
        rodbus_bit_list_destroy(bit_list);
        print_if_error(cmd, err);
    }
    else if (strcmp(cmd, "wmr") == 0) {
        // ANCHOR: write_multiple_registers
        rodbus_register_list_t *register_list = rodbus_register_list_create(2);
        rodbus_register_list_add(register_list, 0xCA);
        rodbus_register_list_add(register_list, 0xFE);

        // the request copies the list, so it can be destroyed right away
        rodbus_param_error_t err = rodbus_client_channel_write_multiple_registers(channel, param, 0, register_list, get_write_callback("wmr"));
        rodbus_register_list_destroy(register_list);
        // ANCHOR_END: write_multiple_registers
        print_if_error(cmd, err);
    }
    else {
        printf("%s: unknown command, type help for a list\n", cmd);
    }
}

// ANCHOR: logging_interface
// callback which will receive log messages: each one already ends in a newline
void on_log_message(rodbus_log_level_t level, const char *message, void *ctx)
{
    printf("%s", message);
}

rodbus_logger_t get_logger(void)
{
    return (rodbus_logger_t){
        .on_message = &on_log_message,
        .on_destroy = NULL, // no context to free
        .ctx = NULL,
    };
}
// ANCHOR_END: logging_interface

void on_client_state_change(rodbus_client_state_t state, void *ctx)
{
    printf("client state: %s\n", client_state_name(state));
}

rodbus_client_state_listener_t get_client_state_listener(void)
{
    return (rodbus_client_state_listener_t){
        .on_change = &on_client_state_change,
        .on_destroy = NULL,
        .ctx = NULL,
    };
}

void on_port_state_change(rodbus_port_state_t state, void *ctx)
{
    printf("port state: %s\n", port_state_name(state));
}

rodbus_port_state_listener_t get_port_state_listener(void)
{
    return (rodbus_port_state_listener_t){
        .on_change = &on_port_state_change,
        .on_destroy = NULL,
        .ctx = NULL,
    };
}

// ANCHOR: bit_read_callback
void on_read_bits_complete(rodbus_bit_value_iterator_t *bits, void *ctx)
{
    const read_context_t *context = (const read_context_t *)ctx;
    printf("%s: success\n", context->cmd);
    rodbus_bit_value_t *bit = NULL;
    while ((bit = rodbus_bit_value_iterator_next(bits))) {
        printf("%s %u: %s\n", context->name, bit->index, bit->value ? "true" : "false");
    }
}

void on_read_bits_failure(rodbus_request_error_t error, void *ctx)
{
    const read_context_t *context = (const read_context_t *)ctx;
    printf("%s: %s\n", context->cmd, rodbus_request_error_to_string(error));
}
// ANCHOR_END: bit_read_callback

// ANCHOR: bit_read_callback_init
rodbus_bit_read_callback_t get_bit_read_callback(const read_context_t *context)
{
    return (rodbus_bit_read_callback_t){
        .on_complete = &on_read_bits_complete,
        .on_failure = &on_read_bits_failure,
        .on_destroy = NULL,     // the context is static, nothing to free
        .ctx = (void *)context, // passed to each callback
    };
}
// ANCHOR_END: bit_read_callback_init

void on_read_registers_complete(rodbus_register_value_iterator_t *registers, void *ctx)
{
    const read_context_t *context = (const read_context_t *)ctx;
    printf("%s: success\n", context->cmd);
    rodbus_register_value_t *reg = NULL;
    while ((reg = rodbus_register_value_iterator_next(registers))) {
        printf("%s %u: %u\n", context->name, reg->index, reg->value);
    }
}

void on_read_registers_failure(rodbus_request_error_t error, void *ctx)
{
    const read_context_t *context = (const read_context_t *)ctx;
    printf("%s: %s\n", context->cmd, rodbus_request_error_to_string(error));
}

rodbus_register_read_callback_t get_register_read_callback(const read_context_t *context)
{
    return (rodbus_register_read_callback_t){
        .on_complete = &on_read_registers_complete,
        .on_failure = &on_read_registers_failure,
        .on_destroy = NULL,
        .ctx = (void *)context,
    };
}

// ANCHOR: write_callback
void on_write_complete(rodbus_nothing_t nothing, void *ctx)
{
    printf("%s: success\n", (const char *)ctx);
}

void on_write_failure(rodbus_request_error_t error, void *ctx)
{
    printf("%s: %s\n", (const char *)ctx, rodbus_request_error_to_string(error));
}
// ANCHOR_END: write_callback

// ANCHOR: write_callback_init
// the command must outlive the request, like a string literal
rodbus_write_callback_t get_write_callback(const char *cmd)
{
    return (rodbus_write_callback_t){
        .on_complete = &on_write_complete,
        .on_failure = &on_write_failure,
        .on_destroy = NULL, // nothing to free
        .ctx = (void *)cmd, // passed to each callback
    };
}
// ANCHOR_END: write_callback_init

void print_requested(const char *cmd, rodbus_param_error_t err)
{
    if (err) {
        printf("%s: %s\n", cmd, rodbus_param_error_to_string(err));
    }
    else {
        printf("%s: requested\n", cmd);
    }
}

// print the error of a request that couldn't be queued
void print_if_error(const char *cmd, rodbus_param_error_t err)
{
    if (err) {
        printf("%s: %s\n", cmd, rodbus_param_error_to_string(err));
    }
}

const char *client_state_name(rodbus_client_state_t state)
{
    switch (state) {
    case RODBUS_CLIENT_STATE_DISABLED:
        return "disabled";
    case RODBUS_CLIENT_STATE_CONNECTING:
        return "connecting";
    case RODBUS_CLIENT_STATE_CONNECTED:
        return "connected";
    case RODBUS_CLIENT_STATE_WAIT_AFTER_FAILED_CONNECT:
        return "connect failed, waiting to retry";
    case RODBUS_CLIENT_STATE_WAIT_AFTER_DISCONNECT:
        return "disconnected, waiting to reconnect";
    case RODBUS_CLIENT_STATE_SHUTDOWN:
        return "shutdown";
    default:
        return "unknown";
    }
}

const char *port_state_name(rodbus_port_state_t state)
{
    switch (state) {
    case RODBUS_PORT_STATE_DISABLED:
        return "disabled";
    case RODBUS_PORT_STATE_WAIT:
        return "open failed, waiting to retry";
    case RODBUS_PORT_STATE_OPEN:
        return "open";
    case RODBUS_PORT_STATE_SHUTDOWN:
        return "shutdown";
    default:
        return "unknown";
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
