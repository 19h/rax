#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>
using Open = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using Close = LONG(NTAPI *)(HANDLE);
static Open open_nt;
static Query query;
static Close close_nt;
static unsigned count;
static void fail(const char *s) {
  printf("failure=%s error=%lu\n", s, GetLastError());
  ExitProcess(21);
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
  if (open(&h, access, parent == 0 ? L"\\KnownDlls" : L"\\KnownDlls32") < 0 ||
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
static void rootcase(unsigned parent, ULONG source_access, unsigned tag,
                     unsigned name) {
  static const wchar_t *names[] = {
      L"",   L"KnownDlls",   L"KnownDllPath",        L"ntdll.dll",
      L"\\", L"\\KnownDlls", L"rax-absent-20261012", L"ntdll.dll\\x"};
  DWORD hb = 0, ha = 0;
  GetProcessHandleCount(GetCurrentProcess(), &hb);
  HANDLE root = base(parent, source_access);
  ULONG root_grant = grant(root);
  uintptr_t selected = uintptr_t(root);
  if (tag < 4)
    selected |= tag;
  else if (sizeof(void *) == 8)
    selected |= tag == 4 ? 0x100000000ull : 0xFFFF000000000000ull;
  else
    selected |= tag == 4 ? 0x40000000u : 0xFFFF0000u;
  HANDLE out = reinterpret_cast<HANDLE>(uintptr_t(0xA5A5A5A5A5A5A5A5ull));
  LONG status = open(&out, 3, names[name], reinterpret_cast<HANDLE>(selected));
  ULONG g = status >= 0 ? grant(out) : 0;
  uintptr_t observed = uintptr_t(out);
  if (status >= 0)
    close(out);
  close(root);
  GetProcessHandleCount(GetCurrentProcess(), &ha);
  printf("case=%u width=%zu family=0 parent=%u source-access=%08lX "
         "root-grant=%08lX tag=%u name=%u status=%08lX output=%llX grant=%08lX "
         "handles-before=%lu handles-after=%lu\n",
         count++, sizeof(void *), parent, source_access, root_grant, tag, name,
         ULONG(status), (unsigned long long)observed, g, hb, ha);
  if (hb != ha)
    fail("lifetime");
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
  if (hb != ha)
    fail("lifetime");
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto n = GetModuleHandleW(L"ntdll.dll");
  open_nt = reinterpret_cast<Open>(GetProcAddress(n, "NtOpenDirectoryObject"));
  query = reinterpret_cast<Query>(GetProcAddress(n, "NtQueryObject"));
  close_nt = reinterpret_cast<Close>(GetProcAddress(n, "NtClose"));
  if (!open_nt || !query || !close_nt)
    fail("exports");
  for (unsigned parent = 0; parent < 2; parent++)
    for (ULONG source : {1u, 0x02000000u})
      for (ULONG desired :
           {0u,         1u,         2u,         4u,         8u,
            0x10u,      0x20u,      0x40u,      0x80u,      0x100u,
            0x200u,     0x400u,     0x800u,     0x1000u,    0x2000u,
            0x4000u,    0x8000u,    0xFFFFu,    0x10000u,   0x100000u,
            0x200000u,  0x400000u,  0x800000u,  0x1000000u, 0x2000000u,
            0x4000000u, 0x8000000u, 0xFFFFFFFFu})
        for (ULONG options : {0u, 1u, 2u, 3u})
          for (ULONG flags : {0u, 3u})
            duplicate(parent, source, desired, options, flags);
  printf("complete cases=%u\n", count);
  return 0;
}
