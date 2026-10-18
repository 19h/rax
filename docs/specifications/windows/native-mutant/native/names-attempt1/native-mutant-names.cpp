#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#include <windows.h>
#include <winternl.h>

using Create = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *, ULONG);
using Open = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using Close = LONG(NTAPI *)(HANDLE);
using OpenDirectory = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);
static Create create_mutant;
static Open open_mutant;
static Query query_object;
static Close close_object;
static std::vector<HANDLE> owned;
static size_t ordinal;

static void hex(const void *data, size_t count) {
  const auto p = static_cast<const unsigned char *>(data);
  for (size_t i = 0; i < count; ++i)
    printf("%02X", unsigned(p[i]));
}

static OBJECT_ATTRIBUTES attributes(HANDLE root, UNICODE_STRING *name,
                                    ULONG flags) {
  OBJECT_ATTRIBUTES result{};
  result.Length = sizeof result;
  result.RootDirectory = root;
  result.ObjectName = name;
  result.Attributes = flags;
  return result;
}

static UNICODE_STRING counted(std::wstring &name) {
  UNICODE_STRING result{};
  result.Length = USHORT(name.size() * sizeof(wchar_t));
  result.MaximumLength = result.Length;
  result.Buffer = name.data();
  return result;
}

static void object_name(HANDLE handle, size_t index) {
  alignas(16) unsigned char data[4096];
  memset(data, 0xA5, sizeof data);
  ULONG returned = 0xA5A5A5A5;
  LONG status = query_object(handle, 1, data, sizeof data, &returned);
  printf("object-name ordinal=%zu status=%08lX returned=%08lX units=", index,
         ULONG(status), returned);
  if (status >= 0) {
    const auto name = reinterpret_cast<const UNICODE_STRING *>(data);
    const auto begin = reinterpret_cast<uintptr_t>(name->Buffer);
    if (begin < reinterpret_cast<uintptr_t>(data) ||
        begin + name->Length > reinterpret_cast<uintptr_t>(data + sizeof data))
      ExitProcess(40);
    hex(name->Buffer, name->Length);
  }
  puts("");
}

