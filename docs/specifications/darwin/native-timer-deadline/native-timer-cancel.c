#include <inttypes.h>
#include <mach/mach.h>
#include <mach/mach_time.h>
#include <stdint.h>
#include <stdio.h>

extern mach_port_name_t mk_timer_create(void);
extern kern_return_t mk_timer_destroy(mach_port_name_t);
extern kern_return_t mk_timer_arm_leeway(mach_port_name_t, uint64_t, uint64_t,
                                         uint64_t);
extern kern_return_t mk_timer_cancel(mach_port_name_t, uint64_t *);

int main(void) {
  mach_timebase_info_data_t tb;
  if (mach_timebase_info(&tb) != KERN_SUCCESS || !tb.numer || !tb.denom)
    return 91;
  mach_port_name_t timer = mk_timer_create();
  if (!timer)
    return 92;
  printf("{\"kind\":\"timebase\",\"numer\":%u,\"denom\":%u}\n", tb.numer,
         tb.denom);
  uint64_t ten_seconds =
      (uint64_t)(((__uint128_t)10000000000 * tb.denom) / tb.numer);
  uint64_t sixty_seconds = ten_seconds * 6;
  unsigned errors = 0;
  for (unsigned i = 0; i < 512; ++i) {
    uint64_t first = mach_absolute_time() + ten_seconds;
    kern_return_t arm1 = mk_timer_arm_leeway(timer, 1, first, 0);
    uint64_t deadline = mach_absolute_time() + sixty_seconds + i;
    kern_return_t arm2 = mk_timer_arm_leeway(timer, 1, deadline, 0);
    uint64_t returned = 0;
    kern_return_t cancel = mk_timer_cancel(timer, &returned);
    int64_t delta = (int64_t)returned - (int64_t)deadline;
    printf("{\"kind\":\"case\",\"ordinal\":%u,\"arm1\":%d,\"arm2\":%d,"
           "\"cancel\":%d,\"deadline\":%" PRIu64 ",\"returned\":%" PRIu64
           ",\"delta_ticks\":%" PRId64 "}\n",
           i, arm1, arm2, cancel, deadline, returned, delta);
    errors += arm1 != KERN_SUCCESS || arm2 != KERN_SUCCESS ||
              cancel != KERN_SUCCESS || returned == 0;
  }
  kern_return_t destroyed = mk_timer_destroy(timer);
  printf("{\"kind\":\"complete\",\"cases\":512,\"errors\":%u,\"destroy\":%d}\n",
         errors, destroyed);
  return errors || destroyed != KERN_SUCCESS ? 93 : 0;
}
