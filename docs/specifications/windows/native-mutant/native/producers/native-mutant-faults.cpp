#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
#include <winternl.h>

using Create = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *, ULONG);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using Release = LONG(NTAPI *)(HANDLE, LONG *);
using Close = LONG(NTAPI *)(HANDLE);
static Query query_mutant;
static Release release_mutant;
static Close close_object;
static size_t page_size;

struct Span {
  BYTE *base;
  explicit Span()
      : base(static_cast<BYTE *>(VirtualAlloc(nullptr, 2 * page_size,
                                              MEM_RESERVE | MEM_COMMIT,
                                              PAGE_READWRITE))) {
    if (!base)
      ExitProcess(31);
  }
  void *prepare(unsigned mode) {
    DWORD previous;
    if (!VirtualProtect(base, 2 * page_size, PAGE_READWRITE, &previous))
      ExitProcess(32);
    memset(base, 0xA5, 2 * page_size);
    const DWORD protections[] = {PAGE_READONLY, PAGE_NOACCESS,
                                 PAGE_READWRITE | PAGE_GUARD,
                                 PAGE_READONLY | PAGE_GUARD};
    if (mode >= 4 && mode <= 7 &&
        !VirtualProtect(base + page_size, page_size, protections[mode - 4],
                        &previous))
      ExitProcess(33);
    switch (mode) {
    case 0:
      return base + 32;
    case 1:
      return base + 33;
    case 2:
      return base + page_size - 4;
    case 3:
      return base + page_size - 1;
    case 4:
    case 5:
    case 6:
    case 7:
      return base + page_size;
    case 8:
      return nullptr;
    case 9:
      return reinterpret_cast<void *>(uintptr_t(1));
    default:
      ExitProcess(34);
    }
  }
  void print() {
    MEMORY_BASIC_INFORMATION info{};
    if (!VirtualQuery(base + page_size, &info, sizeof info))
      ExitProcess(35);
    printf("%08lX:", info.Protect);
    DWORD previous;
    if (!VirtualProtect(base, 2 * page_size, PAGE_READWRITE, &previous))
      ExitProcess(36);
    for (size_t origin : {size_t(32), page_size - 16, page_size})
      for (size_t i = 0; i < 16; ++i)
        printf("%02X", unsigned(base[origin + i]));
  }
};

static LONG query(HANDLE handle, ULONG klass, void *buffer, ULONG length,
                  ULONG *returned, ULONG *escaped) {
  __try {
    return query_mutant(handle, klass, buffer, length, returned);
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}

static LONG release(HANDLE handle, LONG *previous, ULONG *escaped) {
  __try {
    return release_mutant(handle, previous);
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}

static void state(HANDLE handle, const char *label) {
  BYTE info[8];
  memset(info, 0xA5, sizeof info);
  ULONG returned = 0xA5A5A5A5;
  const LONG status = query_mutant(handle, 0, info, 8, &returned);
  printf(" %s=%08lX/%08lX/", label, ULONG(status), returned);
  for (BYTE byte : info)
    printf("%02X", unsigned(byte));
}

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 20;
  SYSTEM_INFO info{};
  GetSystemInfo(&info);
  page_size = info.dwPageSize;
  HMODULE module = GetModuleHandleW(L"ntdll.dll");
  const auto create_mutant =
      reinterpret_cast<Create>(GetProcAddress(module, "NtCreateMutant"));
  query_mutant =
      reinterpret_cast<Query>(GetProcAddress(module, "NtQueryMutant"));
  release_mutant =
      reinterpret_cast<Release>(GetProcAddress(module, "NtReleaseMutant"));
  close_object = reinterpret_cast<Close>(GetProcAddress(module, "NtClose"));
  if (!create_mutant || !query_mutant || !release_mutant || !close_object)
    return 21;
  DWORD before;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 22;
  Span output, returned;
  HANDLE valid = nullptr, zero = nullptr;
  if (create_mutant(&valid, 0x1F0001, nullptr, 0) != 0 ||
      !DuplicateHandle(GetCurrentProcess(), valid, GetCurrentProcess(), &zero,
                       0, FALSE, 0))
    return 23;
  HANDLE event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
  if (!event)
    return 24;
  HANDLE handles[] = {valid, zero, event,
                      reinterpret_cast<HANDLE>(uintptr_t(0x1234)), nullptr};
  const ULONG lengths[] = {0, 1, 7, 8, 16, 17, 0xFFFFFFFF};
  printf("profile=%s pointer-bytes=%zu page-bytes=%zu pid=%lu tid=%lu\n",
         argv[1], sizeof(void *), page_size, GetCurrentProcessId(),
         GetCurrentThreadId());
  size_t queries = 0, releases = 0;
  for (unsigned handle = 0; handle < 5; ++handle)
    for (ULONG klass : {0ul, 1ul, 0xFFFFFFFFul})
      for (ULONG length : lengths)
        for (unsigned out_mode = 0; out_mode < 10; ++out_mode)
          for (unsigned ret_mode : {0u, 1u, 5u, 6u, 8u, 10u}) {
            void *out = output.prepare(out_mode);
            void *ret = returned.prepare(ret_mode == 10 ? 0 : ret_mode);
            if (ret_mode == 10)
              ret = out;
            ULONG escaped = 0;
            LONG status = query(handles[handle], klass, out, length,
                                static_cast<ULONG *>(ret), &escaped);
            printf("query case=%zu handle=%u class=%08lX length=%08lX out=%u "
                   "ret=%u status=%08lX escaped=%08lX bytes=",
                   queries++, handle, klass, length, out_mode, ret_mode,
                   ULONG(status), escaped);
            output.print();
            printf(" returned=");
            returned.print();
            puts("");
          }
  for (unsigned handle = 0; handle < 5; ++handle)
    for (unsigned mode = 0; mode < 10; ++mode) {
      HANDLE owner = nullptr, no_access = nullptr;
      if (create_mutant(&owner, 0x1F0001, nullptr, 1) != 0 ||
          !DuplicateHandle(GetCurrentProcess(), owner, GetCurrentProcess(),
                           &no_access, 0, FALSE, 0))
        return 25;
      HANDLE actual[] = {owner, no_access, event,
                         reinterpret_cast<HANDLE>(uintptr_t(0x1234)), nullptr};
      void *destination = output.prepare(mode);
      ULONG escaped = 0;
      printf("release case=%zu handle=%u previous=%u", releases++, handle,
             mode);
      state(owner, "before");
      const LONG status =
          release(actual[handle], static_cast<LONG *>(destination), &escaped);
      printf(" status=%08lX escaped=%08lX bytes=", ULONG(status), escaped);
      output.print();
      state(owner, "after");
      puts("");
      if (close_object(no_access) != 0 || close_object(owner) != 0)
        return 26;
    }
  if (close_object(event) != 0 || close_object(zero) != 0 ||
      close_object(valid) != 0)
    return 27;
  if (!VirtualFree(output.base, 0, MEM_RELEASE) ||
      !VirtualFree(returned.base, 0, MEM_RELEASE))
    return 28;
  DWORD after;
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 29;
  printf("complete queries=%zu releases=%zu before=%lu after=%lu\n", queries,
         releases, before, after);
  return before == after ? 0 : 30;
}
