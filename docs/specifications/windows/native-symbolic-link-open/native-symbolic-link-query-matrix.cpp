#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>
using Open = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);
using Query = LONG(NTAPI *)(HANDLE, UNICODE_STRING *, ULONG *);
using Close = LONG(NTAPI *)(HANDLE);
static Open open_link, open_directory;
static Query query_link;
static Close close_nt;
static unsigned count;
static void fail(const char *s) {
  printf("failure=%s error=%lu\n", s, GetLastError());
  ExitProcess(31);
}
static void dump(const unsigned char *p, size_t n) {
  for (size_t i = 0; i < n; i++)
    printf("%02X", unsigned(p[i]));
}
static LONG invoke(HANDLE h, UNICODE_STRING *target, ULONG *returned,
                   ULONG *exception) {
  __try {
    return query_link(h, target, returned);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
    return LONG(0xDEADBEEF);
  }
}
static HANDLE open(const wchar_t *s, ULONG rights, bool directory,
                   LONG *status) {
  UNICODE_STRING n{};
  n.Buffer = const_cast<PWSTR>(s);
  n.Length = USHORT(wcslen(s) * 2);
  n.MaximumLength = n.Length + 2;
  OBJECT_ATTRIBUTES a{};
  a.Length = sizeof(a);
  a.ObjectName = &n;
  a.Attributes = 0x40;
  HANDLE h = nullptr;
  *status = (directory ? open_directory : open_link)(&h, rights, &a);
  return h;
}
struct Region {
  unsigned char *p;
  DWORD page;
  Region() {
    SYSTEM_INFO si{};
    GetSystemInfo(&si);
    page = si.dwPageSize;
    p = (unsigned char *)VirtualAlloc(nullptr, page * 18,
                                      MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    if (!p)
      fail("alloc");
    memset(p, 0xA5, page * 18);
  }
  ~Region() { VirtualFree(p, 0, MEM_RELEASE); }
  void protect(DWORD offset, DWORD protection) {
    DWORD old = 0;
    if (!VirtualProtect(p + offset, page, protection, &old))
      fail("protect");
  }
  DWORD protection(DWORD offset) {
    MEMORY_BASIC_INFORMATION i{};
    if (!VirtualQuery(p + offset, &i, sizeof(i)))
      fail("virtual-query");
    return i.Protect;
  }
};
static void one(unsigned family, unsigned handle_kind, ULONG length,
                ULONG maximum, unsigned descriptor_mode, unsigned buffer_mode,
                unsigned returned_mode) {
  Region d, b, r;
  DWORD before = 0, after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    fail("count-before");
  LONG open_status = 0;
  HANDLE owned = nullptr, h = nullptr;
  bool event = false;
  if (handle_kind == 0 || handle_kind == 1 || handle_kind == 2 ||
      handle_kind == 3 || handle_kind == 4 || handle_kind == 9 ||
      handle_kind == 10) {
    const wchar_t *path = handle_kind == 1 ? L"\\KnownDlls32\\KnownDllPath"
                                           : L"\\KnownDlls\\KnownDllPath";
    ULONG rights = handle_kind == 2   ? 1
                   : handle_kind == 3 ? 0x20000
                   : handle_kind == 4 ? 0x20000
                                      : 1;
    if (handle_kind == 4)
      path = L"\\KnownDlls";
    owned = open(path, rights, handle_kind == 4, &open_status);
    h = owned;
    if (handle_kind == 2 && open_status >= 0) {
      HANDLE zero = nullptr;
      if (!DuplicateHandle(GetCurrentProcess(), owned, GetCurrentProcess(),
                           &zero, 0, FALSE, 0))
        fail("duplicate-zero");
      if (close_nt(owned) < 0)
        fail("close-original");
      owned = zero;
      h = zero;
    }
    if (open_status < 0)
      fail("selected-open");
    if (handle_kind == 9) {
      if (close_nt(owned) < 0)
        fail("close-before");
      owned = nullptr;
    }
    if (handle_kind == 10)
      h = (HANDLE)(uintptr_t(h) | 3);
  } else if (handle_kind == 5) {
    owned = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (!owned)
      fail("event");
    h = owned;
    event = true;
  } else if (handle_kind == 6)
    h = (HANDLE)uintptr_t(0x12345678);
  else if (handle_kind == 7)
    h = GetCurrentProcess();
  else if (handle_kind == 8)
    h = GetCurrentThread();
  UNICODE_STRING *target = (UNICODE_STRING *)(d.p + 64);
  PWSTR buffer = (PWSTR)(b.p + 64);
  ULONG *returned = (ULONG *)(r.p + 64);
  if (buffer_mode == 1)
    buffer = nullptr;
  else if (buffer_mode == 2)
    buffer = (PWSTR)uintptr_t(1);
  else if (buffer_mode == 3)
    buffer = (PWSTR)(b.p + 65);
  else if (buffer_mode == 4) {
    buffer = (PWSTR)(b.p + b.page - 1);
    b.protect(b.page, PAGE_NOACCESS);
  } else if (buffer_mode >= 5) {
    buffer = (PWSTR)(b.p + 64);
    b.protect(0, buffer_mode == 5   ? PAGE_NOACCESS
                 : buffer_mode == 6 ? PAGE_READONLY
                                    : PAGE_READWRITE | PAGE_GUARD);
  }
  if (descriptor_mode == 1)
    target = nullptr;
  else if (descriptor_mode == 2)
    target = (UNICODE_STRING *)uintptr_t(1);
  else if (descriptor_mode == 3)
    target = (UNICODE_STRING *)(d.p + 65);
  else if (descriptor_mode == 4)
    target = (UNICODE_STRING *)(d.p + d.page - sizeof(UNICODE_STRING) / 2);
  UNICODE_STRING original{};
  original.Length = USHORT(length);
  original.MaximumLength = USHORT(maximum);
  original.Buffer = buffer;
  if (descriptor_mode != 1 && descriptor_mode != 2)
    memcpy(target, &original, sizeof(original));
  if (descriptor_mode == 4)
    d.protect(d.page, PAGE_NOACCESS);
  else if (descriptor_mode >= 5)
    d.protect(0, descriptor_mode == 5   ? PAGE_NOACCESS
                 : descriptor_mode == 6 ? PAGE_READONLY
                                        : PAGE_READWRITE | PAGE_GUARD);
  if (returned_mode == 1)
    returned = nullptr;
  else if (returned_mode == 2)
    returned = (ULONG *)uintptr_t(1);
  else if (returned_mode == 3)
    returned = (ULONG *)(r.p + 65);
  else if (returned_mode == 4) {
    returned = (ULONG *)(r.p + r.page - 2);
    r.protect(r.page, PAGE_NOACCESS);
  } else if (returned_mode >= 5 && returned_mode <= 7)
    r.protect(0, returned_mode == 5   ? PAGE_NOACCESS
                 : returned_mode == 6 ? PAGE_READONLY
                                      : PAGE_READWRITE | PAGE_GUARD);
  else if (returned_mode == 8)
    returned = (ULONG *)target;
  else if (returned_mode == 9)
    returned = (ULONG *)buffer;

  // Snapshots use initialized readable backing pages after restoring protection
  // only after the syscall. Initial bytes are reconstructible from original
  // descriptor and the A5 page pattern.
  ULONG exception = 0;
  LONG status = invoke(h, target, returned, &exception);
  DWORD dp = d.protection(0), dp2 = d.protection(d.page), bp = b.protection(0),
        bp2 = b.protection(b.page), rp = r.protection(0),
        rp2 = r.protection(r.page);
  d.protect(0, PAGE_READWRITE);
  d.protect(d.page, PAGE_READWRITE);
  b.protect(0, PAGE_READWRITE);
  b.protect(b.page, PAGE_READWRITE);
  r.protect(0, PAGE_READWRITE);
  r.protect(r.page, PAGE_READWRITE);
  if (owned) {
    if (event) {
      if (!CloseHandle(owned))
        fail("close-event");
    } else if (close_nt(owned) < 0)
      fail("close-link");
  }
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    fail("count-after");
  printf("case=%u width=%zu family=%u handle=%u length=%lu maximum=%lu "
         "descriptor-mode=%u buffer-mode=%u returned-mode=%u open=%08lX "
         "status=%08lX exception=%08lX handles-before=%lu handles-after=%lu "
         "descriptor-address=%llX buffer-address=%llX returned-address=%llX "
         "descriptor-protection=%08lX descriptor-next=%08lX "
         "buffer-protection=%08lX buffer-next=%08lX returned-protection=%08lX "
         "returned-next=%08lX original=",
         count++, sizeof(void *), family, handle_kind, length, maximum,
         descriptor_mode, buffer_mode, returned_mode, ULONG(open_status),
         ULONG(status), exception, before, after,
         (unsigned long long)uintptr_t(target),
         (unsigned long long)uintptr_t(buffer),
         (unsigned long long)uintptr_t(returned), dp, dp2, bp, bp2, rp, rp2);
  dump((const unsigned char *)&original, sizeof(original));
  printf(" descriptor-data=");
  dump(d.p + 48, 96);
  printf(" descriptor-edge=");
  dump(d.p + d.page - 32, 64);
  printf(" buffer-data=");
  dump(b.p + 48, 128);
  printf(" buffer-edge=");
  dump(b.p + b.page - 32, 64);
  printf(" returned-data=");
  dump(r.p + 48, 96);
  printf(" returned-edge=");
  dump(r.p + r.page - 32, 64);
  puts("");
  if (before != after)
    fail("lifetime");
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto n = GetModuleHandleW(L"ntdll.dll");
  open_link = (Open)GetProcAddress(n, "NtOpenSymbolicLinkObject");
  open_directory = (Open)GetProcAddress(n, "NtOpenDirectoryObject");
  query_link = (Query)GetProcAddress(n, "NtQuerySymbolicLinkObject");
  close_nt = (Close)GetProcAddress(n, "NtClose");
  if (!open_link || !open_directory || !query_link || !close_nt)
    fail("exports");
  for (unsigned h = 0; h < 11; h++)
    for (ULONG l : {0u, 1u, 37u, 38u, 39u, 40u, 65535u})
      for (ULONG m :
           {0u, 1u, 2u, 37u, 38u, 39u, 40u, 41u, 42u, 64u, 65534u, 65535u})
        for (unsigned r : {0u, 1u})
          one(0, h, l, m, 0, 0, r);
  for (unsigned h : {0u, 2u, 3u, 4u, 6u})
    for (ULONG m : {0u, 38u, 39u, 40u, 64u})
      for (unsigned d = 0; d < 8; d++)
        for (unsigned r : {0u, 1u, 2u})
          one(1, h, 0x1234, m, d, 0, r);
  for (unsigned h : {0u, 2u, 3u, 4u, 6u})
    for (ULONG m : {0u, 38u, 39u, 40u, 64u})
      for (unsigned b = 0; b < 8; b++)
        for (unsigned r : {0u, 1u, 2u})
          one(2, h, 0x1234, m, 0, b, r);
  for (unsigned h : {0u, 2u, 3u, 4u, 6u})
    for (ULONG m : {0u, 38u, 39u, 40u, 64u})
      for (unsigned r = 0; r < 10; r++)
        for (unsigned b : {0u, 1u, 5u})
          one(3, h, 0x1234, m, 0, b, r);
  printf("complete cases=%u\n", count);
  return 0;
}
