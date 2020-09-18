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

void on_read_bits_complete(bit_read_result_t bits, void* ctx) {
	switch (bits.result.summary) {
	case (Status_Ok): {
		printf("success!\n");		
		bit_t* bit = NULL;
		while (bit = next_bit(bits.iterator)) {
			printf("value: %d index: %d\n", bit->value, bit->index);
		}
		break;
	}
	case (Status_Exception):
		printf("Modbus exception: %d\n", bits.result.exception);
		break;
	default:
		printf("error: %s \n", Status_to_string(bits.result.summary));
		break;
	}
}

void on_read_registers_complete(register_read_result_t registers, void* ctx) {
	switch (registers.result.summary) {
	case (Status_Ok): {
		printf("success!\n");
		register_t* bit = NULL;
		while (bit = next_register(registers.iterator)) {
			printf("value: %d index: %d\n", bit->value, bit->index);
		}
		break;
	}
	case (Status_Exception):
		printf("Modbus exception: %d\n", registers.result.exception);
		break;
	default:
		printf("error: %s \n", Status_to_string(registers.result.summary));
		break;
	}
}

void on_write_complete(error_info_t result, void* ctx) {
	switch (result.summary) {
	case (Status_Ok): {
		printf("success!\n");		
		break;
	}
	case (Status_Exception):
		printf("Modbus exception: %d\n", result.exception);
		break;
	default:
		printf("error: %s \n", Status_to_string(result.summary));
		break;
	}
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

int main() {
	
	if (!init_logging()) {
		printf("Unable to initialize logging \n");
		return - 1;
	}
	
	runtime_t* runtime = NULL;	
	channel_t* channel = NULL;

	runtime = runtime_new(NULL);
	if (!runtime) {
		printf("Unable to initialize runtime \n");
		goto cleanup;
	}
	channel = create_tcp_client(runtime, "127.0.0.1:502", 100);
	if (!channel) {
		printf("Unable to initialize channel \n");
		goto cleanup;
	}

	request_param_t params = {
			.unit_id = 1,
			.timeout_ms = 1000,
	};

	for(int i=0; i<3; ++i) {
		bit_read_callback_t bit_callback = {
			.on_complete = on_read_bits_complete,
			.ctx = NULL,
		};
		register_read_callback_t register_callback = {
			.on_complete = on_read_registers_complete,
			.ctx = NULL,
		};
		result_callback_t result_callback = {
			.on_complete = on_write_complete,
			.ctx = NULL,
		};

		address_range_t range = {
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
		bit_list_t* bits = bit_list_create(0);
		bit_list_add(bits, true);
		bit_list_add(bits, false);
		channel_write_multiple_coils_async(channel, 0, bits, params, result_callback);
		bit_list_destroy(bits);
		sleep(1);

	}

cleanup:
	destroy_channel(channel);
	runtime_destroy(runtime);

	return 0;
}

