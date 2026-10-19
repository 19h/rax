#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <limits>
#include <windows.h>
#include <winternl.h>

struct Parameters {
  ULONG version;
  ULONG reserved;
  LONGLONG tolerance;
};
static_assert(sizeof(Parameters) == 16);
using Create2 = LONG(NTAPI *)(PHANDLE, PULONG, POBJECT_ATTRIBUTES, ULONG,
                              ACCESS_MASK);
using Set2 = LONG(NTAPI *)(HANDLE, const LARGE_INTEGER *, const LARGE_INTEGER *,
                           const Parameters *);
using Cancel2 = LONG(NTAPI *)(HANDLE, PVOID);
using Wait = LONG(NTAPI *)(HANDLE, BOOLEAN, const LARGE_INTEGER *);
using Close = LONG(NTAPI *)(HANDLE);
static Create2 create2;
static Set2 set2;
static Cancel2 cancel2;
static Wait wait_one;
static Close close_object;
static unsigned ordinal;
static LONG invoke_set(HANDLE handle, const LARGE_INTEGER *due,
                       const LARGE_INTEGER *period,
                       const Parameters *parameters, ULONG *exception) {
  *exception = 0;
  LONG result = static_cast<LONG>(0xDEADBEEF);
  __try {
    result = set2(handle, due, period, parameters);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
  }
  return result;
}
static LONG invoke_cancel(HANDLE handle, PVOID parameters, ULONG *exception) {
  *exception = 0;
  LONG result = static_cast<LONG>(0xDEADBEEF);
  __try {
    result = cancel2(handle, parameters);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
  }
  return result;
}

static ULONG code(LONG status) { return static_cast<ULONG>(status); }
static DWORD handles() {
  DWORD count = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &count))
    ExitProcess(2);
  return count;
}
static LONG poll(HANDLE handle) {
  LARGE_INTEGER zero{};
  return wait_one(handle, FALSE, &zero);
}
static LONG bounded_wait(HANDLE handle) {
  LARGE_INTEGER timeout{};
  timeout.QuadPart = -20000000; // At most 2 s; own anonymous timer only.
  return wait_one(handle, FALSE, &timeout);
}
struct Handle {
  HANDLE value = nullptr;
  LONG created = 0;
  bool extended = false;
  Handle(unsigned kind, ULONG attributes, ULONG access) {
    if (kind == 0) {
      ULONG id = 1;
      created = create2(&value, (attributes & 2) ? &id : nullptr, nullptr,
                        attributes, access);
      extended = created >= 0;
    } else if (kind == 1) {
      value = CreateWaitableTimerW(nullptr, FALSE, nullptr);
      if (!value)
        ExitProcess(3);
    } else if (kind == 2) {
      value = CreateEventW(nullptr, FALSE, FALSE, nullptr);
      if (!value)
        ExitProcess(4);
    } else if (kind == 3) {
      value = CreateMutexW(nullptr, FALSE, nullptr);
      if (!value)
        ExitProcess(5);
    }
  }
  ~Handle() {
    if (value) {
      if (extended)
        cancel2(value, nullptr);
      if (close_object(value) < 0)
        ExitProcess(6);
    }
  }
};
static HANDLE selected(Handle &owner, unsigned kind) {
  switch (kind) {
  case 4:
    return nullptr;
  case 5:
    return reinterpret_cast<HANDLE>(static_cast<uintptr_t>(0x7fffffff));
  case 6:
    return GetCurrentProcess();
  case 7:
    return GetCurrentThread();
  default:
    return owner.value;
  }
}
static LONG wait_for(HANDLE h, LONGLONG ticks) {
  LARGE_INTEGER timeout{};
  timeout.QuadPart = ticks;
  return wait_one(h, FALSE, &timeout);
}
static LONGLONG counter() {
  LARGE_INTEGER value{};
  if (!QueryPerformanceCounter(&value))
    ExitProcess(40);
  return value.QuadPart;
}
static void measure(ULONG attributes, LONGLONG interval, LONGLONG frequency,
                    unsigned sample) {
  DWORD before = handles();
  {
    Handle timer(0, attributes, 0x1f0003);
    if (timer.created < 0)
      ExitProcess(41);
    LARGE_INTEGER due{}, period{};
    due.QuadPart = -10000;
    period.QuadPart = interval;
    Parameters params{};
    LONG armed = set2(timer.value, &due, &period, &params);
    LONG first =
        armed >= 0 ? wait_for(timer.value, -20000000) : LONG(0xdeadbeef);
    std::printf("TIMING case=%u sample=%u attributes=%08lX period=%016llX "
                "set=%08lX first=%08lX",
                ordinal++, sample, attributes,
                static_cast<unsigned long long>(interval), code(armed),
                code(first));
    for (unsigned index = 0; index < 3; ++index) {
      LONGLONG start = counter();
      LONG waited = wait_for(timer.value, -3000000); // At most 300 ms.
      LONGLONG end = counter();
      LONGLONG elapsed_100ns = (end - start) * 10000000 / frequency;
      std::printf(" w%u=%08lX t%u-100ns=%lld", index, code(waited), index,
                  elapsed_100ns);
    }
    LONG cancelled = cancel2(timer.value, nullptr);
    std::printf(" cancel=%08lX\n", code(cancelled));
  }
  DWORD after = handles();
  if (before != after)
    ExitProcess(42);
}
int main(int argc, char **argv) {
  HMODULE module = GetModuleHandleW(L"ntdll.dll");
  create2 = reinterpret_cast<Create2>(GetProcAddress(module, "NtCreateTimer2"));
  set2 = reinterpret_cast<Set2>(GetProcAddress(module, "NtSetTimer2"));
  cancel2 = reinterpret_cast<Cancel2>(GetProcAddress(module, "NtCancelTimer2"));
  wait_one =
      reinterpret_cast<Wait>(GetProcAddress(module, "NtWaitForSingleObject"));
  close_object = reinterpret_cast<Close>(GetProcAddress(module, "NtClose"));
  if (!create2 || !set2 || !cancel2 || !wait_one || !close_object)
    return 43;
  LARGE_INTEGER frequency{};
  if (!QueryPerformanceFrequency(&frequency))
    return 44;
  std::printf("PROFILE %s POINTER_BYTES %zu QPC_HZ %lld\n",
              argc > 1 ? argv[1] : "unknown", sizeof(void *),
              frequency.QuadPart);
  for (unsigned sample = 0; sample < 2; ++sample)
    for (ULONG attributes : {8ul, 4ul})
      for (LONGLONG period : {100000LL, -100000LL, 1000000LL, -1000000LL,
                              std::numeric_limits<LONGLONG>::min(),
                              std::numeric_limits<LONGLONG>::max()})
        measure(attributes, period, frequency.QuadPart, sample);
  std::printf("SUMMARY cases=%u\n", ordinal);
  return 0;
}
