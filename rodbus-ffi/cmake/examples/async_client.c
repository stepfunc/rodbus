#include <rodbus/rodbus.h>

#include <stdio.h>

#ifdef __unix__
# include <unistd.h>
#elif defined _WIN32
# include <windows.h>
#define sleep(x) Sleep(1000 * (x))
#endif

void log_callback(Level level, const char* msg)
{
    printf("%d - %s \n", level, msg);
}

void coils_callback(Result result, BitIterator* iterator, void *ctx) {
  switch (result.status) {
  case (STATUS_OK): {
    printf("success!\n");
    Bit bit;
    while(get_next_bit(iterator, &bit)) {
        printf("value: %d index: %d\n", bit.value, bit.index);
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

  channel = create_tcp_client(runtime, "127.0.0.1:502", 10);

  if (!channel) {
    printf("unable to initialize channel \n");
    ret = -1;
    goto cleanup;
  }

  Session session = build_session(runtime, channel, 1, 1000);

  // every 5 seconds, start a read operation
  for (int i = 0; i < 3; ++i) {
    read_coils_cb(&session, 0, 10, coils_callback, NULL);
    sleep(5);
  }

cleanup:

  // both of these check for NULL
  destroy_channel(channel);
  destroy_runtime(runtime);

  return ret;
}
