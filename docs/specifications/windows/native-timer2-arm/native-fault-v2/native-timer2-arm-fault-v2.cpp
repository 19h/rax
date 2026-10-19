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
static ULONG page;
struct Input {
  unsigned char *base;
  void *pointer;
  unsigned mode;
  Input(unsigned selected_mode, const void *data, size_t bytes)
      : mode(selected_mode) {
    base = static_cast<unsigned char *>(VirtualAlloc(
        nullptr, 3 * page, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
    if (!base)
      ExitProcess(20);
    std::memset(base, 0xA5, 3 * page);
    auto *target = base + 128;
    if (mode == 6)
      target += 1;
    if (mode == 7)
      target += 2;
    if (mode == 8)
      target += 4;
    if (mode == 14)
      target += 7;
    const unsigned prefixes[] = {1, 4, 7, 8, 15};
    if (mode >= 9 && mode <= 13)
      target = base + page - prefixes[mode - 9];
    if (mode == 15)
      target = base + page - 16;
    std::memcpy(target, data, bytes);
    pointer = target;
    if (mode == 1)
      pointer = nullptr;
    if (mode == 2)
      pointer = reinterpret_cast<void *>(uintptr_t(1));
    if (mode == 16)
      pointer = reinterpret_cast<void *>(~uintptr_t(0));
    if (mode == 3)
      protect(0, PAGE_NOACCESS);
    if (mode == 4)
      protect(0, PAGE_READWRITE | PAGE_GUARD);
    if (mode == 5)
      protect(0, PAGE_READONLY);
    if ((mode >= 9 && mode <= 13) || mode == 15)
      protect(page, PAGE_NOACCESS);
  }
  void protect(unsigned offset, DWORD flags) {
    DWORD previous = 0;
    if (!VirtualProtect(base + offset, page, flags, &previous))
      ExitProcess(21);
  }
  ULONG guard() const {
    MEMORY_BASIC_INFORMATION data{};
    if (!VirtualQuery(base, &data, sizeof(data)))
      ExitProcess(22);
    return data.Protect & PAGE_GUARD;
  }
  ~Input() {
    if (!VirtualFree(base, 0, MEM_RELEASE))
      ExitProcess(23);
  }
};
static void fault_set(unsigned kind, ULONG access, unsigned due_mode,
                      unsigned period_mode, unsigned parameter_mode,
                      ULONG version) {
  DWORD before = handles();
  {
    Handle owner(kind, 8, access);
    LONGLONG due = -3000000000LL, period = 100000;
    Parameters parameters{version, 0, 0};
    Input d(due_mode, &due, sizeof due), p(period_mode, &period, sizeof period),
        a(parameter_mode, &parameters, sizeof parameters);
    unsigned repeats =
        (due_mode == 4 || period_mode == 4 || parameter_mode == 4) ? 2 : 1;
    for (unsigned repeat = 0; repeat < repeats; ++repeat) {
      ULONG dg = d.guard(), pg = p.guard(), ag = a.guard(), exception = 0;
      LONG result = invoke_set(
          selected(owner, kind), static_cast<LARGE_INTEGER *>(d.pointer),
          static_cast<LARGE_INTEGER *>(p.pointer),
          static_cast<Parameters *>(a.pointer), &exception);
      std::printf(
          "FAULT case=%u kind=%u access=%08lX due-mode=%u period-mode=%u "
          "params-mode=%u version=%08lX repeat=%u status=%08lX exception=%08lX "
          "due-guard-before=%lu due-guard-after=%lu period-guard-before=%lu "
          "period-guard-after=%lu params-guard-before=%lu "
          "params-guard-after=%lu\n",
          ordinal++, kind, access, due_mode, period_mode, parameter_mode,
          version, repeat, code(result), exception, dg, d.guard(), pg,
          p.guard(), ag, a.guard());
      if (owner.extended)
        cancel2(owner.value, nullptr);
    }
  }
  DWORD after = handles();
  if (before != after) {
    std::printf("LEAK before=%lu after=%lu\n", before, after);
    ExitProcess(24);
  }
}
static void fault_cancel(unsigned kind, ULONG access, unsigned mode) {
  DWORD before = handles();
  {
    Handle owner(kind, 8, access);
    Parameters data{};
    Input parameter(mode, &data, sizeof data);
    ULONG exception = 0, guard = parameter.guard();
    LONG result =
        invoke_cancel(selected(owner, kind), parameter.pointer, &exception);
    std::printf(
        "CANCEL-FAULT case=%u kind=%u access=%08lX params-mode=%u status=%08lX "
        "exception=%08lX guard-before=%lu guard-after=%lu\n",
        ordinal++, kind, access, mode, code(result), exception, guard,
        parameter.guard());
  }
  DWORD after = handles();
  if (before != after)
    ExitProcess(25);
}
int main(int argc, char **argv) {
  SYSTEM_INFO info{};
  GetSystemInfo(&info);
  page = info.dwPageSize;
  HMODULE module = GetModuleHandleW(L"ntdll.dll");
  create2 = reinterpret_cast<Create2>(GetProcAddress(module, "NtCreateTimer2"));
  set2 = reinterpret_cast<Set2>(GetProcAddress(module, "NtSetTimer2"));
  cancel2 = reinterpret_cast<Cancel2>(GetProcAddress(module, "NtCancelTimer2"));
  wait_one =
      reinterpret_cast<Wait>(GetProcAddress(module, "NtWaitForSingleObject"));
  close_object = reinterpret_cast<Close>(GetProcAddress(module, "NtClose"));
  if (!create2 || !set2 || !cancel2 || !wait_one || !close_object)
    return 26;
  DWORD initial_handles = handles();
  std::printf(
      "PROFILE %s POINTER_BYTES %zu PAGE_BYTES %lu PARAMETERS_BYTES %zu\n",
      argc > 1 ? argv[1] : "unknown", sizeof(void *), page, sizeof(Parameters));
  for (unsigned kind : {0u, 1u, 2u, 5u, 6u, 7u})
    for (ULONG version : {0ul, 1ul})
      for (unsigned mode = 0; mode <= 16; ++mode) {
        fault_set(kind, 0x1f0003, mode, 0, 0, version);
        fault_set(kind, 0x1f0003, 0, mode, 0, version);
        fault_set(kind, 0x1f0003, 0, 0, mode, version);
      }
  const std::array<unsigned, 8> modes{0, 1, 2, 3, 4, 6, 9, 12};
  for (unsigned kind : {0u, 5u})
    for (ULONG version : {0ul, 1ul})
      for (unsigned d : modes)
        for (unsigned p : modes)
          for (unsigned a : modes)
            fault_set(kind, 0x1f0003, d, p, a, version);
  for (ULONG access : {0ul, 1ul, 2ul})
    for (unsigned mode = 0; mode <= 16; ++mode)
      for (unsigned axis = 0; axis < 3; ++axis)
        fault_set(0, access, axis == 0 ? mode : 0, axis == 1 ? mode : 0,
                  axis == 2 ? mode : 0, 0);
  for (unsigned kind = 0; kind < 8; ++kind)
    for (ULONG access : {1ul, 0x1f0003ul})
      for (unsigned mode = 0; mode <= 16; ++mode)
        fault_cancel(kind, access, mode);
  DWORD final_handles = handles();
  std::printf("SUMMARY cases=%u handles-before=%lu handles-after=%lu\n",
              ordinal, initial_handles, final_handles);
  return initial_handles == final_handles ? 0 : 27;
}
