#include <stdio.h>

#include <rodbus.h>

#ifdef __unix__
# include <unistd.h>
#elif defined _WIN32
# include <windows.h>
#define sleep(x) Sleep(1000 * (x))
#endif

void on_log_message(log_level_t level, const char* message, void* ctx) {
	printf("%s \n", message);
}

bool init_logging() {
	log_handler_t handler = {
		.on_message = &on_log_message,
		.on_destroy = NULL,
		.ctx = NULL
	};

	set_max_log_level(LogLevel_Info);

	return set_log_handler(handler);
}

write_result_t on_write_single_coil(bool value, uint16_t address, database_t* db, void* ctx)
{
	if (database_update_coil(db, address, value)) {		
		return write_result_success();
	}
	else {
		return write_result_exception(Exception_IllegalDataAddress);		
	}	
}


write_result_t on_write_single_register(uint16_t value, uint16_t address, database_t* db, void* ctx)
{
	if (database_update_holding_register(db, address, value)) {
		return write_result_success();
	}
	else {
		return write_result_exception(Exception_IllegalDataAddress);
	}
}


write_result_t on_write_multiple_coils(uint16_t start, bit_iterator_t* it, database_t* db, void* ctx)
{
	bit_t* bit = NULL;
	while (bit = next_bit(it))
	{
		if(!database_update_coil(db, bit->index, bit->value)) {
			return write_result_exception(Exception_IllegalDataAddress);
		}
	}
	return write_result_success();
}

write_result_t on_write_multiple_registers(uint16_t start, register_iterator_t* it, database_t* db, void* ctx)
{
	register_t* reg = NULL;
	while (reg = next_register(it))
	{
		if (!database_update_holding_register(db, reg->index, reg->value)) {
			return write_result_exception(Exception_IllegalDataAddress);
		}
	}
	return write_result_success();
}

write_handler_t get_write_handler()
{
	write_handler_t ret = {
		.write_single_coil = on_write_single_coil,
		.write_single_register = on_write_single_register,
		.write_multiple_coils = on_write_multiple_coils,
		.write_multiple_registers = on_write_multiple_registers,		
		.ctx = NULL,
		.destroy = NULL,
	};

	return ret;
}

typedef struct state_t {
	uint16_t register_value;
	bool bit_value;
} state_t;


void configure_db(database_t* db, void* ctx)
{
	for (uint16_t i = 0; i < 10; ++i) {
		database_add_coil(db, i, false);
		database_add_discrete_input(db, i, false);
		database_add_holding_register(db, i, false);
		database_add_input_register(db, i, false);
	}	
}

void update_db(database_t* db, void* ctx)
{
	state_t* state = (state_t*) ctx;

	state->bit_value = !state->bit_value;
	state->register_value = state->register_value + 1;

	for (uint16_t i = 0; i < 10; ++i) {
		database_update_discrete_input(db, i, state->bit_value);
		database_update_input_register(db, i, state->register_value);
	}
}

int main() {
	
	if (!init_logging()) {
		printf("Unable to initialize logging\n");
		return - 1;
	}
	
	runtime_t* runtime = NULL;
	server_t* server = NULL;

	runtime = runtime_new(NULL);
	if (!runtime) {
		printf("Unable to initialize runtime\n");
		goto cleanup;
	}

	device_map_t* map = create_device_map();
	map_add_endpoint(map, 1, get_write_handler(), (database_callback_t) {.callback = configure_db, .ctx = NULL});
	server = create_tcp_server(runtime, "127.0.0.1:502", map);
	destroy_device_map(map);	

	if (server == NULL) {
		printf("Unable to initialize server\n");
		goto cleanup;
	}

	state_t state = { .register_value = 0, .bit_value = false };
	
	while (true) {		
		server_update_database(server, 1, (database_callback_t) { .callback = update_db, .ctx = &state });
		sleep(1);
	}
	
cleanup:	
	destroy_server(server);
	runtime_destroy(runtime);

	return 0;
}

