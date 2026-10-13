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
using Open = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using Close = LONG(NTAPI *)(HANDLE);
static Open open_nt;
static Query query, query_process;
static ULONG section_type;
static Close close_nt;
static unsigned count;
static const ULONG masks[] = {
    0,          1,          2,          4,          8,          0x10,
    0x20,       0x40,       0x80,       0xD,        0x1F,       0xFFFF,
    0x10000,    0x20000,    0x40000,    0x80000,    0xF0000,    0xF001F,
    0x100000,   0x1F001F,   0x01000000, 0x02000000, 0x03000000, 0x04000000,
    0x08000000, 0x80000000, 0x40000000, 0x20000000, 0x10000000, 0xC0000000,
    0xA0000000, 0xE0000000, 0xFFFFFFFF, 0x0200000D, 0x0100000D, 0x0010000D,
    0x0002000D};
static void fail(const char *s) {
  printf("failure=%s error=%lu\n", s, GetLastError());
  ExitProcess(21);
}
struct Entry {
  HANDLE h;
  uintptr_t handles, pointers;
  ULONG access, type, attributes, reserved;
};
static std::vector<Entry> entries() {
  std::vector<uintptr_t> b(131072);
  ULONG returned = 0;
  LONG s = query_process(GetCurrentProcess(), 51, b.data(),
                         ULONG(b.size() * sizeof(uintptr_t)), &returned);
  if (s < 0 || b[0] > 8192 || returned > b.size() * sizeof(uintptr_t) ||
      returned <
          (2 + b[0] * (sizeof(Entry) / sizeof(uintptr_t))) * sizeof(uintptr_t))
    fail("handle-snapshot");
  auto first = reinterpret_cast<Entry *>(b.data() + 2);
  return {first, first + b[0]};
}
static std::set<uintptr_t> sections(const std::vector<Entry> &e) {
  std::set<uintptr_t> s;
  for (auto x : e)
    if (x.type == section_type)
      s.insert(uintptr_t(x.h));
  return s;
}
static LONG open(HANDLE *h, ULONG access, const wchar_t *path,
                 HANDLE root = nullptr) {
  UNICODE_STRING n{};
  n.Length = USHORT(wcslen(path) * 2);
  n.MaximumLength = n.Length + 2;
  n.Buffer = const_cast<PWSTR>(path);
  OBJECT_ATTRIBUTES a{};
  a.Length = sizeof(a);
  a.RootDirectory = root;
  a.ObjectName = &n;
  a.Attributes = 0x40;
  return open_nt(h, access, &a);
}
static HANDLE base(unsigned parent, ULONG access) {
  HANDLE h = nullptr;
  if (open(&h, access,
           parent == 0   ? L"\\KnownDlls\\kernel32.dll"
           : parent == 1 ? L"\\KnownDlls32\\kernel32.dll"
                         : L"\\KnownDllsChpe32\\kernel32.dll") < 0 ||
      !h)
    fail("base");
  return h;
}
static void close(HANDLE h) {
  if (h)
    SetHandleInformation(h, HANDLE_FLAG_PROTECT_FROM_CLOSE, 0);
  if (h && close_nt(h) < 0)
    fail("close");
}
static ULONG grant(HANDLE h) {
  ULONG b[14]{}, n = 0;
  if (query(h, 0, b, sizeof(b), &n) < 0 || n != sizeof(b))
    fail("basic");
  return b[1];
}
static BOOL call_duplicate(HANDLE source, HANDLE *out, ULONG desired,
                           ULONG options, ULONG *exception) {
  __try {
    return DuplicateHandle(GetCurrentProcess(), source, GetCurrentProcess(),
                           out, desired, TRUE, options);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
    return FALSE;
  }
}
static void duplicate(unsigned parent, ULONG source_access, ULONG desired,
                      ULONG options, ULONG source_flags) {
  DWORD hb = 0, ha = 0;
  GetProcessHandleCount(GetCurrentProcess(), &hb);
  auto before_sections = sections(entries());
  HANDLE source = base(parent, source_access),
         out = reinterpret_cast<HANDLE>(uintptr_t(0xA5A5A5A5A5A5A5A5ull));
  ULONG source_grant = grant(source);
  if (!SetHandleInformation(source, 3, source_flags))
    fail("source-flags");
  SetLastError(0xBADC0DE);
  ULONG exception = 0;
  BOOL ok = call_duplicate(source, &out, desired, options, &exception);
  DWORD error = GetLastError(), observed_source_flags = 0,
        source_live =
            GetHandleInformation(source, &observed_source_flags) ? 1 : 0,
        flags = 0;
  ULONG g = ok ? grant(out) : 0;
  uintptr_t observed = uintptr_t(out);
  if (ok && (options & 1) && out == source)
    source_live = 0;
  if (ok) {
    if (!GetHandleInformation(out, &flags))
      fail("flags");
    close(out);
  }
  if (source_live)
    close(source);
  GetProcessHandleCount(GetCurrentProcess(), &ha);
  printf("case=%u width=%zu family=1 parent=%u source-access=%08lX "
         "source-grant=%08lX source-flags=%08lX exception=%08lX desired=%08lX "
         "options=%08lX ok=%u error=%lu output=%llX grant=%08lX flags=%lu "
         "source-live=%lu handles-before=%lu handles-after=%lu\n",
         count++, sizeof(void *), parent, source_access, source_grant,
         source_flags, exception, desired, options, ok, error,
         (unsigned long long)observed, g, flags, source_live, hb, ha);
  auto remaining = entries();
  for (auto e : remaining)
    if (e.type == section_type && !before_sections.count(uintptr_t(e.h)))
      close(e.h);
  bool exact = sections(entries()) == before_sections;
  printf("restored case=%u exact-section-handles=%u global-exact=%u\n",
         count - 1, unsigned(exact), unsigned(hb == ha));
  if (!exact)
    fail("lifetime");
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto n = GetModuleHandleW(L"ntdll.dll");
  open_nt = reinterpret_cast<Open>(GetProcAddress(n, "NtOpenSection"));
  query = reinterpret_cast<Query>(GetProcAddress(n, "NtQueryObject"));
  close_nt = reinterpret_cast<Close>(GetProcAddress(n, "NtClose"));
  query_process = Query(GetProcAddress(n, "NtQueryInformationProcess"));
  if (!open_nt || !query || !close_nt || !query_process)
    fail("exports");
  HANDLE warmup = base(0, 1);
  for (auto e : entries())
    if (e.h == warmup)
      section_type = e.type;
  close(warmup);
  if (!section_type)
    fail("section-type-index");
  for (unsigned parent = 0; parent < 3; parent++)
    for (ULONG source : {1u, 0xDu, 0x20000u, 0x02000000u})
      for (ULONG desired : masks)
        for (ULONG options : {0u, 1u, 2u, 3u, 4u, 5u, 6u, 7u, 8u, 0xFFFFFFFFu})
          for (ULONG flags : {0u, 1u, 2u, 3u})
            duplicate(parent, source, desired, options, flags);
  printf("complete cases=%u\n", count);
  return 0;
}
