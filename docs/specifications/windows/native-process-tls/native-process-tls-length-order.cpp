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
static constexpr unsigned case_start = 190;
static constexpr unsigned case_count = 270;
static void fail(const char *operation);

struct Worker {
  HANDLE ready;
  HANDLE stop;
};

static DWORD WINAPI worker_main(void *argument) {
  auto worker = static_cast<Worker *>(argument);
  SetEvent(worker->ready);
  WaitForSingleObject(worker->stop, INFINITE);
  return 0;
}

static uintptr_t worker_vector(HANDLE thread) {
  using Query = LONG(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);
  auto query = reinterpret_cast<Query>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtQueryInformationThread"));
  unsigned char basic[64]{};
  ULONG returned = 0;
  if (!query ||
      query(thread, 0, basic, sizeof(void *) == 8 ? 48 : 28, &returned) < 0)
    fail("worker-thread-basic");
  uintptr_t teb = 0;
  memcpy(&teb, basic + sizeof(void *), sizeof teb);
  uintptr_t vector = 0;
  memcpy(&vector,
         reinterpret_cast<void *>(teb + (sizeof(void *) == 8 ? 0x58 : 0x2C)),
         sizeof vector);
  return vector;
}

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
  Worker workers[2]{};
  HANDLE threads[2]{};
  DWORD worker_ids[2]{};
  unsigned worker_count = 0;
  for (unsigned i = 0; i < worker_count; ++i) {
    workers[i].ready = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    workers[i].stop = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    threads[i] =
        CreateThread(nullptr, 0, worker_main, &workers[i], 0, &worker_ids[i]);
    if (!workers[i].ready || !workers[i].stop || !threads[i] ||
        WaitForSingleObject(workers[i].ready, 60000) != WAIT_OBJECT_0)
      fail("owned-worker-start");
    printf("worker-before index=%u id=%lu vector=%llX\n", i, worker_ids[i],
           static_cast<unsigned long long>(worker_vector(threads[i])));
  }
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
  auto third_vector = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, page, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!third_vector)
    fail("third-owned-vector");
  memcpy(third_vector, new_vector, 64);
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
  const ULONG sizes[] = {0, 16, 24, 28, 32, 40, 56, 64};
  if (ordinal >= 44 && ordinal < 76) {
    flags = 1 + (ordinal - 44) / 8;
  } else if (ordinal >= 76 && ordinal < 100) {
    const ULONG operations[] = {2, 3, 0xFFFFFFFF};
    operation = operations[(ordinal - 76) / 8];
  } else if (ordinal >= 116 && ordinal < 128) {
    operation = (ordinal - 116) % 2;
    count = 1 + ((ordinal - 116) / 2) % 3;
    index = operation ? 0 : 2;
  } else if (ordinal >= 128 && ordinal < 134) {
    count = 2;
    operation = (ordinal - 128) % 2;
    index = operation ? 0 : 2;
  } else if (ordinal >= 134 && ordinal < 140) {
    operation = 1;
    const ULONG previous[] = {0, 1, 2, 4, 8, 16};
    index = previous[ordinal - 134];
    memset(new_vector + 2 * sizeof(void *), 0x5A, 128 - 2 * sizeof(void *));
  } else if (ordinal >= 140) {
    operation = 1;
    index = (ordinal - 140) % 2;
  }
  if (ordinal >= 146 && ordinal < 152) {
    operation = 0;
    index = (ordinal - 146) % 2;
  } else if (ordinal >= 152 && ordinal < 160) {
    operation = 1;
    index = 1;
  } else if (ordinal >= 160 && ordinal < 168) {
    operation = 1;
    index = (ordinal - 160) % 2;
  } else if (ordinal >= 168 && ordinal < 176) {
    operation = 1;
    index = 1;
    count = 2;
  } else if (ordinal >= 176) {
    operation = 1;
    index = 2;
  }
  if (ordinal >= 190) {
    operation = 0;
    count = 1;
    index = 2;
  }
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
  for (unsigned i = 0; i < 3; ++i) {
    SIZE_T at = 16 + i * stride;
    uintptr_t next = uintptr_t(i == 0   ? new_vector
                               : i == 1 ? second_vector
                                        : third_vector);
    if (ordinal == 14 || ordinal == 15)
      next = 0;
    memcpy(record + at, &entry_flags, 4);
    if (ordinal >= 128 && ordinal < 134 && i == 1) {
      const ULONG poisons[] = {1, 2, 0xFFFFFFFF};
      ULONG poison = poisons[(ordinal - 128) / 2];
      memcpy(record + at, &poison, 4);
    }
    memcpy(record + at + pointer_offset, &next, sizeof next);
  }
  ULONG length = ULONG(16 + (count <= 3 ? count : 1) * stride);
  if (ordinal >= 44 && ordinal < 76)
    length = sizes[(ordinal - 44) % 8];
  if (ordinal >= 76 && ordinal < 100)
    length = sizes[(ordinal - 76) % 8];
  if (ordinal >= 100 && ordinal < 104)
    length = 0;
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
  if (ordinal >= 190) {
    const ULONG probed_lengths[] = {
        0,          15,         16,         39,        40,   41,
        56,         64,         256,        4096,      8192, 0x7FFFFFFF,
        0x80000000, 0x80000010, 0xFFFFFFFE, 0xFFFFFFFF};
    length = probed_lengths[(ordinal - 190) % 16];
  }
  unsigned char *input = arena + 128;
  if (ordinal == 30)
    input += 1;
  if (ordinal == 31)
    input += 4;
  if (ordinal == 34)
    input = arena + page * 2 - 20;
  if (ordinal >= 108 && ordinal < 116) {
    const SIZE_T remaining[] = {16, 24, 32, 40};
    input = arena + page * 2 - remaining[(ordinal - 108) / 2];
    operation = (ordinal - 108) % 2;
    index = operation ? 0 : 2;
    memcpy(record + 4, &operation, 4);
    memcpy(record + 12, &index, 4);
  }
  SIZE_T available = SIZE_T(arena + page * 2 - input);
  memcpy(input, record, available < 128 ? available : 128);
  if (ordinal >= 152 && ordinal < 160) {
    uintptr_t selected = uintptr_t(new_vector);
    if (ordinal == 152)
      selected = 0;
    if (ordinal == 153)
      selected = uintptr_t(arena + page * 2);
    DWORD previous_protection = 0;
    if (ordinal == 154 &&
        !VirtualProtect(new_vector, page, PAGE_READONLY, &previous_protection))
      fail("new-readonly");
    if (ordinal == 155 &&
        !VirtualProtect(new_vector, page, PAGE_READWRITE | PAGE_GUARD,
                        &previous_protection))
      fail("new-guard");
    if (ordinal == 156)
      selected = uintptr_t(new_vector + page - 4);
    if (ordinal == 157)
      selected = uintptr_t(new_vector + page - sizeof(void *));
    if (ordinal == 158)
      selected = uintptr_t(input + 16);
    if (ordinal == 159)
      selected = uintptr_t(input + 24);
    memcpy(input + 16 + pointer_offset, &selected, sizeof selected);
  }
  if (ordinal >= 160 && ordinal < 168) {
    uintptr_t selected = uintptr_t(input + ((ordinal - 160) / 2) * 8);
    memcpy(input + 16 + pointer_offset, &selected, sizeof selected);
  }
  if (ordinal >= 168 && ordinal < 176) {
    DWORD previous_protection = 0;
    bool first = ordinal == 172 || ordinal == 173;
    auto selected = first ? new_vector : second_vector;
    DWORD protection = (ordinal - 168) % 4 == 0   ? PAGE_READONLY
                       : (ordinal - 168) % 4 == 1 ? PAGE_READWRITE | PAGE_GUARD
                       : (ordinal - 168) % 4 == 2 ? PAGE_NOACCESS
                                                  : PAGE_READWRITE;
    if (!VirtualProtect(selected, page, protection, &previous_protection))
      fail("partial-thread-new-protect");
  }
  if (ordinal >= 176) {
    uintptr_t selected = before_vector + ((ordinal - 176) % 2) * sizeof(void *);
    memcpy(input + 16 + pointer_offset, &selected, sizeof selected);
  }
  HANDLE process = GetCurrentProcess();
  if (ordinal == 21)
    process = nullptr;
  if (ordinal == 22)
    process = reinterpret_cast<HANDLE>(intptr_t(-2));
  if (ordinal == 23)
    process = reinterpret_cast<HANDLE>(uintptr_t(0x13572468));
  if (ordinal == 102 || ordinal == 103 || ordinal == 106 || ordinal == 107)
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
  if (ordinal >= 100 && ordinal < 108)
    input = reinterpret_cast<unsigned char *>(uintptr_t(ordinal % 2));
  if (ordinal >= 190) {
    unsigned kind = (ordinal - 190) / 16;
    if (kind == 1)
      input = nullptr;
    if (kind == 2)
      input = reinterpret_cast<unsigned char *>(uintptr_t(1));
  }
  print_bytes("input-before", input, 64);
  print_bytes("new-vector-before", new_vector, 128);
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
  if (ordinal >= 190) {
    unsigned kind = (ordinal - 190) / 16;
    if (kind == 3 && !VirtualProtect(arena, page, PAGE_READONLY, &old))
      fail("length-order-readonly");
    if (kind == 4 &&
        !VirtualProtect(arena, page, PAGE_READWRITE | PAGE_GUARD, &old))
      fail("length-order-guard");
  }
  ULONG escaped = 0;
  auto teb = reinterpret_cast<unsigned char *>(NtCurrentTeb());
  auto tls_pointer = teb + (sizeof(void *) == 8 ? 0x58 : 0x2C);
  unsigned char teb_before[64]{}, teb_after[64]{};
  memcpy(teb_before, tls_pointer, 64);
  uintptr_t selected_old = before_vector;
  if (ordinal >= 146 && ordinal < 152) {
    unsigned kind = (ordinal - 146) / 2;
    selected_old = kind == 0   ? 0
                   : kind == 1 ? uintptr_t(tls_pointer)
                               : uintptr_t(arena + page * 2);
    memcpy(tls_pointer, &selected_old, sizeof selected_old);
  }
  LONG result = invoke(set, process, input, length, &escaped);
  uintptr_t after_vector = tls_vector();
  memcpy(teb_after, tls_pointer, 64);
  if (ordinal >= 146 && ordinal < 152)
    memcpy(tls_pointer, teb_before, 64);
  else if (ordinal >= 140)
    memcpy(tls_pointer, &before_vector, sizeof before_vector);
  MEMORY_BASIC_INFORMATION query{};
  if (!VirtualQuery(arena, &query, sizeof query))
    fail("query-owned-input");
  printf("result case=%u status=%08lX escaped=%08lX after-vector=%llX "
         "input-protect=%08lX\n",
         ordinal, ULONG(result), escaped,
         static_cast<unsigned long long>(after_vector), query.Protect);
  printf("selected-old-vector=%llX\n",
         static_cast<unsigned long long>(selected_old));
  printf("teb-before=");
  for (unsigned i = 0; i < 64; ++i)
    printf("%02X", teb_before[i]);
  printf("\nteb-after=");
  for (unsigned i = 0; i < 64; ++i)
    printf("%02X", teb_after[i]);
  puts("");
  MEMORY_BASIC_INFORMATION new_query{}, second_query{};
  VirtualQuery(new_vector, &new_query, sizeof new_query);
  VirtualQuery(second_vector, &second_query, sizeof second_query);
  printf("new-protect-after=%08lX second-protect-after=%08lX\n",
         new_query.Protect, second_query.Protect);
  for (unsigned i = 0; i < worker_count; ++i)
    printf("worker-after index=%u id=%lu vector=%llX\n", i, worker_ids[i],
           static_cast<unsigned long long>(worker_vector(threads[i])));
  print_bytes("input-after", input, 128);
  print_bytes("new-vector-after", new_vector, 128);
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
  for (unsigned ordinal = case_start; ordinal < case_count; ++ordinal) {
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
  printf("complete cases=%u child-failures=%u width=%zu\n",
         case_count - case_start, failures, sizeof(void *));
  return failures ? 22 : 0;
}
