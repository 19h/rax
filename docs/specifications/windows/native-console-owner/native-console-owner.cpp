#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <windows.h>
#include <winternl.h>

using SetProcess = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG);
using QueryProcess = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
static constexpr unsigned case_count = 136;

static void fail(const char *operation) {
  printf("failure=%s error=%lu\n", operation, GetLastError());
  ExitProcess(21);
}

static LONG set_call(SetProcess set, HANDLE process, void *input, ULONG length,
                     ULONG *escaped) {
  __try {
    return set(process, 49, input, length);
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}

static LONG query_call(QueryProcess query, HANDLE process, void *output,
                       ULONG length, ULONG *returned, ULONG *escaped) {
  __try {
    return query(process, 49, output, length, returned);
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}

static void bytes(const char *name, const void *address, SIZE_T length) {
  BYTE data[32]{};
  SIZE_T got = 0;
  BOOL ok = ReadProcessMemory(GetCurrentProcess(), address, data, length, &got);
  printf(
      "bytes name=%s address=%llX requested=%zu ok=%u error=%lu got=%zu hex=",
      name, static_cast<unsigned long long>(uintptr_t(address)), length,
      unsigned(ok), ok ? 0 : GetLastError(), got);
  for (SIZE_T i = 0; i < got; ++i)
    printf("%02X", data[i]);
  printf(" cells=");
  for (SIZE_T i = 0; i < length; ++i) {
    BYTE cell = 0;
    SIZE_T cell_got = 0;
    if (ReadProcessMemory(
            GetCurrentProcess(),
            reinterpret_cast<const void *>(uintptr_t(address) + i), &cell, 1,
            &cell_got) &&
        cell_got == 1)
      printf("%02X", unsigned(cell));
    else
      printf("??");
  }
  puts("");
}

static void state(QueryProcess query, const char *phase) {
  uintptr_t value = uintptr_t(0xA5A5A5A5A5A5A5A5ULL);
  ULONG returned = 0xB6B6B6B6, escaped = 0;
  LONG status = query_call(query, GetCurrentProcess(), &value, sizeof value,
                           &returned, &escaped);
  printf(
      "state phase=%s status=%08lX escaped=%08lX value=%llX returned=%08lX\n",
      phase, ULONG(status), escaped, static_cast<unsigned long long>(value),
      returned);
}

static void child(unsigned ordinal) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto ntdll = GetModuleHandleW(L"ntdll.dll");
  auto set = reinterpret_cast<SetProcess>(
      GetProcAddress(ntdll, "NtSetInformationProcess"));
  auto query = reinterpret_cast<QueryProcess>(
      GetProcAddress(ntdll, "NtQueryInformationProcess"));
  if (!set || !query || ordinal >= case_count)
    fail("exports-or-case");
  SYSTEM_INFO system{};
  GetSystemInfo(&system);
  SIZE_T page = system.dwPageSize;
  auto arena = static_cast<BYTE *>(VirtualAlloc(
      nullptr, page * 6, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!arena)
    fail("owned-arena");
  memset(arena, 0xA5, page * 6);
  BYTE *buffer = arena + 128;
  auto returned = reinterpret_cast<ULONG *>(arena + page * 4 + 128);
  *returned = 0xB6B6B6B6;
  uintptr_t value = 1;
  HANDLE handle = GetCurrentProcess(), owned = nullptr;
  ULONG length = sizeof(uintptr_t);
  bool setting =
      ordinal < 48 || (ordinal >= 96 && ordinal < 120) || ordinal >= 128;
  unsigned item =
      ordinal >= 120 ? ordinal - 120 + 48 : (setting ? ordinal : ordinal - 48);
  if (item == 1)
    length = 0;
  if (item == 2)
    length -= 1;
  if (item == 3)
    length += 1;
  if (item == 4)
    length = 0xFFFFFFFF;
  if (item == 7)
    buffer += 1;
  if (item == 8)
    buffer += 4;
  if (item == 12 || (!setting && item == 43) || (setting && item == 13))
    buffer = arena + page * 2 - 2;
  if (setting) {
    if (item == 27)
      value = 0;
    if (item == 28)
      value = 2;
    if (item == 29)
      value = 3;
    if (item == 30)
      value = GetCurrentProcessId();
    if (item == 31)
      value = uintptr_t(GetCurrentProcessId()) | 1;
    if (item == 32)
      value = UINTPTR_MAX;
    if (item == 33)
      value = UINTPTR_MAX - 1;
  }
  if (ordinal >= 96 && ordinal < 120) {
    const uintptr_t values[] = {1,
                                5,
                                9,
                                13,
                                17,
                                0x12345679,
                                0xFFFFFFF9,
                                0xFFFFFFFD,
                                uintptr_t(0x100000001ULL),
                                uintptr_t(0x100000005ULL),
                                uintptr_t(0x7FFFFFFFFFFFFFFDULL),
                                uintptr_t(0xFFFFFFFFFFFFFFFDULL),
                                uintptr_t(GetCurrentProcessId()) | 1,
                                uintptr_t(GetCurrentProcessId()) | 2,
                                uintptr_t(GetCurrentProcessId()) | 3,
                                0x7FFFFFFD,
                                0x80000001,
                                0xFFFFFFF1,
                                1,
                                1,
                                1,
                                1,
                                1,
                                1};
    value = values[ordinal - 96];
  }
  if (setting)
    memcpy(buffer, &value, sizeof value);
  if (item == 5)
    buffer = nullptr;
  if (item == 6)
    buffer = reinterpret_cast<BYTE *>(uintptr_t(1));
  if ((setting && item == 14) || (!setting && (item == 13 || item == 44)))
    buffer = reinterpret_cast<BYTE *>(UINTPTR_MAX - 1);
  unsigned h = setting ? item : item + 1;
  if (h == 15)
    handle = nullptr;
  if (h == 16)
    handle = reinterpret_cast<HANDLE>(uintptr_t(0x13572468));
  if (h == 17)
    handle = GetCurrentThread();
  if (h == 18) {
    owned = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (!owned)
      fail("owned-event");
    handle = owned;
  }
  if (h >= 19 && h <= 25) {
    const DWORD rights[] = {0,
                            PROCESS_SET_INFORMATION,
                            0x2000,
                            PROCESS_QUERY_INFORMATION,
                            PROCESS_QUERY_LIMITED_INFORMATION,
                            PROCESS_VM_WRITE,
                            PROCESS_ALL_ACCESS};
    if (!DuplicateHandle(GetCurrentProcess(), GetCurrentProcess(),
                         GetCurrentProcess(), &owned, rights[h - 19], FALSE, 0))
      fail("self-handle-rights");
    handle = owned;
  }
  DWORD input_protect = PAGE_READWRITE, return_protect = PAGE_READWRITE;
  if (item == 9)
    input_protect = PAGE_READONLY;
  if (item == 10)
    input_protect = PAGE_NOACCESS;
  if (item == 11)
    input_protect = PAGE_READWRITE | PAGE_GUARD;
  if (setting) {
    if (item == 34) {
      buffer = nullptr;
      length = 0;
    }
    if (item == 35 || item == 36) {
      buffer = arena + 129;
      length = item == 35 ? 0 : sizeof(uintptr_t) - 1;
    }
    if (item == 37) {
      handle = nullptr;
      buffer = nullptr;
    }
    if (item >= 38 && item <= 42)
      handle = reinterpret_cast<HANDLE>(uintptr_t(0x13572468));
    if (item == 38)
      buffer = arena + 129;
    if (item == 39)
      input_protect = PAGE_READONLY;
    if (item == 40)
      input_protect = PAGE_NOACCESS;
    if (item == 41)
      length -= 1;
    if (item == 42)
      length = 0;
    if (item == 43 || item == 44 || item == 45)
      length -= 1;
    if (item == 43)
      input_protect = PAGE_READWRITE | PAGE_GUARD;
    if (item == 44)
      input_protect = PAGE_READONLY;
    if (item == 45)
      buffer = nullptr;
  } else {
    if (item == 25)
      returned = nullptr;
    if (item == 26)
      returned = reinterpret_cast<ULONG *>(arena + page * 4 + 129);
    if (item == 27)
      return_protect = PAGE_READONLY;
    if (item == 28 || item == 33 || item == 34 || item == 35 || item == 36)
      return_protect = PAGE_NOACCESS;
    if (item == 29)
      return_protect = PAGE_READWRITE | PAGE_GUARD;
    if (item == 30 || item == 42)
      returned = reinterpret_cast<ULONG *>(arena + page * 5 - 2);
    if (item == 31)
      returned = reinterpret_cast<ULONG *>(buffer);
    if (item == 32)
      returned = reinterpret_cast<ULONG *>(buffer + 4);
    if (item == 33 || item == 37 || item == 38)
      handle = reinterpret_cast<HANDLE>(uintptr_t(0x13572468));
    if (item == 34)
      length -= 1;
    if (item == 35)
      buffer = arena + 129;
    if (item == 36)
      length = 0;
    if (item == 37)
      input_protect = PAGE_READONLY;
    if (item == 38)
      input_protect = PAGE_NOACCESS;
    if (item == 39) {
      buffer = nullptr;
      length = 0;
    }
    if (item == 40) {
      length -= 1;
      input_protect = PAGE_READWRITE | PAGE_GUARD;
    }
    if (item == 41)
      returned = reinterpret_cast<ULONG *>(uintptr_t(1));
    if (item == 45)
      length = 8;
  }
  if (ordinal == 114)
    input_protect = PAGE_READONLY | PAGE_GUARD;
  if (ordinal == 115) {
    handle = nullptr;
    input_protect = PAGE_READONLY | PAGE_GUARD;
  }
  if (ordinal == 116) {
    handle = nullptr;
    input_protect = PAGE_READWRITE | PAGE_GUARD;
  }
  if (ordinal == 117) {
    buffer = reinterpret_cast<BYTE *>(UINTPTR_MAX - 7);
  }
  if (ordinal == 118) {
    length = sizeof(uintptr_t) * 2;
  }
  if (ordinal == 119) {
    length = 0;
    buffer = reinterpret_cast<BYTE *>(uintptr_t(1));
  }
  if (ordinal == 120)
    input_protect = PAGE_READONLY | PAGE_GUARD;
  if (ordinal == 121)
    return_protect = PAGE_READONLY | PAGE_GUARD;
  if (ordinal == 122) {
    input_protect = PAGE_READWRITE | PAGE_GUARD;
    return_protect = PAGE_READWRITE | PAGE_GUARD;
  }
  if (ordinal == 123) {
    length -= 1;
    return_protect = PAGE_READWRITE | PAGE_GUARD;
  }
  if (ordinal == 124) {
    handle = nullptr;
    return_protect = PAGE_READWRITE | PAGE_GUARD;
  }
  if (ordinal == 125) {
    buffer = arena + page * 2 - 4;
  }
  if (ordinal == 126) {
    buffer = arena + page * 2 - 4;
    return_protect = PAGE_READONLY;
  }
  if (ordinal == 127) {
    length = 0xFFFFFFFF;
    buffer = reinterpret_cast<BYTE *>(UINTPTR_MAX - 7);
  }
  if (ordinal == 128) {
    length = sizeof(uintptr_t) - 1;
    buffer = reinterpret_cast<BYTE *>(UINTPTR_MAX - 7);
  }
  if (ordinal == 129) {
    length = 0;
    buffer = reinterpret_cast<BYTE *>(UINTPTR_MAX - 7);
  }
  if (ordinal == 130) {
    length = sizeof(uintptr_t) * 2;
    buffer = reinterpret_cast<BYTE *>(UINTPTR_MAX - 7);
  }
  if (ordinal == 131) {
    length = sizeof(uintptr_t) - 1;
    buffer = reinterpret_cast<BYTE *>(uintptr_t(1));
  }
  if (ordinal == 132) {
    length = 0xFFFFFFFF;
  }
  if (ordinal == 133) {
    length = sizeof(uintptr_t) * 2;
    input_protect = PAGE_READWRITE | PAGE_GUARD;
  }
  if (ordinal == 134) {
    length = 0;
    buffer = reinterpret_cast<BYTE *>(uintptr_t(1));
  }
  if (ordinal == 135) {
    buffer = arena + page * 2 - sizeof(uintptr_t);
    memcpy(buffer, &value, sizeof value);
  }
  state(query, "before");
  bytes("buffer-before", buffer, 16);
  bytes("returned-before", returned, 4);
  DWORD old = 0;
  if (!VirtualProtect(arena, page, input_protect, &old) ||
      !VirtualProtect(arena + page * 2, page,
                      (setting && item == 13) || (!setting && item == 43)
                          ? PAGE_READONLY
                          : PAGE_NOACCESS,
                      &old) ||
      !VirtualProtect(arena + page * 4, page, return_protect, &old) ||
      !VirtualProtect(arena + page * 5, page, PAGE_NOACCESS, &old))
    fail("owned-protections");
  printf("case=%u setting=%u width=%zu process=%llX buffer=%llX length=%lu "
         "returned=%llX value=%llX input-protect=%08lX return-protect=%08lX "
         "pid=%lu\n",
         ordinal, unsigned(setting), sizeof(void *),
         static_cast<unsigned long long>(uintptr_t(handle)),
         static_cast<unsigned long long>(uintptr_t(buffer)), length,
         static_cast<unsigned long long>(uintptr_t(returned)),
         static_cast<unsigned long long>(value), input_protect, return_protect,
         GetCurrentProcessId());
  ULONG escaped = 0;
  LONG status =
      setting ? set_call(set, handle, buffer, length, &escaped)
              : query_call(query, handle, buffer, length, returned, &escaped);
  MEMORY_BASIC_INFORMATION input_mapping{}, return_mapping{};
  VirtualQuery(arena, &input_mapping, sizeof input_mapping);
  VirtualQuery(arena + page * 4, &return_mapping, sizeof return_mapping);
  printf("result case=%u status=%08lX escaped=%08lX input-protect=%08lX "
         "return-protect=%08lX\n",
         ordinal, ULONG(status), escaped, input_mapping.Protect,
         return_mapping.Protect);
  bytes("buffer-after", buffer, 16);
  bytes("returned-after", returned, 4);
  state(query, "after");
  if (setting && item >= 46) {
    uintptr_t second = item == 46 ? 1 : 0;
    ULONG second_escaped = 0;
    LONG second_status = set_call(set, GetCurrentProcess(), &second,
                                  sizeof second, &second_escaped);
    printf("second value=%llX status=%08lX escaped=%08lX\n",
           static_cast<unsigned long long>(second), ULONG(second_status),
           second_escaped);
    state(query, "second");
  }
  if (owned)
    CloseHandle(owned);
  if (!VirtualFree(arena, 0, MEM_RELEASE))
    fail("release-owned-arena");
  ExitProcess(77);
}

static HANDLE inherited(HANDLE original) {
  HANDLE result = nullptr;
  if (!DuplicateHandle(GetCurrentProcess(), original, GetCurrentProcess(),
                       &result, 0, TRUE, DUPLICATE_SAME_ACCESS))
    fail("inherited-stream");
  return result;
}

int main(int argc, char **argv) {
  if (argc == 2)
    child(unsigned(strtoul(argv[1], nullptr, 10)));
  setvbuf(stdout, nullptr, _IONBF, 0);
  wchar_t executable[32768];
  if (!GetModuleFileNameW(nullptr, executable, 32768))
    fail("executable-path");
  auto ntdll = GetModuleHandleW(L"ntdll.dll");
  wchar_t module_path[32768]{};
  DWORD module_chars = GetModuleFileNameW(ntdll, module_path, 32768);
  if (!module_chars || module_chars >= 32768)
    fail("ntdll-path");
  printf("ntdll-path utf16=");
  for (DWORD i = 0; i < module_chars; ++i)
    printf("%04X", unsigned(module_path[i]));
  puts("");
  for (const char *name :
       {"NtSetInformationProcess", "NtQueryInformationProcess"}) {
    auto address = reinterpret_cast<const BYTE *>(GetProcAddress(ntdll, name));
    if (!address)
      fail("leaf");
    printf("leaf name=%s pointer-bytes=%zu hex=", name, sizeof(void *));
    for (unsigned i = 0; i < 32; ++i)
      printf("%02X", unsigned(address[i]));
    puts("");
  }
  HANDLE output = inherited(GetStdHandle(STD_OUTPUT_HANDLE));
  HANDLE error = inherited(GetStdHandle(STD_ERROR_HANDLE));
  SECURITY_ATTRIBUTES security{sizeof security, nullptr, TRUE};
  HANDLE input =
      CreateFileW(L"NUL", GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE,
                  &security, OPEN_EXISTING, 0, nullptr);
  if (input == INVALID_HANDLE_VALUE)
    fail("null-input");
  unsigned failures = 0;
  for (unsigned ordinal = 0; ordinal < case_count; ++ordinal) {
    std::wstring command =
        L"\"" + std::wstring(executable) + L"\" " + std::to_wstring(ordinal);
    STARTUPINFOW startup{};
    startup.cb = sizeof startup;
    startup.dwFlags = STARTF_USESTDHANDLES;
    startup.hStdInput = input;
    startup.hStdOutput = output;
    startup.hStdError = error;
    PROCESS_INFORMATION process{};
    if (!CreateProcessW(executable, command.data(), nullptr, nullptr, TRUE,
                        DETACHED_PROCESS, nullptr, nullptr, &startup, &process))
      fail("create-owned-detached-case");
    DWORD wait = WaitForSingleObject(process.hProcess, 60000);
    if (wait != WAIT_OBJECT_0) {
      TerminateProcess(process.hProcess, 78);
      WaitForSingleObject(process.hProcess, 5000);
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
  CloseHandle(input);
  CloseHandle(output);
  CloseHandle(error);
  printf("complete cases=%u child-failures=%u width=%zu\n", case_count,
         failures, sizeof(void *));
  return failures ? 22 : 0;
}
