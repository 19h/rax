#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <winternl.h>

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>

using Query = LONG(NTAPI *)(LARGE_INTEGER *);
using LastStatus = LONG(NTAPI *)();

struct Result {
  LONG status = LONG(0x99999999);
  DWORD exception = 0;
  ULONG_PTR access = ~ULONG_PTR(0);
  intptr_t fault_offset = -1;
  bool returned = false;
};

static int capture(EXCEPTION_POINTERS *exception, unsigned char *scratch,
                   Result *result) {
  const auto *record = exception->ExceptionRecord;
  result->exception = record->ExceptionCode;
  if (record->NumberParameters >= 2) {
    result->access = record->ExceptionInformation[0];
    const auto address = record->ExceptionInformation[1];
    const auto base = reinterpret_cast<uintptr_t>(scratch);
    if (address >= base && address - base < 24576)
      result->fault_offset = intptr_t(address - base);
  }
  return EXCEPTION_EXECUTE_HANDLER;
}

static Result invoke(Query query, void *output, unsigned char *scratch) {
  Result result;
  __try {
    result.status = query(static_cast<LARGE_INTEGER *>(output));
    result.returned = true;
  } __except (capture(GetExceptionInformation(), scratch, &result)) {
  }
  return result;
}

static void run(Query query, LastStatus last_status, unsigned ordinal,
                const char *name, intptr_t offset) {
  auto *scratch = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 24576, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!scratch)
    ExitProcess(90);
  memset(scratch, 0xA5, 24576);
  DWORD old = 0;
  if (!VirtualProtect(scratch + 4096, 4096, PAGE_READONLY, &old) ||
      !VirtualProtect(scratch + 8192, 4096, PAGE_NOACCESS, &old) ||
      !VirtualProtect(scratch + 12288, 4096, PAGE_READWRITE | PAGE_GUARD,
                      &old) ||
      !VirtualProtect(scratch + 20480, 4096, PAGE_READWRITE | PAGE_GUARD, &old))
    ExitProcess(91);
  void *output = offset == -1   ? nullptr
                 : offset == -2 ? reinterpret_cast<void *>(uintptr_t(1))
                 : offset == -3 ? reinterpret_cast<void *>(~uintptr_t(0))
                                : scratch + offset;
  LARGE_INTEGER before{}, after{};
  if (query(&before) < 0)
    ExitProcess(92);
  SetLastError(0x4A2E19F7);
  LONG status_before = last_status();
  Result result = invoke(query, output, scratch);
  DWORD error_after = GetLastError();
  LONG status_after = last_status();
  if (query(&after) < 0)
    ExitProcess(93);
  MEMORY_BASIC_INFORMATION guard1{}, guard2{};
  if (VirtualQuery(scratch + 12288, &guard1, sizeof guard1) != sizeof guard1 ||
      VirtualQuery(scratch + 20480, &guard2, sizeof guard2) != sizeof guard2)
    ExitProcess(94);
  // Only the two ordinary writable pages are inspected. Querying the guard
  // pages above cannot consume a guard; no guarded byte is read by this probe.
  printf("{\"kind\":\"case\",\"ordinal\":%u,\"name\":\"%s\","
         "\"returned\":%s,\"status\":\"%08lX\",\"exception\":\"%08lX\","
         "\"access\":%llu,\"fault_offset\":%lld,\"before\":%lld,\"after\":%lld,"
         "\"last_error\":%lu,\"last_status_before\":\"%08lX\","
         "\"last_status_after\":\"%08lX\",\"guard1\":%lu,\"guard2\":%lu,"
         "\"changes\":[",
         ordinal, name, result.returned ? "true" : "false",
         static_cast<unsigned long>(result.status), result.exception,
         static_cast<unsigned long long>(result.access),
         static_cast<long long>(result.fault_offset), before.QuadPart,
         after.QuadPart, error_after, static_cast<unsigned long>(status_before),
         static_cast<unsigned long>(status_after), guard1.Protect,
         guard2.Protect);
  bool comma = false;
  for (size_t page : {size_t(0), size_t(16384)}) {
    for (size_t i = page; i < page + 4096; ++i) {
      if (scratch[i] != 0xA5) {
        printf("%s[%zu,%u]", comma ? "," : "", i, unsigned(scratch[i]));
        comma = true;
      }
    }
  }
  printf("]}\n");
  if (!VirtualFree(scratch, 0, MEM_RELEASE))
    ExitProcess(95);
}

int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto module = GetModuleHandleW(L"ntdll.dll");
  auto query =
      reinterpret_cast<Query>(GetProcAddress(module, "NtQuerySystemTime"));
  auto last_status = reinterpret_cast<LastStatus>(
      GetProcAddress(module, "RtlGetLastNtStatus"));
  using Version = LONG(NTAPI *)(PRTL_OSVERSIONINFOW);
  auto version =
      reinterpret_cast<Version>(GetProcAddress(module, "RtlGetVersion"));
  RTL_OSVERSIONINFOW os{};
  os.dwOSVersionInfoSize = sizeof os;
  if (!query || !last_status || !version || version(&os) < 0)
    return 96;
  const auto *base = reinterpret_cast<const unsigned char *>(module);
  const auto *dos = reinterpret_cast<const IMAGE_DOS_HEADER *>(base);
  if (dos->e_magic != IMAGE_DOS_SIGNATURE || dos->e_lfanew < 0 ||
      dos->e_lfanew > 4096 - LONG(sizeof(IMAGE_NT_HEADERS)))
    return 97;
  const auto *nt =
      reinterpret_cast<const IMAGE_NT_HEADERS *>(base + dos->e_lfanew);
  auto entry = reinterpret_cast<uintptr_t>(query);
  auto start = reinterpret_cast<uintptr_t>(base);
  if (nt->Signature != IMAGE_NT_SIGNATURE ||
      nt->OptionalHeader.SizeOfImage < 4096 ||
      nt->OptionalHeader.SizeOfImage > (64U << 20) || entry < start ||
      entry - start > nt->OptionalHeader.SizeOfImage - 48)
    return 98;
  printf("{\"kind\":\"host\",\"pointer_bytes\":%zu,\"build\":%lu,"
         "\"ntdll_machine\":%u,\"query_rva\":%llu,\"query_hex\":\"",
         sizeof(void *), os.dwBuildNumber, unsigned(nt->FileHeader.Machine),
         static_cast<unsigned long long>(entry - start));
  const auto *leaf = reinterpret_cast<const unsigned char *>(entry);
  for (unsigned i = 0; i < 48; ++i)
    printf("%02X", unsigned(leaf[i]));
  puts("\"}");
  struct Case {
    const char *name;
    intptr_t offset;
  };
  const Case cases[] = {
      {"aligned8", 256},
      {"aligned4", 260},
      {"unaligned1", 257},
      {"unaligned2", 258},
      {"unaligned3", 259},
      {"null", -1},
      {"one", -2},
      {"top", -3},
      {"readonly", 4352},
      {"noaccess", 8448},
      {"split_readonly4", 4092},
      {"split_readonly1", 4095},
      {"guard", 12544},
      {"split_guard4", 20476},
      {"last_valid8", 4088},
      {"unaligned_guard", 12545},
  };
  unsigned ordinal = 0;
  for (const auto &row : cases)
    run(query, last_status, ordinal++, row.name, row.offset);
  printf("{\"kind\":\"complete\",\"cases\":%u}\n", ordinal);
  return 0;
}
