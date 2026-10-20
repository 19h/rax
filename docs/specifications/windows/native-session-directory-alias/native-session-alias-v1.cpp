#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <string>
#include <windows.h>
#include <winternl.h>
using Open = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using Enumerate = LONG(NTAPI *)(HANDLE, void *, ULONG, BOOLEAN, BOOLEAN,
                                ULONG *, ULONG *);
using Close = LONG(NTAPI *)(HANDLE);
using CreateLink = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *,
                                 UNICODE_STRING *);
using CompareObjects = LONG(NTAPI *)(HANDLE, HANDLE);
struct Entry {
  UNICODE_STRING name, type;
};
static Open open_directory, create_directory;
static Query query_object;
static Enumerate enumerate_directory;
static Close close_native;
static void fail(const char *message) {
  printf("failure=%s error=%lu\n", message, GetLastError());
  ExitProcess(21);
}
static void hex(const void *data, size_t size) {
  auto p = static_cast<const unsigned char *>(data);
  for (size_t i = 0; i < size; ++i)
    printf("%02X", p[i]);
}
static bool inside(const unsigned char *base, size_t size,
                   const UNICODE_STRING &text) {
  uintptr_t first = uintptr_t(base), at = uintptr_t(text.Buffer);
  return text.Length % 2 == 0 && at % 2 == 0 && at >= first &&
         at - first <= size && text.Length <= size - (at - first);
}
static LONG open(Open function, const std::wstring &path, HANDLE root,
                 ULONG flags, ULONG access, HANDLE *handle) {
  UNICODE_STRING name{};
  name.Length = USHORT(path.size() * 2);
  name.MaximumLength = name.Length + 2;
  name.Buffer = const_cast<PWSTR>(path.c_str());
  OBJECT_ATTRIBUTES attributes{};
  attributes.Length = sizeof(attributes);
  attributes.RootDirectory = root;
  attributes.ObjectName = &name;
  attributes.Attributes = flags;
  return function(handle, access, &attributes);
}
static void close(HANDLE handle) {
  if (close_native(handle) != 0)
    fail("close");
}
static std::wstring object_name(HANDLE handle, const char *role) {
  alignas(void *) unsigned char bytes[65536]{};
  ULONG returned = 0;
  LONG status = query_object(handle, 1, bytes, sizeof(bytes), &returned);
  if (status != 0 || returned > sizeof(bytes) ||
      returned < sizeof(UNICODE_STRING))
    fail("name-header");
  auto name = reinterpret_cast<const UNICODE_STRING *>(bytes);
  if (!inside(bytes, returned, *name))
    fail("name-bounds");
  printf("object role=%s returned=%lu units=%u name=", role, returned,
         unsigned(name->Length / 2));
  hex(name->Buffer, name->Length);
  printf("\n");
  return std::wstring(name->Buffer, name->Length / 2);
}
static void entries(HANDLE handle, const char *role, bool fixed_roots) {
  ULONG context = 0;
  unsigned count = 0;
  for (;;) {
    alignas(void *) unsigned char bytes[65536]{};
    ULONG returned = 0;
    LONG status = enumerate_directory(handle, bytes, sizeof(bytes), TRUE, FALSE,
                                      &context, &returned);
    if (status == LONG(0x8000001A))
      break;
    if (status != 0 || returned > sizeof(bytes) || returned < sizeof(Entry) ||
        ++count > 8192)
      fail("directory-header");
    auto entry = reinterpret_cast<const Entry *>(bytes);
    if (!inside(bytes, returned, entry->name) ||
        !inside(bytes, returned, entry->type))
      fail("directory-bounds");
    std::wstring name(entry->name.Buffer, entry->name.Length / 2);
    if (fixed_roots) {
      bool selected = false;
      for (const wchar_t *expected :
           {L"KnownDlls", L"KnownDlls32", L"KnownDllsChpe32", L"Windows",
            L"BaseNamedObjects", L"Sessions"})
        selected |= CompareStringOrdinal(name.data(), int(name.size()),
                                         expected, -1, TRUE) == CSTR_EQUAL;
      if (!selected)
        continue;
    }
    printf("entry role=%s name=", role);
    hex(entry->name.Buffer, entry->name.Length);
    printf(" type=");
    hex(entry->type.Buffer, entry->type.Length);
    printf("\n");
  }
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    fail("profile");
  auto module = GetModuleHandleW(L"ntdll.dll");
  open_directory =
      reinterpret_cast<Open>(GetProcAddress(module, "NtOpenDirectoryObject"));
  create_directory =
      reinterpret_cast<Open>(GetProcAddress(module, "NtCreateDirectoryObject"));
  query_object =
      reinterpret_cast<Query>(GetProcAddress(module, "NtQueryObject"));
  enumerate_directory = reinterpret_cast<Enumerate>(
      GetProcAddress(module, "NtQueryDirectoryObject"));
  close_native = reinterpret_cast<Close>(GetProcAddress(module, "NtClose"));
  if (!open_directory || !create_directory || !query_object ||
      !enumerate_directory || !close_native)
    fail("exports");
  printf("profile=%s pointer=%zu attributes=%zu unicode=%zu\n", argv[1],
         sizeof(void *), sizeof(OBJECT_ATTRIBUTES), sizeof(UNICODE_STRING));
  DWORD before = 0, after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    fail("handle-count-before");
  HANDLE root = nullptr;
  if (open(open_directory, L"\\", nullptr, 0x40, 1, &root) != 0)
    fail("root");
  entries(root, "fixed-root", true);
  close(root);
  for (const wchar_t *path :
       {L"\\KnownDlls", L"\\KnownDlls32", L"\\KnownDllsChpe32", L"\\Windows",
        L"\\BaseNamedObjects"}) {
    HANDLE handle = nullptr;
    LONG status = open(open_directory, path, nullptr, 0x40, 1, &handle);
    printf("fixed status=%08lX requested=", status);
    hex(path, wcslen(path) * 2);
    printf("\n");
    if (status == 0) {
      object_name(handle, "fixed-open");
      close(handle);
    }
  }
  std::wstring parent_name = L"\\BaseNamedObjects\\AssistSessionAlias-" +
                             std::to_wstring(GetCurrentProcessId()) + L"-" +
                             std::to_wstring(GetTickCount64());
  HANDLE parent = nullptr, child = nullptr, alias = nullptr;
  if (open(create_directory, parent_name, nullptr, 0x40, 0x000F000F, &parent) !=
      0)
    fail("create-parent");
  if (open(create_directory, L"MiXeD", parent, 0x40, 0x000F000F, &child) != 0)
    fail("create-child");
  entries(parent, "private-parent", false);
  auto original = object_name(child, "private-child");
  LONG status = open(open_directory, L"mIxEd", parent, 0x40, 1, &alias);
  printf("alias status=%08lX flags=00000040 requested=", status);
  hex(L"mIxEd", 10);
  printf("\n");
  if (status != 0 || !alias)
    fail("case-insensitive-alias");
  auto captured = object_name(alias, "private-alias");
  if (captured != original || captured.size() < 6 ||
      captured.substr(captured.size() - 6) != L"\\MiXeD")
    fail("canonical-identity");
  auto create_link = reinterpret_cast<CreateLink>(
      GetProcAddress(module, "NtCreateSymbolicLinkObject"));
  auto compare = reinterpret_cast<CompareObjects>(
      GetProcAddress(module, "NtCompareObjects"));
  if (!create_link || !compare)
    fail("link-identity-exports");
  std::wstring link_name = L"DirectoryLink";
  UNICODE_STRING counted_name{USHORT(link_name.size() * 2),
                              USHORT((link_name.size() + 1) * 2),
                              link_name.data()};
  UNICODE_STRING counted_target{USHORT(original.size() * 2),
                                USHORT((original.size() + 1) * 2),
                                original.data()};
  OBJECT_ATTRIBUTES attributes{
      sizeof(attributes), parent, &counted_name, 0x40, nullptr, nullptr};
  HANDLE symbolic = nullptr;
  LONG created =
      create_link(&symbolic, 0x000F0001, &attributes, &counted_target);
  printf("symbolic-created status=%08lX target=", created);
  hex(original.data(), original.size() * 2);
  printf("\n");
  if (created != 0 || !symbolic)
    fail("create-private-symbolic-link");
  entries(parent, "private-with-symbolic", false);
  HANDLE followed = nullptr;
  LONG opened = open(open_directory, link_name, parent, 0x40, 1, &followed);
  printf("symbolic-directory status=%08lX\n", opened);
  if (opened != 0 || !followed)
    fail("open-symbolic-directory-target");
  auto followed_name = object_name(followed, "followed-directory");
  auto link_object_name = object_name(symbolic, "symbolic-link");
  LONG same = compare(child, followed);
  LONG different = compare(parent, followed);
  printf("identity same=%08lX different=%08lX canonical-same=%u "
         "link-name-distinct=%u\n",
         same, different, unsigned(followed_name == original),
         unsigned(link_object_name != original));
  if (same != 0 || different == 0 || followed_name != original ||
      link_object_name == original)
    fail("native-object-identity");
  HANDLE refused = nullptr;
  LONG reparse = open(open_directory, link_name, parent, 0x1040, 1, &refused);
  printf("dont-reparse status=%08lX opened=%u\n", reparse,
         unsigned(refused != nullptr));
  if (reparse == 0 || refused)
    fail("dont-reparse-must-refuse-link");
  close(followed);
  close(symbolic);
  close(alias);
  close(child);
  close(parent);
  if (!GetProcessHandleCount(GetCurrentProcess(), &after) || after != before)
    fail("handle-count-after");
  printf("finished=1 handle-count-before=%lu handle-count-after=%lu\n", before,
         after);
  return 0;
}
