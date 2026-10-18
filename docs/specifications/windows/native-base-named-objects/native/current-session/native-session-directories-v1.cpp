#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#include <windows.h>
#include <winternl.h>

using Open = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using Directory = LONG(NTAPI *)(HANDLE, void *, ULONG, BOOLEAN, BOOLEAN,
                                ULONG *, ULONG *);
using Close = LONG(NTAPI *)(HANDLE);
using Version = LONG(NTAPI *)(PRTL_OSVERSIONINFOW);
struct Entry {
  UNICODE_STRING name, type;
};
static Open open_directory;
static Query query_object, query_token;
static Directory query_directory;
static Close close_handle;
static void fail(const char *what) {
  printf("failure=%s win32=%lu\n", what, GetLastError());
  ExitProcess(21);
}
static void hex(const void *p, size_t n) {
  auto b = static_cast<const unsigned char *>(p);
  for (size_t i = 0; i != n; ++i)
    printf("%02X", unsigned(b[i]));
}
static LONG open(const std::wstring &path, HANDLE root, ULONG access,
                 ULONG flags, HANDLE *out) {
  UNICODE_STRING name{};
  name.Buffer = const_cast<PWSTR>(path.c_str());
  name.Length = USHORT(path.size() * sizeof(wchar_t));
  name.MaximumLength = name.Length + sizeof(wchar_t);
  OBJECT_ATTRIBUTES attributes{};
  attributes.Length = sizeof attributes;
  attributes.RootDirectory = root;
  attributes.ObjectName = &name;
  attributes.Attributes = flags;
  return open_directory(out, access, &attributes);
}
static void close(HANDLE h) {
  if (close_handle(h) != 0)
    fail("close");
}
static bool inside(const unsigned char *start, size_t size,
                   const UNICODE_STRING &s) {
  auto at = reinterpret_cast<uintptr_t>(s.Buffer),
       low = reinterpret_cast<uintptr_t>(start);
  return s.Length % 2 == 0 && at % 2 == 0 && at >= low && at - low <= size &&
         s.Length <= size - (at - low);
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    fail("profile");
  auto ntdll = GetModuleHandleW(L"ntdll.dll");
  open_directory =
      reinterpret_cast<Open>(GetProcAddress(ntdll, "NtOpenDirectoryObject"));
  query_object =
      reinterpret_cast<Query>(GetProcAddress(ntdll, "NtQueryObject"));
  query_token =
      reinterpret_cast<Query>(GetProcAddress(ntdll, "NtQueryInformationToken"));
  query_directory = reinterpret_cast<Directory>(
      GetProcAddress(ntdll, "NtQueryDirectoryObject"));
  close_handle = reinterpret_cast<Close>(GetProcAddress(ntdll, "NtClose"));
  auto version =
      reinterpret_cast<Version>(GetProcAddress(ntdll, "RtlGetVersion"));
  if (!open_directory || !query_object || !query_token || !query_directory ||
      !close_handle || !version)
    fail("exports");
  RTL_OSVERSIONINFOW os{};
  os.dwOSVersionInfoSize = sizeof os;
  if (version(&os) != 0)
    fail("version");
  HANDLE token = nullptr;
  if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token))
    fail("query-token");
  ULONG session = 0, private_namespace = 0, length = 0;
  if (query_token(token, 12, &session, 4, &length) != 0 || length != 4)
    fail("token-session");
  if (query_token(token, 42, &private_namespace, 4, &length) != 0 ||
      length != 4)
    fail("token-namespace");
  close(token);
  DWORD process_session = 0, before = 0, after = 0;
  if (!ProcessIdToSessionId(GetCurrentProcessId(), &process_session) ||
      !GetProcessHandleCount(GetCurrentProcess(), &before))
    fail("process-identity");
  printf("identity profile=%s pointer-bytes=%zu build=%lu.%lu.%lu "
         "token-session=%lu process-session=%lu private-namespace=%lu\n",
         argv[1], sizeof(void *), os.dwMajorVersion, os.dwMinorVersion,
         os.dwBuildNumber, session, process_session, private_namespace);
  const std::wstring session_root = L"\\Sessions\\" + std::to_wstring(session);
  std::vector<std::wstring> paths = {L"\\BaseNamedObjects",
                                     L"\\Sessions",
                                     session_root,
                                     session_root + L"\\BaseNamedObjects",
                                     session_root +
                                         L"\\BaseNamedObjects\\Restricted",
                                     L"\\BaseNamedObjects\\Restricted"};
  unsigned ordinal = 0, total_entries = 0, opened = 0;
  for (const auto &path : paths) {
    printf("path ordinal=%u name=", ordinal);
    hex(path.data(), path.size() * 2);
    puts("");
    for (ULONG flags : {0u, 0x40u, 0x100u, 0x140u}) {
      for (ULONG access :
           {0u, 1u, 2u, 3u, 4u, 8u, 15u, 16u, 0x20000u, 0xF000Fu, 0x10000000u,
            0x80000000u, 0x40000000u, 0x20000000u, 0x01000000u, 0x02000000u}) {
        HANDLE h = nullptr;
        LONG status = open(path, nullptr, access, flags, &h);
        printf("open path=%u flags=%08lX access=%08lX status=%08lX", ordinal,
               flags, access, ULONG(status));
        if (status >= 0) {
          if (!h)
            fail("null-successful-handle");
          alignas(void *) unsigned char basic[56]{};
          ULONG returned = 0;
          LONG result = query_object(h, 0, basic, sizeof basic, &returned);
          printf(" basic-status=%08lX basic-bytes=%lu basic=", ULONG(result),
                 returned);
          hex(basic, sizeof basic);
          if (result != 0 || returned != sizeof basic)
            fail("object-basic");
          close(h);
          ++opened;
        } else if (h)
          fail("failed-output-changed");
        puts("");
      }
    }
    HANDLE h = nullptr;
    LONG status = open(path, nullptr, 0x02000000, 0x40, &h);
    if (status >= 0) {
      if (!h)
        fail("enumeration-null-handle");
      ULONG context = 0;
      unsigned count = 0;
      for (;;) {
        alignas(void *) unsigned char buffer[65536]{};
        ULONG returned = 0;
        LONG result = query_directory(h, buffer, sizeof buffer, TRUE, FALSE,
                                      &context, &returned);
        if (result == LONG(0x8000001A)) {
          printf("enumeration-complete path=%u count=%u context=%lu "
                 "status=%08lX\n",
                 ordinal, count, context, ULONG(result));
          break;
        }
        if (result != 0 || returned > sizeof buffer ||
            returned < sizeof(Entry) || count >= 8192)
          fail("enumeration-bounds");
        const auto &e = *reinterpret_cast<const Entry *>(buffer);
        if (!inside(buffer, sizeof buffer, e.name) ||
            !inside(buffer, sizeof buffer, e.type))
          fail("enumeration-string");
        printf("entry path=%u ordinal=%u context=%lu name=", ordinal, count,
               context);
        hex(e.name.Buffer, e.name.Length);
        printf(" type=");
        hex(e.type.Buffer, e.type.Length);
        puts("");
        ++count;
        ++total_entries;
      }
      close(h);
    } else
      printf("enumeration-unavailable path=%u status=%08lX\n", ordinal,
             ULONG(status));
    ++ordinal;
  }
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    fail("final-handle-count");
  printf("complete paths=%u opens=%u entries=%u handles-before=%lu "
         "handles-after=%lu\n",
         ordinal, opened, total_entries, before, after);
  return before == after ? 0 : 22;
}
