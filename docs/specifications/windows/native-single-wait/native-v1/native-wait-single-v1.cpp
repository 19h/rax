#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>

using Wait = LONG(NTAPI *)(HANDLE, ULONG, LARGE_INTEGER *);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using OpenDirectory = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);
using TestAlert = LONG(NTAPI *)();
static Wait wait_one;
static Query query_event, query_mutant, query_semaphore;
static size_t page_bytes;
static unsigned apc_calls;

static LONG invoke(HANDLE handle, ULONG alertable, LARGE_INTEGER *timeout,
                   ULONG *escaped) {
  __try {
    return wait_one(handle, alertable, timeout);
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}

struct Span {
  BYTE *base;
  Span()
      : base(static_cast<BYTE *>(VirtualAlloc(nullptr, 2 * page_bytes,
                                              MEM_RESERVE | MEM_COMMIT,
                                              PAGE_READWRITE))) {
    if (!base)
      ExitProcess(31);
  }
  LARGE_INTEGER *prepare(unsigned mode) {
    DWORD old;
    if (!VirtualProtect(base, 2 * page_bytes, PAGE_READWRITE, &old))
      ExitProcess(32);
    memset(base, 0, 2 * page_bytes);
    const DWORD protection[] = {PAGE_READONLY, PAGE_NOACCESS,
                                PAGE_READWRITE | PAGE_GUARD,
                                PAGE_READONLY | PAGE_GUARD};
    if (mode >= 4 && mode <= 7 &&
        !VirtualProtect(base + page_bytes, page_bytes, protection[mode - 4],
                        &old))
      ExitProcess(33);
    BYTE *p = nullptr;
    switch (mode) {
    case 0:
      p = base + 32;
      break;
    case 1:
      p = base + 33;
      break;
    case 2:
      p = base + page_bytes - 4;
      break;
    case 3:
      p = base + page_bytes - 1;
      break;
    case 4:
    case 5:
    case 6:
    case 7:
      p = base + page_bytes;
      break;
    case 8:
      return nullptr;
    case 9:
      return reinterpret_cast<LARGE_INTEGER *>(uintptr_t(1));
    default:
      ExitProcess(34);
    }
    return reinterpret_cast<LARGE_INTEGER *>(p);
  }
  DWORD protection() const {
    MEMORY_BASIC_INFORMATION info{};
    if (!VirtualQuery(base + page_bytes, &info, sizeof info))
      ExitProcess(35);
    return info.Protect;
  }
  ~Span() { VirtualFree(base, 0, MEM_RELEASE); }
};

struct Object {
  HANDLE handle = nullptr;
  HANDLE full = nullptr;
  unsigned kind;
  Object(unsigned type, OpenDirectory open_directory) : kind(type) {
    if (type == 0 || type == 1 || type == 4)
      full = CreateEventW(nullptr, type != 0, TRUE, nullptr);
    else if (type == 2)
      full = CreateMutexW(nullptr, FALSE, nullptr);
    else if (type == 3)
      full = CreateSemaphoreW(nullptr, 1, 3, nullptr);
    else if (type == 5) {
      wchar_t name[] = L"\\BaseNamedObjects";
      UNICODE_STRING text{};
      text.Length = sizeof name - sizeof(wchar_t);
      text.MaximumLength = sizeof name;
      text.Buffer = name;
      OBJECT_ATTRIBUTES attrs{};
      attrs.Length = sizeof attrs;
      attrs.Attributes = 0x40;
      attrs.ObjectName = &text;
      if (open_directory(&full, 1, &attrs) != 0)
        ExitProcess(36);
    } else {
      handle =
          type == 6 ? reinterpret_cast<HANDLE>(uintptr_t(0x1234)) : nullptr;
      return;
    }
    if (!full)
      ExitProcess(37);
    handle = full;
    if (type == 4 &&
        !DuplicateHandle(GetCurrentProcess(), full, GetCurrentProcess(),
                         &handle, 0, FALSE, 0))
      ExitProcess(38);
  }
  void state(const char *label) const {
    if (kind >= 5) {
      printf(" %s=none", label);
      return;
    }
    Query query = kind == 2   ? query_mutant
                  : kind == 3 ? query_semaphore
                              : query_event;
    alignas(16) BYTE bytes[16];
    memset(bytes, 0xA5, sizeof bytes);
    ULONG returned = 0xA5A5A5A5;
    LONG status = query(full, 0, bytes, 8, &returned);
    printf(" %s=%08lX/%08lX/", label, ULONG(status), returned);
    for (BYTE byte : bytes)
      printf("%02X", unsigned(byte));
  }
  ~Object() {
    if (full) {
      if (handle != full && !CloseHandle(handle))
        ExitProcess(39);
      if (!CloseHandle(full))
        ExitProcess(40);
    }
  }
};

static void CALLBACK apc(ULONG_PTR) { ++apc_calls; }

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 20;
  HMODULE module = GetModuleHandleW(L"ntdll.dll");
  wait_one =
      reinterpret_cast<Wait>(GetProcAddress(module, "NtWaitForSingleObject"));
  query_event = reinterpret_cast<Query>(GetProcAddress(module, "NtQueryEvent"));
  query_mutant =
      reinterpret_cast<Query>(GetProcAddress(module, "NtQueryMutant"));
  query_semaphore =
      reinterpret_cast<Query>(GetProcAddress(module, "NtQuerySemaphore"));
  auto open_directory = reinterpret_cast<OpenDirectory>(
      GetProcAddress(module, "NtOpenDirectoryObject"));
  auto test_alert =
      reinterpret_cast<TestAlert>(GetProcAddress(module, "NtTestAlert"));
  if (!wait_one || !query_event || !query_mutant || !query_semaphore ||
      !open_directory || !test_alert)
    return 21;
  SYSTEM_INFO info{};
  GetSystemInfo(&info);
  page_bytes = info.dwPageSize;
  DWORD before, after;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 22;
  printf("profile=%s pointer-bytes=%zu page-bytes=%zu pid=%lu tid=%lu\n",
         argv[1], sizeof(void *), page_bytes, GetCurrentProcessId(),
         GetCurrentThreadId());
  size_t cases = 0;
  const ULONG alerts[] = {0, 1, 2, 255, 256, 257, 0xFFFFFFFF};
  {
    Span timeout;
    for (unsigned kind = 0; kind != 8; ++kind)
      for (ULONG alertable : alerts)
        for (unsigned mode = 0; mode != 10; ++mode) {
          Object object(kind, open_directory);
          auto pointer = timeout.prepare(mode);
          ULONG escaped = 0;
          printf("wait case=%zu kind=%u alertable=%08lX timeout=%u", cases++,
                 kind, alertable, mode);
          object.state("before");
          LONG status = invoke(object.handle, alertable, pointer, &escaped);
          printf(" status=%08lX escaped=%08lX protection=%08lX", ULONG(status),
                 escaped, timeout.protection());
          object.state("after");
          puts("");
        }
  }
  size_t apcs = 0;
  for (bool signaled : {false, true})
    for (ULONG alertable : alerts) {
      HANDLE event = CreateEventW(nullptr, FALSE, signaled, nullptr);
      if (!event)
        return 23;
      apc_calls = 0;
      if (!QueueUserAPC(apc, GetCurrentThread(), 0))
        return 24;
      LARGE_INTEGER zero{};
      ULONG escaped = 0;
      LONG status = invoke(event, alertable, &zero, &escaped);
      unsigned during = apc_calls;
      LONG drained = test_alert();
      LONG next = invoke(event, 0, &zero, &escaped);
      printf("apc case=%zu signaled=%u alertable=%08lX status=%08lX during=%u "
             "after-drain=%u drained=%08lX next=%08lX escaped=%08lX\n",
             apcs++, unsigned(signaled), alertable, ULONG(status), during,
             apc_calls, ULONG(drained), ULONG(next), escaped);
      if (!CloseHandle(event))
        return 25;
    }
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 26;
  printf("done waits=%zu apcs=%zu before=%lu after=%lu\n", cases, apcs, before,
         after);
  return before == after ? 0 : 27;
}
