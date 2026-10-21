#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <intrin.h>
#include <windows.h>

__declspec(noinline) static unsigned __int64 read_current_el() {
  return _ReadStatusReg(ARM64_SYSREG(3, 0, 4, 2, 2));
}

int main() {
  unsigned __int64 value = 0xA5A5A5A5A5A5A5A5ULL;
  unsigned long status = 0;
  unsigned calls = 0;
  __try {
    ++calls;
    value = read_current_el();
  } __except ((status = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
  }
  std::printf("current-el calls=%u status=%08lX value=%016llX\n", calls, status,
              value);
  return calls == 1 ? 0 : 10;
}
