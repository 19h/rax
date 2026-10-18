#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdio>
#include <string>
#include <vector>
#include <windows.h>
#include <winternl.h>

using Open = LONG(NTAPI *)(HANDLE *, ACCESS_MASK, OBJECT_ATTRIBUTES *);
using Query = LONG(NTAPI *)(HANDLE, UNICODE_STRING *, ULONG *);
using Close = LONG(NTAPI *)(HANDLE);

static void text(const std::wstring &value) {
  for (const auto unit : value)
    printf("%04X", unsigned(unit));
}

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 61;
  const auto module = GetModuleHandleW(L"ntdll.dll");
  const auto open = reinterpret_cast<Open>(
      GetProcAddress(module, "NtOpenSymbolicLinkObject"));
  const auto query = reinterpret_cast<Query>(
      GetProcAddress(module, "NtQuerySymbolicLinkObject"));
  const auto close = reinterpret_cast<Close>(GetProcAddress(module, "NtClose"));
  if (!open || !query || !close)
    return 62;
  DWORD session = 0, before = 0, after = 0;
  if (!ProcessIdToSessionId(GetCurrentProcessId(), &session) ||
      !GetProcessHandleCount(GetCurrentProcess(), &before))
    return 63;
  printf("links profile=%s pointer-bytes=%zu process-session=%lu\n", argv[1],
         sizeof(void *), session);
  unsigned opened = 0, closed = 0;
  for (const auto &root : {std::wstring(L"\\BaseNamedObjects"),
                           std::wstring(L"\\Sessions\\1\\BaseNamedObjects")}) {
    for (const auto leaf : {L"Local", L"Global", L"Session"}) {
      auto path = root + L"\\" + leaf;
      UNICODE_STRING name{};
      name.Length = USHORT(path.size() * sizeof(wchar_t));
      name.MaximumLength = name.Length;
      name.Buffer = path.data();
      OBJECT_ATTRIBUTES attrs{};
      attrs.Length = sizeof attrs;
      attrs.Attributes = 0x40;
      attrs.ObjectName = &name;
      HANDLE handle = nullptr;
      const auto status = open(&handle, 1, &attrs);
      printf("link open=%08lX path=", ULONG(status));
      text(path);
      if (status >= 0) {
        ++opened;
        std::vector<wchar_t> buffer(1024, wchar_t(0xA5A5));
        UNICODE_STRING target{};
        target.MaximumLength = USHORT(buffer.size() * sizeof(wchar_t));
        target.Buffer = buffer.data();
        ULONG required = 0xA5A5A5A5;
        const auto result = query(handle, &target, &required);
        printf(" query=%08lX length=%u required=%lu target=", ULONG(result),
               unsigned(target.Length), required);
        if (result >= 0) {
          if (target.Length % 2 || target.Length > target.MaximumLength ||
              target.Buffer != buffer.data())
            return 64;
          text(std::wstring(buffer.data(), target.Length / sizeof(wchar_t)));
        }
        if (close(handle) != 0)
          return 65;
        ++closed;
      }
      puts("");
    }
  }
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 66;
  printf("complete opens=%u closed=%u handles-before=%lu handles-after=%lu\n",
         opened, closed, before, after);
  return opened == closed && before == after ? 0 : 67;
}
