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
static void run_set(const char *label, ULONG attributes, ULONG access,
                    unsigned kind, LONGLONG due_value, bool period_present,
                    LONGLONG period_value, ULONG version, ULONG reserved,
                    LONGLONG tolerance, bool parameters_present = true,
                    bool due_present = true) {
  DWORD before = handles();
  LONG created = 0, result = 0, cancel = 0, state = 0;
  ULONG exception = 0;
  {
    Handle owner(kind, attributes, access);
    created = owner.created;
    LARGE_INTEGER due{}, period{};
    due.QuadPart = due_value;
    period.QuadPart = period_value;
    Parameters parameters{version, reserved, tolerance};
    HANDLE handle = selected(owner, kind);
    result = invoke_set(handle, due_present ? &due : nullptr,
                        period_present ? &period : nullptr,
                        parameters_present ? &parameters : nullptr, &exception);
    state = poll(handle);
    cancel = cancel2(handle, nullptr);
  }
  DWORD after = handles();
  std::printf(
      "SET case=%u label=%s attributes=%08lX access=%08lX kind=%u "
      "due-present=%u due=%016llX period-present=%u period=%016llX "
      "params-present=%u version=%08lX reserved=%08lX "
      "tolerance=%016llX create=%08lX status=%08lX exception=%08lX poll=%08lX "
      "cancel=%08lX handles-before=%lu handles-after=%lu\n",
      ordinal++, label, attributes, access, kind, unsigned(due_present),
      static_cast<unsigned long long>(due_value), unsigned(period_present),
      static_cast<unsigned long long>(period_value),
      unsigned(parameters_present), version, reserved,
      static_cast<unsigned long long>(tolerance), code(created), code(result),
      exception, code(state), code(cancel), before, after);
  if (before != after)
    ExitProcess(7);
}
static void run_cancel(ULONG attributes, ULONG access, unsigned kind,
                       uintptr_t parameter) {
  DWORD before = handles();
  LONG created = 0, arm = 0, result = 0, state = 0;
  ULONG exception = 0;
  {
    Handle owner(kind, attributes, access);
    created = owner.created;
    HANDLE handle = selected(owner, kind);
    LARGE_INTEGER due{};
    due.QuadPart = -3000000000LL;
    Parameters params{};
    arm = set2(handle, &due, nullptr, &params);
    result =
        invoke_cancel(handle, reinterpret_cast<PVOID>(parameter), &exception);
    state = poll(handle);
  }
  DWORD after = handles();
  std::printf(
      "CANCEL case=%u attributes=%08lX access=%08lX kind=%u "
      "parameter=%016llX create=%08lX arm=%08lX status=%08lX exception=%08lX "
      "poll=%08lX handles-before=%lu handles-after=%lu\n",
      ordinal++, attributes, access, kind,
      static_cast<unsigned long long>(parameter), code(created), code(arm),
      code(result), exception, code(state), before, after);
  if (before != after)
    ExitProcess(8);
}
static void transitions(ULONG attributes, bool periodic) {
  DWORD before = handles();
  std::array<LONG, 9> states{};
  {
    Handle owner(0, attributes, 0x1f0003);
    if (owner.created < 0)
      ExitProcess(9);
    LARGE_INTEGER due{}, period{};
    due.QuadPart = -10000;    // 1 ms.
    period.QuadPart = 100000; // 10 ms, independent from the one-shot arm.
    Parameters params{};
    states[0] = set2(owner.value, &due, periodic ? &period : nullptr, &params);
    states[1] = bounded_wait(owner.value);
    states[2] = poll(owner.value);
    states[3] = cancel2(owner.value, nullptr);
    states[4] = poll(owner.value);
    due.QuadPart = -3000000000LL;
    states[5] = set2(owner.value, &due, nullptr, &params);
    states[6] = poll(owner.value);
    states[7] = cancel2(owner.value, nullptr);
    states[8] = poll(owner.value);
  }
  DWORD after = handles();
  std::printf("STATE case=%u attributes=%08lX periodic=%u", ordinal++,
              attributes, unsigned(periodic));
  for (unsigned i = 0; i < states.size(); ++i)
    std::printf(" s%u=%08lX", i, code(states[i]));
  std::printf(" handles-before=%lu handles-after=%lu\n", before, after);
  if (before != after)
    ExitProcess(10);
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
    return 11;
  std::printf("PROFILE %s POINTER_BYTES %zu PARAMETERS_BYTES %zu\n",
              argc > 1 ? argv[1] : "unknown", sizeof(void *),
              sizeof(Parameters));
  const std::array<ULONG, 8> attributes{0,          4,          8, 0x80000000,
                                        0x80000004, 0x80000008, 2, 0x80000002};
  for (ULONG attr : attributes) {
    for (LONGLONG due :
         {std::numeric_limits<LONGLONG>::min(), -3000000000LL, -1LL, 0LL, 1LL,
          3000000000LL, std::numeric_limits<LONGLONG>::max()}) {
      run_set("due-null-period", attr, 0x1f0003, 0, due, false, 0, 0, 0, 0);
      for (LONGLONG period :
           {std::numeric_limits<LONGLONG>::min(), -1LL, 0LL, 1LL, 10000LL,
            750000000LL, std::numeric_limits<LONGLONG>::max()})
        run_set("period", attr, 0x1f0003, 0, due, true, period, 0, 0, 0);
    }
  }
  for (ULONG attr : {8ul, 0x80000008ul}) {
    for (ULONG version : {0ul, 1ul, 2ul, 0xfffffffful})
      for (ULONG reserved : {0ul, 1ul, 2ul, 0xfffffffful})
        for (LONGLONG tolerance :
             {std::numeric_limits<LONGLONG>::min(), -2LL, -1LL, 0LL, 1LL,
              750000000LL, std::numeric_limits<LONGLONG>::max()})
          run_set("parameters", attr, 0x1f0003, 0, -3000000000LL, false, 0,
                  version, reserved, tolerance);
    run_set("parameters-null", attr, 0x1f0003, 0, -3000000000LL, false, 0, 0, 0,
            0, false);
  }
  for (ULONG access :
       {0ul, 1ul, 2ul, 3ul, 4ul, 0x20000ul, 0x100000ul, 0x100002ul, 0x1f0003ul,
        0x02000000ul, 0x80000000ul, 0x40000000ul, 0x20000000ul, 0x10000000ul}) {
    for (ULONG version : {0ul, 1ul}) {
      run_set("access", 8, access, 0, -3000000000LL, false, 0, version, 0, 0);
      run_cancel(8, access, 0, 0);
    }
  }
  for (unsigned kind = 0; kind < 8; ++kind) {
    for (ULONG version : {0ul, 1ul})
      for (bool params : {false, true})
        for (bool due : {false, true})
          run_set("type-order", 8, 0x1f0003, kind, -3000000000LL, false, 0,
                  version, 0, 0, params, due);
    for (uintptr_t parameter :
         {uintptr_t(0), uintptr_t(1), uintptr_t(4), uintptr_t(~uintptr_t(0))})
      run_cancel(8, 0x1f0003, kind, parameter);
  }
  for (ULONG attr : attributes)
    for (bool periodic : {false, true})
      transitions(attr, periodic);
  std::printf("SUMMARY cases=%u\n", ordinal);
  return 0;
}
