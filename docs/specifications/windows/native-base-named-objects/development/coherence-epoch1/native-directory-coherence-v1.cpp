#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <utility>
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
  HANDLE h = nullptr;
  LONG opened = open(L"\BaseNamedObjects", nullptr, 0x02000000, 0x40, &h);
  printf("selected status=%08lX\n", ULONG(opened));
  if (opened != 0 || !h)
    fail("selected-open");
  using Names = std::vector<std::pair<std::wstring, std::wstring>>;
  std::vector<Names> passes;
  std::vector<bool> unique;
  for (unsigned pass = 0; pass != 6; ++pass) {
    Names names;
    ULONG context = 0;
    for (;;) {
      alignas(void *) unsigned char buffer[65536]{};
      ULONG returned = 0;
      LONG status = query_directory(h, buffer, sizeof buffer, TRUE, FALSE,
                                    &context, &returned);
      if (status == LONG(0x8000001A))
        break;
      if (status != 0 || returned > sizeof buffer || returned < sizeof(Entry) ||
          names.size() >= 8192)
        fail("enumeration-bounds");
      const auto &e = *reinterpret_cast<const Entry *>(buffer);
      if (!inside(buffer, sizeof buffer, e.name) ||
          !inside(buffer, sizeof buffer, e.type))
        fail("enumeration-string");
      printf("entry pass=%u ordinal=%zu context=%lu name=", pass, names.size(),
             context);
      hex(e.name.Buffer, e.name.Length);
      printf(" type=");
      hex(e.type.Buffer, e.type.Length);
      puts("");
      names.emplace_back(std::wstring(e.name.Buffer, e.name.Length / 2),
                         std::wstring(e.type.Buffer, e.type.Length / 2));
    }
    std::sort(names.begin(), names.end());
    bool distinct =
        std::adjacent_find(names.begin(), names.end()) == names.end();
    printf("pass-complete pass=%u count=%zu unique=%u same-previous=%u\n", pass,
           names.size(), unsigned(distinct),
           unsigned(pass != 0 && names == passes.back()));
    unique.push_back(distinct);
    passes.push_back(std::move(names));
  }
  bool accepted = false;
  for (unsigned pair = 0; pair != 3; ++pair) {
    bool stable = unique[pair * 2] && unique[pair * 2 + 1] &&
                  passes[pair * 2] == passes[pair * 2 + 1];
    printf("pair attempt=%u coherent=%u\n", pair, unsigned(stable));
    accepted |= stable;
  }
  close(h);
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    fail("final-handles");
  printf("complete passes=6 bounded-acquisition=%s handles-before=%lu "
         "handles-after=%lu\n",
         accepted ? "accepted" : "unstable", before, after);
  return before == after ? 0 : 22;
}
