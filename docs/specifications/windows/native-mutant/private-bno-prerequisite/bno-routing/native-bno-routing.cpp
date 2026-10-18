#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cwchar>
#include <set>
#include <string>
#include <vector>
#include <windows.h>
#include <winternl.h>

using TokenQuery = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using ObjectQuery = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using LinkQuery = LONG(NTAPI *)(HANDLE, UNICODE_STRING *, ULONG *);
using DirectoryOpen = LONG(NTAPI *)(HANDLE *, ACCESS_MASK, OBJECT_ATTRIBUTES *);

struct Owned {
  HANDLE value = nullptr;
  ~Owned() {
    if (value)
      CloseHandle(value);
  }
  bool close() {
    const auto h = value;
    value = nullptr;
    return h && CloseHandle(h);
  }
};

static void text(const std::wstring &value) {
  for (wchar_t unit : value)
    printf("%04X", unsigned(unit));
}

static bool inside(const std::vector<ULONG_PTR> &storage, const void *pointer,
                   size_t bytes) {
  const auto first = reinterpret_cast<uintptr_t>(storage.data());
  const auto at = reinterpret_cast<uintptr_t>(pointer);
  const auto length = storage.size() * sizeof(ULONG_PTR);
  return at >= first && at - first <= length && bytes <= length - (at - first);
}

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 3)
    return 21;
  const auto ntdll = GetModuleHandleW(L"ntdll.dll");
  const auto token_query = reinterpret_cast<TokenQuery>(
      GetProcAddress(ntdll, "NtQueryInformationToken"));
  const auto object_query =
      reinterpret_cast<ObjectQuery>(GetProcAddress(ntdll, "NtQueryObject"));
  const auto directory_open = reinterpret_cast<DirectoryOpen>(
      GetProcAddress(ntdll, "NtOpenDirectoryObject"));
  Owned token;
  if (!token_query || !object_query || !directory_open ||
      !OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token.value))
    return 22;
  std::vector<ULONG_PTR> storage(65536 / sizeof(ULONG_PTR), 0);
  ULONG bytes = 0;
  const auto query_status =
      token_query(token.value, 44, storage.data(), 65536, &bytes);
  if (query_status != 0 || bytes < sizeof(TOKEN_BNO_ISOLATION_INFORMATION) ||
      bytes > 65536)
    return 23;
  const auto info =
      reinterpret_cast<const TOKEN_BNO_ISOLATION_INFORMATION *>(storage.data());
  std::wstring prefix;
  if (info->IsolationEnabled) {
    auto pointer = info->IsolationPrefix;
    for (size_t i = 0; i <= 32759; ++i) {
      if (!inside(storage, pointer + i, sizeof(wchar_t)))
        return 24;
      if (pointer[i] == 0)
        break;
      if (i == 32759)
        return 25;
      prefix.push_back(pointer[i]);
    }
  }
  ULONG session = 0;
  if (token_query(token.value, 12, &session, sizeof(session), &bytes) != 0 ||
      bytes != sizeof(session))
    return 26;
  printf(
      "namespace profile=%s pointer-bytes=%zu session=%lu enabled=%u prefix=",
      argv[1], sizeof(void *), session, unsigned(info->IsolationEnabled));
  text(prefix);
  puts("");

  const auto selected_base =
      session == 0
          ? std::wstring(L"\\BaseNamedObjects")
          : L"\\Sessions\\" + std::to_wstring(session) + L"\\BaseNamedObjects";
  const auto selected_parent =
      info->IsolationEnabled ? selected_base + L"\\" + prefix : selected_base;
  UNICODE_STRING before_name{};
  before_name.Length = USHORT(selected_parent.size() * sizeof(wchar_t));
  before_name.MaximumLength = before_name.Length;
  before_name.Buffer = const_cast<PWSTR>(selected_parent.data());
  OBJECT_ATTRIBUTES before_attrs{};
  before_attrs.Length = sizeof before_attrs;
  before_attrs.ObjectName = &before_name;
  before_attrs.Attributes = 0x40;
  Owned before_directory;
  const auto before_status =
      directory_open(&before_directory.value, 3, &before_attrs);
  printf("namespace-before-event status=%08lX path=", ULONG(before_status));
  text(selected_parent);
  puts("");
  size_t preopened = 0;
  if (before_status >= 0) {
    if (!before_directory.close())
      return 34;
    preopened = 1;
  }

  const auto link_open = reinterpret_cast<DirectoryOpen>(
      GetProcAddress(ntdll, "NtOpenSymbolicLinkObject"));
  const auto link_query = reinterpret_cast<LinkQuery>(
      GetProcAddress(ntdll, "NtQuerySymbolicLinkObject"));
  if (!link_open || !link_query)
    return 35;
  size_t extra_closed = 0;
  for (const auto &suffix : {L"Local", L"Global", L"Session"}) {
    const auto path = selected_parent + L"\\" + suffix;
    UNICODE_STRING n{};
    n.Length = USHORT(path.size() * 2);
    n.MaximumLength = n.Length;
    n.Buffer = const_cast<PWSTR>(path.data());
    OBJECT_ATTRIBUTES a{};
    a.Length = sizeof a;
    a.ObjectName = &n;
    a.Attributes = 0x40;
    Owned h;
    const auto opened = link_open(&h.value, 1, &a);
    printf("private-link open=%08lX path=", ULONG(opened));
    text(path);
    if (opened >= 0) {
      std::vector<wchar_t> target(2048);
      UNICODE_STRING output{};
      output.MaximumLength = USHORT(target.size() * 2);
      output.Buffer = target.data();
      ULONG required = 0xA5A5A5A5;
      const auto queried = link_query(h.value, &output, &required);
      printf(" query=%08lX length=%u required=%lu target=", ULONG(queried),
             output.Length, required);
      if (queried == 0) {
        if (output.Length % 2 || output.Length > target.size() * 2 ||
            output.Buffer != target.data())
          return 36;
        text(std::wstring(target.data(), output.Length / 2));
      }
      if (!h.close())
        return 37;
      ++extra_closed;
    }
    puts("");
  }
  for (const auto &qualifier : {std::wstring(), std::wstring(L"Global\\")}) {
    const auto path = qualifier + L"AssistBnoRoute_" +
                      std::to_wstring(GetCurrentProcessId()) + L"_" +
                      std::to_wstring(GetTickCount64());
    Owned additional;
    additional.value = CreateEventW(nullptr, FALSE, FALSE, path.c_str());
    const auto error = additional.value ? 0ul : GetLastError();
    printf("routed-event input=");
    text(path);
    printf(" error=%lu", error);
    if (additional.value) {
      std::vector<ULONG_PTR> result(65536 / sizeof(ULONG_PTR), 0);
      ULONG used = 0;
      const auto queried =
          object_query(additional.value, 1, result.data(), 65536, &used);
      printf(" query=%08lX actual=", ULONG(queried));
      if (queried == 0) {
        const auto n = reinterpret_cast<const UNICODE_STRING *>(result.data());
        if (n->Length % 2 || !inside(result, n->Buffer, n->Length))
          return 38;
        text(std::wstring(n->Buffer, n->Length / 2));
      }
      if (!additional.close())
        return 39;
      ++extra_closed;
    }
    puts("");
  }
  wchar_t event_name[160];
  swprintf_s(event_name, L"Local\\AssistBnoNamespace_%lu_%llu",
             GetCurrentProcessId(),
             static_cast<unsigned long long>(GetTickCount64()));
  Owned event;
  event.value = CreateEventW(nullptr, FALSE, FALSE, event_name);
  if (!event.value || GetLastError() == ERROR_ALREADY_EXISTS)
    return 27;
  std::vector<ULONG_PTR> name_storage(65536 / sizeof(ULONG_PTR), 0);
  const auto name_status =
      object_query(event.value, 1, name_storage.data(), 65536, &bytes);
  if (name_status != 0 || bytes > 65536 || bytes < sizeof(UNICODE_STRING))
    return 28;
  const auto name =
      reinterpret_cast<const UNICODE_STRING *>(name_storage.data());
  if (name->Length % 2 || !inside(name_storage, name->Buffer, name->Length))
    return 29;
  const std::wstring actual(name->Buffer, name->Length / sizeof(wchar_t));
  printf("event name=");
  text(actual);
  puts("");
  const auto separator = actual.rfind(L'\\');
  if (separator == std::wstring::npos)
    return 30;
  const auto parent = actual.substr(0, separator);
  const auto local =
      L"\\Sessions\\" + std::to_wstring(session) + L"\\BaseNamedObjects";
  std::set<std::wstring> paths = {L"\\BaseNamedObjects", local, parent};
  if (!prefix.empty()) {
    paths.insert(L"\\BaseNamedObjects\\" + prefix);
    paths.insert(local + L"\\" + prefix);
  }
  size_t opened = 0;
  for (const auto &path : paths) {
    UNICODE_STRING unicode{};
    unicode.Length = USHORT(path.size() * sizeof(wchar_t));
    unicode.MaximumLength = unicode.Length;
    unicode.Buffer = const_cast<PWSTR>(path.data());
    OBJECT_ATTRIBUTES attrs{};
    attrs.Length = sizeof(attrs);
    attrs.ObjectName = &unicode;
    attrs.Attributes = 0x40;
    Owned directory;
    const auto status = directory_open(&directory.value, 3, &attrs);
    printf("directory status=%08lX actual-parent=%u path=", ULONG(status),
           unsigned(path == parent));
    text(path);
    puts("");
    if (status >= 0) {
      if (!directory.close())
        return 31;
      ++opened;
    } else if (path == parent) {
      return 32;
    }
  }
  if (!event.close() || !token.close())
    return 33;
  printf("namespace-complete opened-directories=%zu preopened=%zu "
         "closed-handles=%zu\n",
         opened, preopened, opened + preopened + extra_closed + 2);
  return 0;
}
