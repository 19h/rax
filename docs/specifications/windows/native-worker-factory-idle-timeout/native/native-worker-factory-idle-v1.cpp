#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>
using Create = LONG(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES, HANDLE,
                             HANDLE, PVOID, PVOID, ULONG, SIZE_T, SIZE_T);
using Port = LONG(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES, ULONG);
using Set = LONG(NTAPI *)(HANDLE, ULONG, PVOID, ULONG);
using Query = LONG(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);
using Close = LONG(NTAPI *)(HANDLE);
static Create create_factory;
static Port create_port;
static Set set_factory;
static Query query_factory;
static Close close_object;
static DWORD page;
static unsigned ordinal;
static ULONG factory_maximum = 512;
static SIZE_T stack_reserve = SIZE_T(1) << 20;
static SIZE_T stack_commit = 4096;
static VOID NTAPI worker(PVOID) {}
static LONG invoke(HANDLE h, void *p, ULONG n, ULONG *exception) {
  LONG status = LONG(0xDEADBEEF);
  *exception = 0;
  __try {
    status = set_factory(h, 2, p, n);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
  }
  return status;
}
static ULONG protection(void *p) {
  MEMORY_BASIC_INFORMATION m{};
  return VirtualQuery(p, &m, sizeof m) ? m.Protect : 0;
}
static void run(const char *label, LONGLONG value, unsigned mode, ULONG length,
                unsigned handle_mode) {
  DWORD before = 0, after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    ExitProcess(2);
  HANDLE port = nullptr, factory = nullptr, alias = nullptr, event = nullptr;
  if (create_port(&port, 0x1F0003, nullptr, 0) < 0 ||
      create_factory(&factory, 0xF00FF, nullptr, port, GetCurrentProcess(),
                     reinterpret_cast<void *>(&worker), nullptr,
                     factory_maximum, stack_reserve, stack_commit) < 0)
    ExitProcess(3);
  ULONG flags = 1;
  if (set_factory(factory, 13, &flags, 4) < 0)
    ExitProcess(19);
  HANDLE h = factory;
  switch (handle_mode) {
  case 1:
    h = nullptr;
    break;
  case 2:
    h = reinterpret_cast<HANDLE>(uintptr_t(0x13572468));
    break;
  case 3:
  case 4:
    event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    if (!event)
      ExitProcess(4);
    h = event;
    if (handle_mode == 4) {
      if (!DuplicateHandle(GetCurrentProcess(), event, GetCurrentProcess(),
                           &alias, 0, FALSE, 0))
        ExitProcess(5);
      h = alias;
    }
    break;
  case 5:
  case 6:
  case 7:
  case 8:
  case 9:
  case 10: {
    ULONG access = handle_mode == 5    ? 0
                   : handle_mode == 10 ? 0xF00FF
                                       : 1u << (handle_mode - 6);
    if (!DuplicateHandle(GetCurrentProcess(), factory, GetCurrentProcess(),
                         &alias, access, FALSE, 0))
      ExitProcess(6);
    h = alias;
    break;
  }
  case 13:
  case 14:
  case 15:
  case 16:
  case 17: {
    ULONG access = handle_mode == 17 ? 0xC0 : 1u << (handle_mode - 9);
    if (!DuplicateHandle(GetCurrentProcess(), factory, GetCurrentProcess(),
                         &alias, access, FALSE, 0))
      ExitProcess(6);
    h = alias;
    break;
  }
  case 11:
    h = GetCurrentProcess();
    break;
  case 12:
    h = GetCurrentThread();
    break;
  }
  auto memory = static_cast<unsigned char *>(VirtualAlloc(
      nullptr, 2 * page, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE));
  if (!memory)
    ExitProcess(7);
  memset(memory, 0xA5, 2 * page);
  void *p = memory;
  if (mode == 1)
    p = nullptr;
  else if (mode == 2)
    p = reinterpret_cast<void *>(uintptr_t(1));
  else if (mode == 6)
    p = memory + 1;
  else if (mode == 7)
    p = memory + page - 8;
  else if (mode == 8)
    p = memory + page - 4;
  if (mode != 1 && mode != 2)
    memcpy(p, &value, sizeof value);
  DWORD old = 0;
  if (mode == 3 && !VirtualProtect(memory, page, PAGE_READONLY, &old))
    ExitProcess(8);
  if (mode == 4 && !VirtualProtect(memory, page, PAGE_NOACCESS, &old))
    ExitProcess(9);
  if (mode == 5 &&
      !VirtualProtect(memory, page, PAGE_READWRITE | PAGE_GUARD, &old))
    ExitProcess(10);
  if (mode == 8 && !VirtualProtect(memory + page, page, PAGE_NOACCESS, &old))
    ExitProcess(11);
  unsigned char original[128], updated[128];
  memset(original, 0xA5, sizeof original);
  memset(updated, 0xA5, sizeof updated);
  ULONG rb = 0, ra = 0;
  LONG qb =
      query_factory(factory, 7, original, sizeof(void *) == 8 ? 120 : 96, &rb);
  ULONG exception = 0;
  LONG status = invoke(h, p, length, &exception);
  ULONG protect = protection(memory);
  LONG qa =
      query_factory(factory, 7, updated, sizeof(void *) == 8 ? 120 : 96, &ra);
  ULONG maximum_before = 0, maximum_after = 0, total_before = 0,
        total_after = 0;
  memcpy(&maximum_before, original + 40, 4);
  memcpy(&maximum_after, updated + 40, 4);
  memcpy(&total_before, original + 52, 4);
  memcpy(&total_after, updated + 52, 4);
  if (alias && close_object(alias) < 0)
    ExitProcess(12);
  if (event && close_object(event) < 0)
    ExitProcess(13);
  if (close_object(factory) < 0 || close_object(port) < 0)
    ExitProcess(14);
  if (!VirtualFree(memory, 0, MEM_RELEASE))
    ExitProcess(15);
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    ExitProcess(16);
  printf(
      "case=%u label=%s width=%zu class=2 value=%016llX mode=%u length=%08lX "
      "handle-mode=%u status=%08lX exception=%08lX protection=%08lX "
      "query-before=%08lX returned-before=%lu maximum-before=%08lX "
      "total-before=%lu query-after=%08lX returned-after=%lu "
      "maximum-after=%08lX total-after=%lu handles-before=%lu "
      "handles-after=%lu before=",
      ordinal++, label, sizeof(void *), static_cast<unsigned long long>(value),
      mode, length, handle_mode, ULONG(status), exception, protect, ULONG(qb),
      rb, maximum_before, total_before, ULONG(qa), ra, maximum_after,
      total_after, before, after);
  for (auto b : original)
    printf("%02X", b);
  printf(" after=");
  for (auto b : updated)
    printf("%02X", b);
  puts("");
  if (qb < 0 || qa < 0 || before != after || total_before || total_after)
    ExitProcess(17);
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  SYSTEM_INFO info{};
  GetSystemInfo(&info);
  page = info.dwPageSize;
  auto n = GetModuleHandleW(L"ntdll.dll");
  create_factory =
      reinterpret_cast<Create>(GetProcAddress(n, "NtCreateWorkerFactory"));
  create_port =
      reinterpret_cast<Port>(GetProcAddress(n, "NtCreateIoCompletion"));
  set_factory =
      reinterpret_cast<Set>(GetProcAddress(n, "NtSetInformationWorkerFactory"));
  query_factory = reinterpret_cast<Query>(
      GetProcAddress(n, "NtQueryInformationWorkerFactory"));
  close_object = reinterpret_cast<Close>(GetProcAddress(n, "NtClose"));
  if (!create_factory || !create_port || !set_factory || !query_factory ||
      !close_object)
    return 18;
  for (unsigned sizing = 0; sizing < 2; ++sizing) {
    stack_reserve = sizing == 0 ? SIZE_T(1) << 20 : SIZE_T(1) << 19;
    stack_commit = sizing == 0 ? 4096 : 8192;
    for (LONGLONG value : {0ll, 1ll, -1ll, -10000000ll, -300000000ll,
                           10000000ll, 300000000ll, -2147483648ll, 2147483647ll,
                           4294967295ll, (-9223372036854775807ll - 1),
                           9223372036854775807ll, 4294967296ll, -4294967296ll})
      run("value", value, 0, 8, 0);
    for (unsigned mode = 0; mode < 9; ++mode)
      for (ULONG length :
           {0ul, 1ul, 3ul, 4ul, 7ul, 8ul, 9ul, 16ul, 128ul, 0xFFFFFFFFul})
        run("buffer", -300000000ll, mode, length, 0);
    for (unsigned handle = 0; handle < 18; ++handle)
      for (unsigned mode : {0u, 1u, 4u, 5u})
        for (ULONG length : {0ul, 8ul, 9ul})
          run("handle-order", -300000000ll, mode, length, handle);
  }
  printf("complete cases=%u\n", ordinal);
  return 0;
}
