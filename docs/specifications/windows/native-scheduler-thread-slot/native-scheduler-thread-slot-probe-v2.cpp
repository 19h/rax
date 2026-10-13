#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <set>
#include <vector>
#include <windows.h>
#include <winternl.h>
using Set = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
static Set set_thread, set_process;
static Query query_thread, query_process;
static unsigned char *memory;
static HANDLE threads[14], owners[6];
struct Info {
  ULONG action;
  HANDLE owner;
  void *slot;
};
struct HandleEntry {
  HANDLE h;
  uintptr_t handles, pointers;
  ULONG access, type, attributes, reserved;
};
static void fail(const char *what) {
  printf("failure=%s error=%lu\n", what, GetLastError());
  ExitProcess(21);
}
static std::set<uintptr_t> handles() {
  std::vector<uintptr_t> b(131072);
  ULONG r = 0;
  LONG s = query_process(GetCurrentProcess(), 51, b.data(),
                         ULONG(b.size() * sizeof(uintptr_t)), &r);
  if (s < 0 || b[0] > 8192 || r > b.size() * sizeof(uintptr_t))
    fail("handle-snapshot");
  auto e = reinterpret_cast<HandleEntry *>(b.data() + 2);
  std::set<uintptr_t> v;
  for (size_t i = 0; i < b[0]; ++i)
    v.insert(uintptr_t(e[i].h));
  return v;
}
static uintptr_t slot() {
  uintptr_t p = 0;
  ULONG r = 0;
  LONG s = query_thread(GetCurrentThread(), 57, &p, sizeof(p), &r);
  if (s < 0 || r != sizeof(p))
    fail("query-current-slot");
  return p;
}
static void clear() {
  Info i{1, nullptr, reinterpret_cast<void *>(slot())};
  if (i.slot && set_thread(GetCurrentThread(), 57, &i, sizeof(i)) < 0)
    fail("clear-current-slot");
  if (slot())
    fail("clear-slot-state");
}
static uintptr_t bind() {
  Info i{0, owners[0], nullptr};
  LONG s = set_thread(GetCurrentThread(), 57, &i, sizeof(i));
  if (s < 0 || !i.slot || slot() != uintptr_t(i.slot))
    fail("bind-current-slot");
  return uintptr_t(i.slot);
}
static void protect(unsigned page, DWORD flags) {
  DWORD old = 0;
  if (!VirtualProtect(memory + page * 4096, 4096, flags, &old))
    fail("protect");
}
static LONG invoke(HANDLE thread, void *buffer, ULONG length,
                   ULONG *exception) {
  __try {
    return set_thread(thread, 57, buffer, length);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
    return LONG(*exception);
  }
}
static void dump(unsigned at, unsigned size) {
  for (unsigned i = 0; i < size; ++i)
    printf("%02X", unsigned(memory[at + i]));
}
static void one(unsigned ordinal, unsigned family, unsigned bound,
                unsigned thread, ULONG length, unsigned buffer_mode,
                ULONG action, unsigned owner, unsigned slot_mode,
                unsigned padding_mode = 0) {
  clear();
  uintptr_t active = bound ? bind() : 0;
  for (unsigned page = 0; page < 2; ++page)
    protect(page, PAGE_READWRITE);
  memset(memory, 0xA5, 8192);
  void *buffer = memory + 128;
  switch (buffer_mode) {
  case 1:
    buffer = nullptr;
    break;
  case 2:
    buffer = reinterpret_cast<void *>(uintptr_t(1));
    break;
  case 3:
    buffer = memory + 129;
    break;
  case 4:
    buffer = memory + 132;
    break;
  case 5:
  case 6:
  case 7:
  case 8:
    buffer = memory + 4096;
    break;
  case 9:
  case 10:
  case 11:
    buffer = memory + 4092;
    break;
  case 12:
    buffer = memory + 4096 - sizeof(Info);
    break;
  case 13:
    buffer = reinterpret_cast<void *>(~uintptr_t(0) - 7);
    break;
  }
  uintptr_t supplied_slot = slot_mode == 0   ? 0
                            : slot_mode == 1 ? 1
                            : slot_mode == 2 ? ~uintptr_t(0)
                                             : active;
  Info input{};
  memset(&input, 0, sizeof(input));
  input.action = action;
  input.owner = owners[owner];
  input.slot = reinterpret_cast<void *>(supplied_slot);
  if (sizeof(void *) == 8 && padding_mode)
    memset(reinterpret_cast<unsigned char *>(&input) + 4, 0xff, 4);
  if (buffer_mode != 1 && buffer_mode != 2 && buffer_mode != 13)
    memcpy(buffer, &input, sizeof(input));
  if (sizeof(void *) == 4 && padding_mode && buffer_mode == 0)
    memset(memory + 128 + sizeof(input), 0xff, 4);
  DWORD protection = PAGE_READWRITE;
  if (buffer_mode == 5 || buffer_mode == 11)
    protection = PAGE_READONLY;
  if (buffer_mode == 6 || buffer_mode == 9)
    protection = PAGE_NOACCESS;
  if (buffer_mode == 7 || buffer_mode == 10)
    protection = PAGE_READWRITE | PAGE_GUARD;
  if (buffer_mode == 8)
    protection = PAGE_READONLY | PAGE_GUARD;
  protect(1, protection);
  auto hb = handles();
  ULONG exception = 0;
  LONG status = invoke(threads[thread], buffer, length, &exception);
  uintptr_t after = slot();
  DWORD flags[2]{};
  for (unsigned page = 0; page < 2; ++page) {
    MEMORY_BASIC_INFORMATION q{};
    if (!VirtualQuery(memory + page * 4096, &q, sizeof(q)))
      fail("query-buffer");
    flags[page] = q.Protect;
    protect(page, PAGE_READWRITE);
  }
  printf("case=%u family=%u width=%zu bound=%u thread=%u length=%08lX "
         "buffer=%u action=%08lX owner=%u slot-mode=%u padding-mode=%u "
         "input-thread=%llX "
         "input-owner=%llX input-slot=%llX before-slot=%llX after-slot=%llX "
         "status=%08lX exception=%08lX protection=%08lX,%08lX, bytes0=",
         ordinal, family, sizeof(void *), bound, thread, length, buffer_mode,
         action, owner, slot_mode, padding_mode,
         (unsigned long long)threads[thread], (unsigned long long)owners[owner],
         (unsigned long long)supplied_slot, (unsigned long long)active,
         (unsigned long long)after, status, exception, flags[0], flags[1]);
  dump(112, 96);
  printf(" bytes1=");
  dump(4064, 96);
  clear();
  auto ha = handles();
  printf(" exact-handle-set=%u handles-before=%zu handles-after=%zu "
         "final-slot=%llX\n",
         unsigned(ha == hb), hb.size(), ha.size(), (unsigned long long)slot());
  if (ha != hb)
    fail("row-handle-lifetime");
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto n = GetModuleHandleW(L"ntdll.dll");
  set_thread =
      reinterpret_cast<Set>(GetProcAddress(n, "NtSetInformationThread"));
  set_process =
      reinterpret_cast<Set>(GetProcAddress(n, "NtSetInformationProcess"));
  query_thread =
      reinterpret_cast<Query>(GetProcAddress(n, "NtQueryInformationThread"));
  query_process =
      reinterpret_cast<Query>(GetProcAddress(n, "NtQueryInformationProcess"));
  memory = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 8192, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!set_thread || !set_process || !query_thread || !query_process || !memory)
    return 22;
  printf("host width=%zu info=%zu initial-slot=%llX\n", sizeof(void *),
         sizeof(Info), (unsigned long long)slot());
  clear();
  auto initial = handles();
  threads[0] = GetCurrentThread();
  threads[1] = nullptr;
  threads[2] = GetCurrentProcess();
  threads[3] = reinterpret_cast<HANDLE>(uintptr_t(0x1234));
  unsigned at = 4;
  for (DWORD access : {0u, 0x20u, 0x400u, 0x40u, 0x800u, 0x1fffffu}) {
    if (!DuplicateHandle(GetCurrentProcess(), GetCurrentThread(),
                         GetCurrentProcess(), &threads[at++], access, FALSE, 0))
      fail("duplicate-thread");
  }
  threads[10] = CreateEventW(nullptr, FALSE, FALSE, nullptr);
  if (!threads[10])
    fail("event");
  if (!DuplicateHandle(GetCurrentProcess(), GetCurrentProcess(),
                       GetCurrentProcess(), &threads[11], 0x200, FALSE, 0) ||
      !DuplicateHandle(GetCurrentProcess(), GetCurrentThread(),
                       GetCurrentProcess(), &threads[12], 0x100000, FALSE, 0))
    fail("duplicate-auxiliary");
  threads[13] = reinterpret_cast<HANDLE>(uintptr_t(threads[9]) | 3);
  if (set_process(GetCurrentProcess(), 112, &owners[0], sizeof(void *)) < 0 ||
      !owners[0])
    fail("create-owner");
  owners[1] = nullptr;
  owners[2] = reinterpret_cast<HANDLE>(uintptr_t(0x1234));
  owners[3] = threads[10];
  owners[4] = threads[11];
  owners[5] = threads[9];
  auto hb = handles();
  unsigned ordinal = 0;
  for (unsigned bound = 0; bound < 2; ++bound)
    for (unsigned thread = 0; thread < 14; ++thread)
      for (ULONG action : {0u, 1u, 2u, 3u, 0xffffffffu})
        for (unsigned owner = 0; owner < 6; ++owner)
          for (unsigned s = 0; s < 4; ++s)
            one(ordinal++, 0, bound, thread, sizeof(Info), 0, action, owner, s);
  for (unsigned bound = 0; bound < 2; ++bound)
    for (ULONG length : {0u, 1u, 4u, 7u, 8u, 11u, 12u, 16u, 23u, 24u, 25u, 32u,
                         4097u, 0xffffffffu})
      for (unsigned buffer = 0; buffer < 14; ++buffer)
        for (ULONG action : {0u, 1u, 2u, 0xffffffffu})
          for (unsigned owner = 0; owner < 3; ++owner)
            one(ordinal++, 1, bound, 0, length, buffer, action, owner, 0);
  for (unsigned bound = 0; bound < 2; ++bound)
    for (unsigned thread : {1u, 3u, 4u, 10u})
      for (ULONG length : {0u, 1u, 4u, 7u, 8u, 11u, 12u, 16u, 23u, 24u, 25u,
                           32u, 4097u, 0xffffffffu})
        for (unsigned buffer = 0; buffer < 14; ++buffer)
          one(ordinal++, 2, bound, thread, length, buffer, 0, 0, 0);
  for (unsigned bound = 0; bound < 2; ++bound)
    for (ULONG action : {0u, 1u, 2u, 3u, 0xffffffffu})
      for (unsigned owner = 0; owner < 3; ++owner)
        for (unsigned supplied = 0; supplied < 4; ++supplied)
          for (unsigned padding = 0; padding < 2; ++padding)
            one(ordinal++, 3, bound, 0, sizeof(Info), 0, action, owner,
                supplied, padding);
  clear();
  if (handles() != hb)
    fail("complete-handle-lifetime");
  for (unsigned i = 4; i < 13; ++i)
    if (!CloseHandle(threads[i]))
      fail("close-thread-input");
  if (!CloseHandle(owners[0]) || handles() != initial)
    fail("close-owner-input");
  if (!VirtualFree(memory, 0, MEM_RELEASE))
    fail("release-buffer");
  printf("complete cases=%u exact-handle-set=1 owned-input-handles-closed=10 "
         "final-slot=0\n",
         ordinal);
  return 0;
}
