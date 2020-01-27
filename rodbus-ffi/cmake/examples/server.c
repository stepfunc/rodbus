#include <rodbus/rodbus.h>

#include <stdio.h>
#include <unistd.h>

void log_callback(Level level, const char* msg)
{
    printf("%d - %s \n", level, msg);
}

void toggle_coils(Updater* updater, void* user_data) {
   bool value = *(bool*) user_data;
   for(uint16_t index=0; index < 10; ++index) {
      update_coil(updater, value, index);
   }
}

int main() {

  int ret = 0;
  Runtime *runtime = NULL;
  Handler *handler = NULL;

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
            create_sizes(10, 10, 10, 10),
            create_callbacks(NULL, NULL, NULL, NULL),
            NULL);

  bool result = create_server(runtime, "127.0.0.1:40000", 1, handler);

  if(!result) {
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


  destroy_runtime(runtime);
  destroy_handler(handler);


  return ret;
}
