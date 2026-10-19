#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <climits>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>

using Create = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *, LONG, LONG);
using Open = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);
using Release = LONG(NTAPI *)(HANDLE, LONG, LONG *);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using Wait = LONG(NTAPI *)(HANDLE, BOOLEAN, LARGE_INTEGER *);
using Close = LONG(NTAPI *)(HANDLE);
static Create create_sem;
static Open open_sem;
static Release release_sem;
static Query query_sem, query_object;
static Wait wait_object;
static Close close_object;
static size_t page_size, ordinal, created_count, closed_count;

struct Span {
  BYTE *base;
  Span()
      : base(static_cast<BYTE *>(VirtualAlloc(nullptr, 2 * page_size,
                                              MEM_COMMIT | MEM_RESERVE,
                                              PAGE_READWRITE))) {
    if (!base)
      ExitProcess(31);
  }
  void *prepare(unsigned mode, const void *input = nullptr, size_t size = 0) {
    DWORD previous;
    if (!VirtualProtect(base, 2 * page_size, PAGE_READWRITE, &previous))
      ExitProcess(32);
    memset(base, 0xA5, 2 * page_size);
    void *pointer = nullptr;
    if (mode == 0 || mode == 1)
      pointer = base + 32 + mode;
    else if (mode >= 2 && mode <= 5)
      pointer = base + page_size;
    else if (mode == 6)
      pointer = nullptr;
    else if (mode == 7)
      pointer = reinterpret_cast<void *>(uintptr_t(1));
    else if (mode == 8)
      pointer = reinterpret_cast<void *>(uintptr_t(-2));
    else if (mode == 9)
      pointer = base + page_size - 4;
    else if (mode >= 10 && mode <= 12)
      pointer = base + page_size - 4;
    else if (mode >= 13 && mode <= 15)
      pointer = base + page_size - 1;
    else
      ExitProcess(33);
    const auto at = reinterpret_cast<uintptr_t>(pointer);
    if (input && at >= reinterpret_cast<uintptr_t>(base) &&
        at + size <= reinterpret_cast<uintptr_t>(base + 2 * page_size))
      memcpy(pointer, input, size);
    DWORD protection = PAGE_READWRITE;
    if (mode >= 2 && mode <= 5) {
      const DWORD flags[] = {PAGE_READONLY, PAGE_NOACCESS,
                             PAGE_READWRITE | PAGE_GUARD,
                             PAGE_READONLY | PAGE_GUARD};
      protection = flags[mode - 2];
    } else if (mode == 10 || mode == 13)
      protection = PAGE_READONLY;
    else if (mode == 11 || mode == 14)
      protection = PAGE_NOACCESS;
    else if (mode == 12 || mode == 15)
      protection = PAGE_READWRITE | PAGE_GUARD;
    if (!VirtualProtect(base + page_size, page_size, protection, &previous))
      ExitProcess(34);
    return pointer;
  }
  void print() {
    MEMORY_BASIC_INFORMATION info{};
    if (!VirtualQuery(base + page_size, &info, sizeof info))
      ExitProcess(35);
    printf("%08lX:", info.Protect);
    DWORD previous;
    if (!VirtualProtect(base, 2 * page_size, PAGE_READWRITE, &previous))
      ExitProcess(36);
    for (size_t origin : {size_t(32), page_size - 32, page_size})
      for (size_t i = 0; i < 32; ++i)
        printf("%02X", unsigned(base[origin + i]));
  }
};