static void run(const char *label, bool open, HANDLE root, std::wstring name,
                ULONG flags = 0, ULONG access = 0x1F0001, ULONG owner = 0,
                bool null_name = false, ULONG length_override = 0,
                int maximum_delta = 0) {
  auto text = counted(name);
  text.MaximumLength = USHORT(int(text.Length) + maximum_delta);
  auto attrs = attributes(root, null_name ? nullptr : &text, flags);
  if (length_override)
    attrs.Length = length_override;
  HANDLE handle = reinterpret_cast<HANDLE>(uintptr_t(-1));
  LONG status = open ? open_mutant(&handle, access, &attrs)
                     : create_mutant(&handle, access, &attrs, owner);
  printf("named ordinal=%zu label=%s operation=%s flags=%08lX access=%08lX "
         "owner=%08lX attrs-length=%lu root=%llX null-name=%u name=",
         ordinal, label, open ? "open" : "create", flags, access, owner,
         attrs.Length, static_cast<unsigned long long>(uintptr_t(root)),
         unsigned(null_name));
  hex(name.data(), name.size() * sizeof(wchar_t));
  printf(" maximum=%u status=%08lX output=%llX\n", text.MaximumLength,
         ULONG(status), static_cast<unsigned long long>(uintptr_t(handle)));
  if (status >= 0) {
    if (!handle || handle == reinterpret_cast<HANDLE>(uintptr_t(-1)))
      ExitProcess(41);
    owned.push_back(handle);
    object_name(handle, ordinal);
  }
  ++ordinal;
}

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 20;
  const auto module = GetModuleHandleW(L"ntdll.dll");
  create_mutant =
      reinterpret_cast<Create>(GetProcAddress(module, "NtCreateMutant"));
  open_mutant = reinterpret_cast<Open>(GetProcAddress(module, "NtOpenMutant"));
  query_object =
      reinterpret_cast<Query>(GetProcAddress(module, "NtQueryObject"));
  close_object = reinterpret_cast<Close>(GetProcAddress(module, "NtClose"));
  const auto open_directory = reinterpret_cast<OpenDirectory>(
      GetProcAddress(module, "NtOpenDirectoryObject"));
  if (!create_mutant || !open_mutant || !query_object || !close_object ||
      !open_directory)
    return 21;
  DWORD before = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 22;
  std::wstring root_name = L"\\BaseNamedObjects";
  auto root_string = counted(root_name);
  auto root_attrs = attributes(nullptr, &root_string, 0x40);
  HANDLE root = nullptr;
  if (open_directory(&root, 3, &root_attrs) != 0)
    return 23;
  owned.push_back(root);
  wchar_t buffer[160];
  swprintf_s(buffer, L"AssistMutantNames_%lu_%llu", GetCurrentProcessId(),
             static_cast<unsigned long long>(GetTickCount64()));
  std::wstring leaf = buffer;
  std::wstring flipped = leaf;
  for (auto &unit : flipped) {
    if (unit >= L'A' && unit <= L'Z')
      unit = wchar_t(unit + 32);
    else if (unit >= L'a' && unit <= L'z')
      unit = wchar_t(unit - 32);
  }
  printf("profile=%s pointer-bytes=%zu pid=%lu tid=%lu root=%llX leaf=",
         argv[1], sizeof(void *), GetCurrentProcessId(), GetCurrentThreadId(),
         static_cast<unsigned long long>(uintptr_t(root)));
  hex(leaf.data(), leaf.size() * 2);
  puts("");
  run("first", false, root, leaf);
  for (bool open : {false, true}) {
    for (const auto &name :
         {leaf, L"Local\\" + leaf, L"Global\\" + leaf, flipped,
          L"Local\\" + flipped, L"Global\\" + flipped}) {
      for (ULONG flags : {0ul, 0x40ul, 0x80ul, 0xC0ul})
        run("relative-existing", open, root, name, flags, 0x1F0001, 1);
    }
    for (const auto &name :
         {L"\\BaseNamedObjects\\" + leaf, L"\\BaseNamedObjects\\Local\\" + leaf,
          L"\\BaseNamedObjects\\Global\\" + leaf}) {
      run("absolute-no-root", open, nullptr, name, 0x40);
      run("absolute-with-root", open, root, name, 0x40);
    }
  }
  for (bool open : {false, true}) {
    for (const auto &name : {std::wstring(), L"Local\\", L"Global\\", L"\\",
                             leaf + L"\\child", L"Missing\\" + leaf})
      run("path-boundary", open, root, name);
    run("null-name-root", open, root, L"", 0, 0x1F0001, 0, true);
    run("null-name-no-root", open, nullptr, L"", 0, 0x1F0001, 0, true);
    run("empty-name-no-root", open, nullptr, L"");
    for (HANDLE bad :
         {reinterpret_cast<HANDLE>(uintptr_t(0x1234)), GetCurrentProcess()})
      run("invalid-root", open, bad, leaf);
    run("short-maximum", open, root, leaf, 0xC0, 0x1F0001, 0, false, 0, -2);
    for (ULONG length : {1ul, ULONG(sizeof(OBJECT_ATTRIBUTES) - 1),
                         ULONG(sizeof(OBJECT_ATTRIBUTES) + 1), 0xFFFFFFFFul})
      run("attributes-length", open, root, leaf, 0xC0, 0x1F0001, 0, false,
          length);
  }
  std::wstring event_path = L"Local\\" + leaf + L"_Event";
  HANDLE event = CreateEventW(nullptr, FALSE, FALSE, event_path.c_str());
  if (!event || GetLastError() == ERROR_ALREADY_EXISTS)
    return 24;
  owned.push_back(event);
  for (bool open : {false, true})
    for (ULONG flags : {0ul, 0x40ul, 0x80ul, 0xC0ul})
      run("event-type-collision", open, root, event_path, flags);
  size_t closed = 0;
  for (auto it = owned.rbegin(); it != owned.rend(); ++it) {
    if (close_object(*it) != 0)
      return 25;
    ++closed;
  }
  DWORD after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 26;
  printf("complete cases=%zu acquired=%zu closed=%zu before=%lu after=%lu\n",
         ordinal, owned.size(), closed, before, after);
  return before == after && closed == owned.size() ? 0 : 27;
}
