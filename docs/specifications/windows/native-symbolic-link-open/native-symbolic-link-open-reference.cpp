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
static Open directory_nt;
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
  static const wchar_t *paths[] = {L"\\", L"\\KnownDlls", L"\\KnownDlls32",
                                   L"\\KnownDlls\\KnownDllPath",
                                   L"\\KnownDlls32\\KnownDllPath"};
  HANDLE h = nullptr;
  UNICODE_STRING n{};
  n.Length = USHORT(wcslen(paths[parent]) * 2);
  n.MaximumLength = n.Length + 2;
  n.Buffer = const_cast<PWSTR>(paths[parent]);
  OBJECT_ATTRIBUTES a{};
  a.Length = sizeof(a);
  a.ObjectName = &n;
  a.Attributes = 0x40;
  LONG status = (parent < 3 ? directory_nt : open_nt)(&h, access, &a);
  if (status < 0 || !h)
    fail("base");
  return h;
}
static void close(HANDLE h) {
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
  LONG status = open(&out, 1, names[name], reinterpret_cast<HANDLE>(selected));
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
static void duplicate(unsigned parent, ULONG source_access, ULONG desired,
                      ULONG options) {
  DWORD hb = 0, ha = 0;
  GetProcessHandleCount(GetCurrentProcess(), &hb);
  HANDLE source = base(parent, source_access),
         out = reinterpret_cast<HANDLE>(uintptr_t(0xA5A5A5A5A5A5A5A5ull));
  ULONG source_grant = grant(source);
  SetLastError(0xBADC0DE);
  BOOL ok = DuplicateHandle(GetCurrentProcess(), source, GetCurrentProcess(),
                            &out, desired, TRUE, options);
  DWORD error = GetLastError(), source_flags = 0,
        source_live = GetHandleInformation(source, &source_flags) ? 1 : 0,
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
         "source-grant=%08lX desired=%08lX options=%08lX ok=%u error=%lu "
         "output=%llX grant=%08lX flags=%lu source-live=%lu handles-before=%lu "
         "handles-after=%lu\n",
         count++, sizeof(void *), parent, source_access, source_grant, desired,
         options, ok, error, (unsigned long long)observed, g, flags,
         source_live, hb, ha);
  if (hb != ha)
    fail("lifetime");
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto n = GetModuleHandleW(L"ntdll.dll");
  open_nt =
      reinterpret_cast<Open>(GetProcAddress(n, "NtOpenSymbolicLinkObject"));
  directory_nt =
      reinterpret_cast<Open>(GetProcAddress(n, "NtOpenDirectoryObject"));
  query = reinterpret_cast<Query>(GetProcAddress(n, "NtQueryObject"));
  close_nt = reinterpret_cast<Close>(GetProcAddress(n, "NtClose"));
  if (!open_nt || !directory_nt || !query || !close_nt)
    fail("exports");
  for (unsigned parent = 0; parent < 5; parent++)
    for (ULONG access : {1u, 0x20000u, 0x02000000u})
      for (unsigned tag = 0; tag < 6; tag++)
        for (unsigned name = 0; name < 8; name++)
          rootcase(parent, access, tag, name);
  printf("complete cases=%u\n", count);
  return 0;
}