static LONG invoke(unsigned op, HANDLE handle, void *output, ULONG access,
                   OBJECT_ATTRIBUTES *attrs, LONG first, LONG second,
                   ULONG info_class, ULONG length, ULONG *returned,
                   ULONG *escaped) {
  __try {
    if (op == 0)
      return create_sem(static_cast<HANDLE *>(output), access, attrs, first,
                        second);
    if (op == 1)
      return open_sem(static_cast<HANDLE *>(output), access, attrs);
    if (op == 2)
      return release_sem(handle, first, static_cast<LONG *>(output));
    return query_sem(handle, info_class, output, length, returned);
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}
static HANDLE new_sem(ULONG access, LONG count, LONG max) {
  HANDLE result = nullptr;
  if (create_sem(&result, access, nullptr, count, max) != 0 || !result)
    ExitProcess(40);
  ++created_count;
  return result;
}
static void close_handle(HANDLE handle) {
  if (close_object(handle) != 0)
    ExitProcess(41);
  ++closed_count;
}
static LONG current(HANDLE master) {
  LONG counts[2] = {-1, -1};
  if (query_sem(master, 0, counts, sizeof counts, nullptr) != 0 ||
      counts[1] != 2)
    ExitProcess(42);
  return counts[0];
}
static void reset(HANDLE master) {
  LARGE_INTEGER zero{};
  while (wait_object(master, FALSE, &zero) == 0) {
  }
  if (release_sem(master, 1, nullptr) != 0 || current(master) != 1)
    ExitProcess(43);
}
static void created(LONG status, ULONG escaped, void *output) {
  if (status < 0 || escaped)
    return;
  HANDLE handle = nullptr;
  memcpy(&handle, output, sizeof handle);
  if (!handle)
    ExitProcess(44);
  ++created_count;
  PUBLIC_OBJECT_BASIC_INFORMATION info{};
  ULONG returned = 0;
  const LONG query_status =
      query_object(handle, 0, &info, sizeof info, &returned);
  LONG counts[2] = {-1, -1};
  const LONG state_status =
      query_sem(handle, 0, counts, sizeof counts, nullptr);
  printf(" created=%08lX:%08lX:%08lX:%08lX:%ld:%ld", ULONG(query_status),
         info.GrantedAccess, info.Attributes, ULONG(state_status), counts[0],
         counts[1]);
  close_handle(handle);
}

static void create_matrix() {
  Span output, attributes;
  const LONG pairs[][2] = {{0, 1},        {1, 2},
                           {-1, 2},       {0, 0},
                           {3, 2},        {LONG_MIN, LONG_MAX},
                           {0, LONG_MAX}, {LONG_MAX, LONG_MAX}};
  for (unsigned ac = 0; ac < 12; ++ac)
    for (unsigned pair = 0; pair < 8; ++pair)
      for (unsigned om = 0; om < 16; ++om) {
        OBJECT_ATTRIBUTES attrs{};
        attrs.Length = sizeof attrs;
        if (ac == 2)
          attrs.Length = 0;
        if (ac == 3)
          attrs.Length -= 1;
        if (ac == 9)
          attrs.Attributes = 1;
        if (ac == 10)
          attrs.RootDirectory = reinterpret_cast<HANDLE>(uintptr_t(0x12345678));
        if (ac == 11)
          attrs.ObjectName = reinterpret_cast<UNICODE_STRING *>(uintptr_t(1));
        unsigned am = ac == 4   ? 1
                      : ac == 5 ? 7
                      : ac == 6 ? 2
                      : ac == 7 ? 3
                      : ac == 8 ? 4
                                : 0;
        auto ap = static_cast<OBJECT_ATTRIBUTES *>(
            attributes.prepare(am, &attrs, sizeof attrs));
        if (ac == 0)
          ap = nullptr;
        void *out = output.prepare(om);
        ULONG escaped = 0;
        const LONG status =
            invoke(0, nullptr, out, 0x1F0003, ap, pairs[pair][0],
                   pairs[pair][1], 0, 0, nullptr, &escaped);
        printf("create ordinal=%zu ac=%u pair=%u om=%u status=%08lX "
               "escaped=%08lX output=",
               ordinal++, ac, pair, om, ULONG(status), escaped);
        output.print();
        printf(" attrs=");
        attributes.print();
        created(status, escaped, out);
        puts("");
      }
  for (ULONG access :
       {0ul, 1ul, 2ul, 3ul, 4ul, 0x100000ul, 0x1F0003ul, 0xFFFFFFFFul,
        0x80000000ul, 0x40000000ul, 0x20000000ul, 0x10000000ul, 0x02000000ul,
        0x01000000ul, 0x00800000ul, 0x000F0000ul}) {
    void *out = output.prepare(0);
    ULONG escaped = 0;
    const LONG status =
        invoke(0, nullptr, out, access, nullptr, 1, 2, 0, 0, nullptr, &escaped);
    printf("access ordinal=%zu access=%08lX status=%08lX escaped=%08lX output=",
           ordinal++, access, ULONG(status), escaped);
    output.print();
    created(status, escaped, out);
    puts("");
  }
}

static void open_matrix() {
  Span output, attributes;
  wchar_t missing[128];
  swprintf_s(missing, L"\\BaseNamedObjects\\AssistSemaphoreMissing-%lu",
             GetCurrentProcessId());
  UNICODE_STRING name{};
  name.Buffer = missing;
  name.Length = USHORT(wcslen(missing) * sizeof(wchar_t));
  name.MaximumLength = name.Length;
  for (unsigned ac = 0; ac < 12; ++ac)
    for (unsigned om = 0; om < 16; ++om) {
      OBJECT_ATTRIBUTES attrs{};
      attrs.Length = sizeof attrs;
      attrs.ObjectName = &name;
      if (ac == 1)
        attrs.ObjectName = nullptr;
      if (ac == 2)
        attrs.Length = 0;
      if (ac == 3)
        attrs.Length -= 1;
      if (ac == 9)
        attrs.Attributes = 1;
      if (ac == 10)
        attrs.RootDirectory = reinterpret_cast<HANDLE>(uintptr_t(0x12345678));
      if (ac == 11)
        attrs.ObjectName = reinterpret_cast<UNICODE_STRING *>(uintptr_t(1));
      unsigned am = ac == 4   ? 1
                    : ac == 5 ? 7
                    : ac == 6 ? 2
                    : ac == 7 ? 3
                    : ac == 8 ? 4
                              : 0;
      auto ap = static_cast<OBJECT_ATTRIBUTES *>(
          attributes.prepare(am, &attrs, sizeof attrs));
      if (ac == 0)
        ap = nullptr;
      void *out = output.prepare(om);
      ULONG escaped = 0;
      const LONG status =
          invoke(1, nullptr, out, 0x1F0003, ap, 0, 0, 0, 0, nullptr, &escaped);
      printf("open ordinal=%zu ac=%u om=%u status=%08lX escaped=%08lX output=",
             ordinal++, ac, om, ULONG(status), escaped);
      output.print();
      printf(" attrs=");
      attributes.print();
      created(status, escaped, out);
      puts("");
    }
}

static void operations() {
  HANDLE master = new_sem(0x1F0003, 1, 2);
  HANDLE denied = nullptr, query = nullptr, modify = nullptr, sync = nullptr;
  HANDLE *duplicates[] = {&denied, &query, &modify, &sync};
  const ULONG masks[] = {0, 1, 2, 0x100000};
  for (unsigned i = 0; i < 4; ++i) {
    if (!DuplicateHandle(GetCurrentProcess(), master, GetCurrentProcess(),
                         duplicates[i], masks[i], FALSE, 0))
      ExitProcess(45);
    ++created_count;
  }
  HANDLE event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
  if (!event)
    ExitProcess(46);
  ++created_count;
  const HANDLE handles[] = {master,
                            nullptr,
                            reinterpret_cast<HANDLE>(uintptr_t(0x12345678)),
                            GetCurrentProcess(),
                            GetCurrentThread(),
                            event,
                            denied,
                            query,
                            modify,
                            sync};
  Span output, returned;
  for (unsigned hc : {0u, 2u, 6u})
    for (unsigned shape = 0; shape < 6; ++shape)
      for (unsigned om = 0; om < 16; ++om)
        for (unsigned rm = 0; rm < 16; ++rm) {
          const ULONG classes[] = {1, ULONG_MAX, 0, 0, 0, 0};
          const ULONG lengths[] = {8, 8, 0, 7, 9, ULONG_MAX};
          void *out = output.prepare(om);
          auto ret = static_cast<ULONG *>(returned.prepare(rm));
          ULONG escaped = 0;
          const LONG status =
              invoke(3, handles[hc], out, 0, nullptr, 0, 0, classes[shape],
                     lengths[shape], ret, &escaped);
          printf("query-precedence ordinal=%zu hc=%u class=%lu length=%lu "
                 "om=%u rm=%u status=%08lX escaped=%08lX output=",
                 ordinal++, hc, classes[shape], lengths[shape], om, rm,
                 ULONG(status), escaped);
          output.print();
          printf(" returned=");
          returned.print();
          puts("");
        }
  for (HANDLE handle : {master, denied, query, modify, sync, event})
    close_handle(handle);
}

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 20;
  SYSTEM_INFO info{};
  GetSystemInfo(&info);
  page_size = info.dwPageSize;
  const auto module = GetModuleHandleW(L"ntdll.dll");
  create_sem =
      reinterpret_cast<Create>(GetProcAddress(module, "NtCreateSemaphore"));
  open_sem = reinterpret_cast<Open>(GetProcAddress(module, "NtOpenSemaphore"));
  release_sem =
      reinterpret_cast<Release>(GetProcAddress(module, "NtReleaseSemaphore"));
  query_sem =
      reinterpret_cast<Query>(GetProcAddress(module, "NtQuerySemaphore"));
  query_object =
      reinterpret_cast<Query>(GetProcAddress(module, "NtQueryObject"));
  wait_object =
      reinterpret_cast<Wait>(GetProcAddress(module, "NtWaitForSingleObject"));
  close_object = reinterpret_cast<Close>(GetProcAddress(module, "NtClose"));
  if (!create_sem || !open_sem || !release_sem || !query_sem || !query_object ||
      !wait_object || !close_object)
    return 21;
  DWORD before = 0, after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 22;
  printf("profile=%s pointer-bytes=%zu page-bytes=%zu pid=%lu tid=%lu\n",
         argv[1], sizeof(void *), page_size, GetCurrentProcessId(),
         GetCurrentThreadId());
  operations();
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 23;
  printf("summary cases=%zu created=%zu closed=%zu handles-before=%lu "
         "handles-after=%lu\n",
         ordinal, created_count, closed_count, before, after);
  return created_count == closed_count && before == after ? 0 : 24;
}
