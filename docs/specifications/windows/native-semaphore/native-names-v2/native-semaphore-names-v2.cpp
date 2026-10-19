#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <climits>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#include <windows.h>
#include <winternl.h>

using Create = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *, LONG, LONG);
using Open = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using Close = LONG(NTAPI *)(HANDLE);
using OpenDirectory = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);
static Create create_semaphore;
static Open open_semaphore;
static Query query_object, query_semaphore;
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
    if (name->Length != 0 &&
        (begin < reinterpret_cast<uintptr_t>(data) ||
         begin + name->Length >
             reinterpret_cast<uintptr_t>(data + sizeof data)))
      ExitProcess(40);
    hex(name->Buffer, name->Length);
  }
  puts("");
}

static LONG invoke(bool opening, HANDLE *handle, ULONG access,
                   OBJECT_ATTRIBUTES *attrs, LONG initial, LONG maximum,
                   ULONG *escaped) {
  __try {
    return opening ? open_semaphore(handle, access, attrs)
                   : create_semaphore(handle, access, attrs, initial, maximum);
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}

static void run(const char *label, bool open, HANDLE root, std::wstring name,
                ULONG flags = 0, ULONG access = 0x1F0003, LONG initial = 0,
                bool null_name = false, ULONG length_override = 0,
                int maximum_delta = 0, bool null_attrs = false,
                LONG maximum = 2) {
  auto text = counted(name);
  text.MaximumLength = USHORT(int(text.Length) + maximum_delta);
  auto attrs = attributes(root, null_name ? nullptr : &text, flags);
  if (length_override)
    attrs.Length = length_override;
  HANDLE handle = reinterpret_cast<HANDLE>(uintptr_t(-1));
  ULONG escaped = 0;
  LONG status = invoke(open, &handle, access, null_attrs ? nullptr : &attrs,
                       initial, maximum, &escaped);
  printf("attribute-pointer ordinal=%zu null=%u escaped=%08lX\n", ordinal,
         unsigned(null_attrs), escaped);
  printf("named ordinal=%zu label=%s operation=%s flags=%08lX access=%08lX "
         "initial=%ld max=%ld attrs-length=%lu root=%llX null-name=%u name=",
         ordinal, label, open ? "open" : "create", flags, access, initial,
         maximum, attrs.Length,
         static_cast<unsigned long long>(uintptr_t(root)), unsigned(null_name));
  hex(name.data(), name.size() * sizeof(wchar_t));
  printf(" maximum=%u status=%08lX output=%llX\n", text.MaximumLength,
         ULONG(status), static_cast<unsigned long long>(uintptr_t(handle)));
  if (status >= 0) {
    if (!handle || handle == reinterpret_cast<HANDLE>(uintptr_t(-1)))
      ExitProcess(41);
    owned.push_back(handle);
    object_name(handle, ordinal);
    LONG counts[2] = {-1, -1};
    const LONG qs = query_semaphore(handle, 0, counts, sizeof counts, nullptr);
    PUBLIC_OBJECT_BASIC_INFORMATION basic{};
    const LONG bs = query_object(handle, 0, &basic, sizeof basic, nullptr);
    printf("state ordinal=%zu status=%08lX count=%ld max=%ld "
           "object-status=%08lX grant=%08lX\n",
           ordinal, ULONG(qs), counts[0], counts[1], ULONG(bs),
           basic.GrantedAccess);
  }
  ++ordinal;
}

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 20;
  const auto module = GetModuleHandleW(L"ntdll.dll");
  create_semaphore =
      reinterpret_cast<Create>(GetProcAddress(module, "NtCreateSemaphore"));
  open_semaphore =
      reinterpret_cast<Open>(GetProcAddress(module, "NtOpenSemaphore"));
  query_object =
      reinterpret_cast<Query>(GetProcAddress(module, "NtQueryObject"));
  close_object = reinterpret_cast<Close>(GetProcAddress(module, "NtClose"));
  query_semaphore =
      reinterpret_cast<Query>(GetProcAddress(module, "NtQuerySemaphore"));
  const auto open_directory = reinterpret_cast<OpenDirectory>(
      GetProcAddress(module, "NtOpenDirectoryObject"));
  if (!create_semaphore || !open_semaphore || !query_object ||
      !query_semaphore || !close_object || !open_directory)
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
  swprintf_s(buffer, L"AssistSemaphoreNames_%lu_%llu", GetCurrentProcessId(),
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
        run("relative-existing", open, root, name, flags, 0x1F0003, 1);
    }
    for (const auto &name :
         {L"\\BaseNamedObjects\\" + leaf, L"\\BaseNamedObjects\\Local\\" + leaf,
          L"\\BaseNamedObjects\\Global\\" + leaf}) {
      run("absolute-no-root", open, nullptr, name, 0x40);
      run("absolute-with-root", open, root, name, 0x40);
    }
  }
  for (bool open : {false, true}) {
    for (const auto &name : std::vector<std::wstring>{
             std::wstring(), L"Local\\", L"Global\\", L"\\", leaf + L"\\child",
             L"Missing\\" + leaf})
      run("path-boundary", open, root, name);
    run("null-name-root", open, root, L"", 0, 0x1F0003, 0, true);
    run("null-name-no-root", open, nullptr, L"", 0, 0x1F0003, 0, true);
    run("empty-name-no-root", open, nullptr, L"");
    for (HANDLE bad :
         {reinterpret_cast<HANDLE>(uintptr_t(0x1234)), GetCurrentProcess()})
      run("invalid-root", open, bad, leaf);
    run("short-maximum", open, root, leaf, 0xC0, 0x1F0003, 0, false, 0, -2);
    for (ULONG length : {1ul, ULONG(sizeof(OBJECT_ATTRIBUTES) - 1),
                         ULONG(sizeof(OBJECT_ATTRIBUTES) + 1), 0xFFFFFFFFul})
      run("attributes-length", open, root, leaf, 0xC0, 0x1F0003, 0, false,
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

  for (HANDLE root_kind : {static_cast<HANDLE>(nullptr), root,
                           reinterpret_cast<HANDLE>(uintptr_t(0x1234)),
                           GetCurrentProcess(), event, owned[1]}) {
    for (bool opening : {false, true})
      for (bool missing_name : {false, true})
        run("empty-name-root-kind", opening, root_kind, L"", 0, 0x1F0003, 0,
            missing_name);
  }
  for (bool opening : {false, true})
    for (ULONG flags : {0ul, 0xC0ul})
      run("null-attributes", opening, nullptr, L"", flags, 0x1F0003, 0, false,
          0, 0, true);
  for (ULONG rights : {0ul, 1ul, 2ul, 4ul, 0xFul, 0xF000Ful}) {
    HANDLE weak = nullptr;
    const BOOL duplicated =
        DuplicateHandle(GetCurrentProcess(), root, GetCurrentProcess(), &weak,
                        rights, FALSE, 0);
    printf("root-duplicate rights=%08lX success=%u error=%lu\n", rights,
           unsigned(duplicated), duplicated ? 0ul : GetLastError());
    if (duplicated) {
      owned.push_back(weak);
      run("weak-root-create", false, weak,
          leaf + L"_Rights_" + std::to_wstring(rights));
      run("weak-root-open", true, weak, leaf);
    }
  }
  size_t early_closed = 0;
  const auto early_name = leaf + L"_LastClose";
  const auto count_before = owned.size();
  run("owned-before-last-close", false, root, early_name, 0, 0x1F0003, 1);
  if (owned.size() != count_before + 1)
    return 28;
  if (close_object(owned.back()) != 0)
    return 29;
  owned.pop_back();
  ++early_closed;
  run("open-after-owned-last-close", true, root, early_name);
  run("create-after-owned-last-close", false, root, early_name, 0x80);
  const std::vector<std::wstring> raw_suffixes{std::wstring{wchar_t(0), L'A'},
                                               std::wstring{wchar_t(0xD800)},
                                               std::wstring{wchar_t(0xDC00)},
                                               std::wstring{L'/', L'A'},
                                               std::wstring{L'.'},
                                               std::wstring{L'.', L'.'}};
  const auto raw_first = owned.size();
  for (const auto &suffix : raw_suffixes) {
    const auto raw_name = leaf + L"_Raw_" + suffix;
    run("raw-counted-create", false, root, raw_name);
    run("raw-counted-open", true, root, raw_name);
  }
  using Compare = BOOL(WINAPI *)(HANDLE, HANDLE);
  const auto compare = reinterpret_cast<Compare>(GetProcAddress(
      GetModuleHandleW(L"kernelbase.dll"), "CompareObjectHandles"));
  if (!compare || owned.size() != raw_first + raw_suffixes.size() * 2)
    return 30;
  for (size_t index = 0; index < raw_suffixes.size(); ++index) {
    const auto input = L"Local\\" + leaf + L"_Raw_" + raw_suffixes[index];
    for (bool opening : {false, true}) {
      SetLastError(0xA5A5A5A5);
      HANDLE h = opening ? OpenSemaphoreW(0x1F0003, FALSE, input.c_str())
                         : CreateSemaphoreW(nullptr, 0, 2, input.c_str());
      const auto error = GetLastError();
      if (!h)
        return 31;
      const auto native = owned[raw_first + index * 2];
      const BOOL same = compare(native, h);
      printf(
          "win32-raw role=%zu operation=%s error=%08lX same-native=%u input=",
          index, opening ? "open" : "create", error, unsigned(same));
      hex(input.data(), input.size() * sizeof(wchar_t));
      puts("");
      object_name(h, ordinal + index * 2 + size_t(opening));
      owned.push_back(h);
    }
  }
  for (LONG initial : {-1l, 0l, 1l, 3l, LONG_MIN, LONG_MAX})
    for (LONG maximum : {0l, 2l, LONG_MAX})
      for (ULONG flags : {0ul, 0x80ul})
        run("existing-count-validation", false, root, leaf, flags, 0x1F0003,
            initial, false, 0, 0, false, maximum);
  for (ULONG access :
       {0ul, 1ul, 2ul, 3ul, 4ul, 0x100000ul, 0x1F0003ul, 0xFFFFFFFFul,
        0x80000000ul, 0x40000000ul, 0x20000000ul, 0x10000000ul, 0x02000000ul,
        0x01000000ul, 0x00800000ul, 0x000F0000ul})
    for (bool opening : {false, true})
      run("existing-access", opening, root, leaf, 0x80, access, 1);
  auto bridge_name = L"Local\\" + leaf + L"_Bridge";
  HANDLE bridge = CreateSemaphoreW(nullptr, 1, 3, bridge_name.c_str());
  if (!bridge || GetLastError() == ERROR_ALREADY_EXISTS)
    return 32;
  owned.push_back(bridge);
  auto bridge_text = counted(bridge_name);
  auto bridge_attrs = attributes(root, &bridge_text, 0);
  HANDLE native_bridge = nullptr;
  const LONG bridge_status =
      open_semaphore(&native_bridge, 0x1F0003, &bridge_attrs);
  printf("bridge-open status=%08lX same=%u\n", ULONG(bridge_status),
         unsigned(bridge_status >= 0 && compare(bridge, native_bridge)));
  if (bridge_status < 0)
    return 33;
  owned.push_back(native_bridge);
  using Release = LONG(NTAPI *)(HANDLE, LONG, LONG *);
  const auto release_sem =
      reinterpret_cast<Release>(GetProcAddress(module, "NtReleaseSemaphore"));
  if (!release_sem)
    return 34;
  for (unsigned stage = 0; stage < 4; ++stage) {
    LONG previous = -1;
    const ULONG action =
        stage == 0   ? WaitForSingleObject(bridge, 0)
        : stage == 1 ? ULONG(release_sem(native_bridge, 2, &previous))
        : stage == 2 ? ULONG(ReleaseSemaphore(bridge, 1, &previous))
                     : WaitForSingleObject(native_bridge, 0);
    LONG counts[2] = {-1, -1};
    const LONG qs =
        query_semaphore(native_bridge, 0, counts, sizeof counts, nullptr);
    printf("bridge-state stage=%u action=%08lX previous=%ld status=%08lX "
           "count=%ld max=%ld\n",
           stage, action, previous, ULONG(qs), counts[0], counts[1]);
  }
  size_t closed = early_closed;
  for (auto it = owned.rbegin(); it != owned.rend(); ++it) {
    if (close_object(*it) != 0)
      return 25;
    ++closed;
  }
  DWORD after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 26;
  printf("complete cases=%zu acquired=%zu closed=%zu before=%lu after=%lu\n",
         ordinal, owned.size() + early_closed, closed, before, after);
  return before == after && closed == owned.size() + early_closed ? 0 : 27;
}
