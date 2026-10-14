#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <windows.h>
#include <winternl.h>

using SetProcess = LONG(NTAPI *)(HANDLE, ULONG, PVOID, ULONG);
__declspec(thread) volatile ULONG tls_canary = 0x12345678;
static constexpr unsigned case_count = 44;

static void fail(const char *operation) {
  printf("failure=%s error=%lu\n", operation, GetLastError());
  ExitProcess(21);
}

static uintptr_t tls_vector() {
  uintptr_t value = 0;
  auto teb = reinterpret_cast<unsigned char *>(NtCurrentTeb());
  memcpy(&value, teb + (sizeof(void *) == 8 ? 0x58 : 0x2C), sizeof value);
  return value;
}

static void print_bytes(const char *name, const void *address, SIZE_T length) {
  unsigned char bytes[128]{};
  SIZE_T got = 0;
  BOOL ok =
      ReadProcessMemory(GetCurrentProcess(), address, bytes, length, &got);
  DWORD error = ok ? 0 : GetLastError();
  printf(
      "bytes name=%s address=%llX requested=%zu ok=%u error=%lu got=%zu hex=",
      name, static_cast<unsigned long long>(uintptr_t(address)), length,
      unsigned(ok), error, got);
  for (SIZE_T i = 0; i < got; ++i)
    printf("%02X", bytes[i]);
  puts("");
}

static LONG invoke(SetProcess set, HANDLE process, void *input, ULONG length,
                   ULONG *escaped) {
  __try {
    return set(process, 35, input, length);
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}

static void child(unsigned ordinal) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto set = reinterpret_cast<SetProcess>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtSetInformationProcess"));
  if (!set)
    fail("export");
  SYSTEM_INFO system{};
  GetSystemInfo(&system);
  SIZE_T page = system.dwPageSize;
  auto arena = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, page * 4, MEM_RESERVE, PAGE_NOACCESS));
  if (!arena || !VirtualAlloc(arena, page * 2, MEM_COMMIT, PAGE_READWRITE))
    fail("owned-input-arena");
  memset(arena, 0xA5, page * 2);
  auto new_vector = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, page, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  auto second_vector = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, page, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!new_vector || !second_vector)
    fail("owned-vector-arena");
  uintptr_t before_vector = tls_vector();
  SIZE_T got = 0;
  if (!ReadProcessMemory(GetCurrentProcess(),
                         reinterpret_cast<void *>(before_vector), new_vector,
                         64, &got) ||
      got != 64)
    fail("copy-old-vector");
  memcpy(second_vector, new_vector, 64);
  // Compact layout derived from the captured installed kernel, not PHNT.
  // Four ULONGs followed by entries of ULONG, pointer padding, and two
  // pointers.
  SIZE_T stride = sizeof(void *) == 8 ? 24 : 12;
  SIZE_T pointer_offset = sizeof(void *);
  unsigned char record[128]{};
  ULONG operation = ordinal == 1 || ordinal == 4 || ordinal == 6 ||
                            ordinal == 15 || ordinal == 16 || ordinal == 17
                        ? 1
                        : 0;
  ULONG count = ordinal == 3 || ordinal == 4   ? 0
                : ordinal == 5 || ordinal == 6 ? 2
                                               : 1;
  ULONG index = operation == 1 ? 0 : 2;
  ULONG flags = ordinal == 7   ? 1
                : ordinal == 8 ? 2
                : ordinal == 9 ? 0xFFFFFFFF
                               : 0;
  ULONG entry_flags = ordinal == 10   ? 1
                      : ordinal == 11 ? 2
                      : ordinal == 12 ? 4
                      : ordinal == 13 ? 0xFFFFFFFF
                                      : 0;
  if (ordinal == 2)
    operation = 2;
  if (ordinal == 16)
    index = 1;
  if (ordinal == 17)
    index = 16;
  if (ordinal == 18)
    index = 0xFFFFFFFF;
  if (ordinal == 19)
    index = 0;
  if (ordinal == 43)
    count = 0xFFFFFFFF;
  memcpy(record, &flags, 4);
  memcpy(record + 4, &operation, 4);
  memcpy(record + 8, &count, 4);
  memcpy(record + 12, &index, 4);
  for (unsigned i = 0; i < 2; ++i) {
    SIZE_T at = 16 + i * stride;
    uintptr_t next = uintptr_t(i ? second_vector : new_vector);
    if (ordinal == 14 || ordinal == 15)
      next = 0;
    memcpy(record + at, &entry_flags, 4);
    memcpy(record + at + pointer_offset, &next, sizeof next);
  }
  ULONG length = ULONG(16 + (count <= 2 ? count : 1) * stride);
  if (ordinal == 36)
    length = 0;
  if (ordinal == 37)
    length = 15;
  if (ordinal == 38)
    length = 16;
  if (ordinal == 39)
    length -= 1;
  if (ordinal == 40)
    length += 1;
  if (ordinal == 41)
    length = sizeof(void *) == 8 ? 56 : 36;
  if (ordinal == 42)
    length = 0xFFFFFFFF;
  unsigned char *input = arena + 128;
  if (ordinal == 30)
    input += 1;
  if (ordinal == 31)
    input += 4;
  if (ordinal == 34)
    input = arena + page * 2 - 20;
  SIZE_T available = SIZE_T(arena + page * 2 - input);
  memcpy(input, record, available < 128 ? available : 128);
  HANDLE process = GetCurrentProcess();
  if (ordinal == 21)
    process = nullptr;
  if (ordinal == 22)
    process = reinterpret_cast<HANDLE>(intptr_t(-2));
  if (ordinal == 23)
    process = reinterpret_cast<HANDLE>(uintptr_t(0x13572468));
  if (ordinal >= 24 && ordinal <= 27) {
    const DWORD rights[] = {0, PROCESS_SET_INFORMATION, PROCESS_VM_WRITE,
                            PROCESS_ALL_ACCESS};
    if (!DuplicateHandle(GetCurrentProcess(), GetCurrentProcess(),
                         GetCurrentProcess(), &process, rights[ordinal - 24],
                         FALSE, 0))
      fail("self-process-rights");
  }
  if (ordinal == 28)
    input = nullptr;
  if (ordinal == 29)
    input = reinterpret_cast<unsigned char *>(uintptr_t(1));
  if (ordinal == 35)
    input = arena + page * 2;
  print_bytes("input-before", input, 64);
  print_bytes("vector-before", reinterpret_cast<void *>(before_vector), 64);
  DWORD old = 0;
  if (ordinal == 32 || ordinal == 33) {
    DWORD protection =
        ordinal == 32 ? PAGE_READONLY : PAGE_READWRITE | PAGE_GUARD;
    if (!VirtualProtect(arena, page, protection, &old))
      fail("protect-owned-input");
  }
  printf("case=%u width=%zu process=%llX operation=%lu count=%lu index=%lu "
         "flags=%08lX entry-flags=%08lX input=%llX length=%lu new=%llX "
         "second=%llX before-vector=%llX thread=%lu canary=%08lX\n",
         ordinal, sizeof(void *),
         static_cast<unsigned long long>(uintptr_t(process)), operation, count,
         index, flags, entry_flags,
         static_cast<unsigned long long>(uintptr_t(input)), length,
         static_cast<unsigned long long>(uintptr_t(new_vector)),
         static_cast<unsigned long long>(uintptr_t(second_vector)),
         static_cast<unsigned long long>(before_vector), GetCurrentThreadId(),
         tls_canary);
  ULONG escaped = 0;
  LONG result = invoke(set, process, input, length, &escaped);
  uintptr_t after_vector = tls_vector();
  MEMORY_BASIC_INFORMATION query{};
  if (!VirtualQuery(arena, &query, sizeof query))
    fail("query-owned-input");
  printf("result case=%u status=%08lX escaped=%08lX after-vector=%llX "
         "input-protect=%08lX\n",
         ordinal, ULONG(result), escaped,
         static_cast<unsigned long long>(after_vector), query.Protect);
  print_bytes("input-after", input, 64);
  print_bytes("vector-after", reinterpret_cast<void *>(after_vector), 64);
  ExitProcess(77);
}

