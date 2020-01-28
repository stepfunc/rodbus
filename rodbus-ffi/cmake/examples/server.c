#include <rodbus/rodbus.h>

#include <stdio.h>
#include <unistd.h>

#define NUM_VALUES 10

void log_callback(Level level, const char* msg)
{
    printf("%d - %s \n", level, msg);
}

void toggle_coils(Updater* updater, void* user_data) {
   bool value = *(bool*) user_data;
   for(uint16_t index=0; index < NUM_VALUES; ++index) {
      update_coil(updater, value, index);
   }
}

bool write_single_coil_handler(bool value, uint16_t index, void* userdata)
{
    return true;
}

bool write_single_register_handler(uint16_t value, uint16_t index, void* userdata)
{
    return true;
}

bool write_multiple_coils_handler(const bool* values, uint16_t count, uint16_t index, void* userdata)
{
    return true;
}

bool write_multiple_registers_handler(const uint16_t* values, uint16_t count, uint16_t index, void* userdata)
{
    return true;
}

int main() {

  int ret = 0;
  Runtime *runtime = NULL;
  Handler *handler = NULL;
  ServerHandle  *server = NULL;

  set_max_level(LEVEL_TRACE);
  set_log_callback(log_callback);

  runtime = create_threaded_runtime(NULL);

  if (!runtime) {
    printf("unable to initialize runtime \n");
    ret = -1;
    goto cleanup;
  }

  handler = create_handler(
            runtime,
            create_sizes(NUM_VALUES, NUM_VALUES, NUM_VALUES, NUM_VALUES),
            create_callbacks(
                    write_single_coil_handler,
                    write_single_register_handler,
                    write_multiple_coils_handler,
                    write_multiple_registers_handler
            ),
            NULL);

  server = create_server(runtime, "127.0.0.1:40000", 1, handler);

  if(!server) {
    printf("unable to create server \n");
    ret = -1;
    goto cleanup;
  }

  // every couple of seconds, toggle some coils
  bool value = true;
  while(true) {
     update_handler(handler, &value, toggle_coils);
     sleep(2);
     value = !value;
  }

cleanup:

  destroy_server(server);
  destroy_runtime(runtime);
  destroy_handler(handler);


  return ret;
}
