#include <rodbus/rodbus.h>

#include <stdio.h>
#include <unistd.h>

#define COUNT 10

void log_callback(Level level, const char* msg)
{
    printf("%d - %s \n", level, msg);
}

int main() {

  int ret = 0;
  Runtime *runtime = NULL;
  Channel *channel = NULL;

  set_max_level(LEVEL_TRACE);
  set_log_callback(log_callback);

  runtime = create_threaded_runtime(NULL);

  if (!runtime) {
    printf("unable to initialize runtime \n");
    ret = -1;
    goto cleanup;
  }

  channel = create_tcp_client(runtime, "127.0.0.1:40000", 10);

  if (!channel) {
    printf("unable to initialize channel \n");
    ret = -1;
    goto cleanup;
  }

  Session session = build_session(runtime, channel, 1, 1000);

  bool values[COUNT];

  // every 5 seconds, perform a blocking read operation
  for (int i = 0; i < 3; ++i) {
    Result result = read_coils(&session, 0, COUNT, &values);
    switch (result.status) {
    case (STATUS_OK): {
      printf("success!\n");
      for (uintptr_t i = 0; i < COUNT; ++i) {
        printf("value: %d\n", values[i]);
      }
      break;
    }
    case (STATUS_EXCEPTION):
      printf("Modbus exception: %d\n", result.exception);
      break;
    default:
      printf("error: %d \n", result.status);
      break;
    }
    sleep(5);
  }

cleanup:

  // both of these check for NULL
  destroy_channel(channel);
  destroy_runtime(runtime);

  return ret;
}