int main(int argc, char **argv) {
  if (argc == 2)
    child(unsigned(strtoul(argv[1], nullptr, 10)));
  setvbuf(stdout, nullptr, _IONBF, 0);
  wchar_t executable[32768];
  if (!GetModuleFileNameW(nullptr, executable, 32768))
    fail("executable-path");
  unsigned failures = 0;
  for (unsigned ordinal = 0; ordinal < case_count; ++ordinal) {
    std::wstring command =
        L"\"" + std::wstring(executable) + L"\" " + std::to_wstring(ordinal);
    STARTUPINFOW startup{};
    startup.cb = sizeof startup;
    PROCESS_INFORMATION process{};
    if (!CreateProcessW(executable, command.data(), nullptr, nullptr, TRUE, 0,
                        nullptr, nullptr, &startup, &process))
      fail("create-owned-case");
    DWORD wait = WaitForSingleObject(process.hProcess, 60000);
    if (wait != WAIT_OBJECT_0) {
      TerminateProcess(process.hProcess, 78);
      fail("owned-case-deadline");
    }
    DWORD exit_code = 0;
    if (!GetExitCodeProcess(process.hProcess, &exit_code))
      fail("case-exit");
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);
    printf("child-exit case=%u code=%lu\n", ordinal, exit_code);
    failures += exit_code != 77;
  }
  printf("complete cases=%u child-failures=%u width=%zu\n", case_count,
         failures, sizeof(void *));
  return failures ? 22 : 0;
}
