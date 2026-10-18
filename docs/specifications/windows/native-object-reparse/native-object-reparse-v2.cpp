#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <string>
#include <vector>
#include <windows.h>
#include <winternl.h>
using Open = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);
using Close = LONG(NTAPI *)(HANDLE);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 20;
  const auto module = GetModuleHandleW(L"ntdll.dll");
  const auto open =
      reinterpret_cast<Open>(GetProcAddress(module, "NtOpenDirectoryObject"));
  const auto close = reinterpret_cast<Close>(GetProcAddress(module, "NtClose"));
  const auto query =
      reinterpret_cast<Query>(GetProcAddress(module, "NtQueryObject"));
  if (!open || !close || !query)
    return 21;
  DWORD before = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 22;
  size_t opened = 0, closed = 0, ordinal = 0;
  printf("profile=%s pointer-bytes=%zu\n", argv[1], sizeof(void *));
  std::vector<unsigned> counts;
  for (unsigned i = 0; i <= 72; ++i)
    counts.push_back(i);
  for (unsigned i : {128u, 256u, 1024u})
    counts.push_back(i);
  for (unsigned count : counts) {
    for (ULONG flags : {0ul, 0x40ul, 0x100ul, 0x140ul, 0x1000ul, 0x1040ul,
                        0x1100ul, 0x1140ul}) {
      std::wstring path = L"\\BaseNamedObjects";
      for (unsigned i = 0; i < count; ++i)
        path += i % 2 ? L"\\Global" : L"\\Local";
      UNICODE_STRING text{};
      text.Length = USHORT(path.size() * 2);
      text.MaximumLength = text.Length;
      text.Buffer = path.data();
      OBJECT_ATTRIBUTES attrs{};
      attrs.Length = sizeof attrs;
      attrs.ObjectName = &text;
      attrs.Attributes = flags;
      HANDLE handle = nullptr;
      const LONG status = open(&handle, 3, &attrs);
      printf("reparse case=%zu links=%u flags=%08lX status=%08lX", ordinal++,
             count, flags, ULONG(status));
      if (status >= 0) {
        ++opened;
        alignas(16) BYTE info[512];
        memset(info, 0xA5, sizeof info);
        ULONG returned = 0;
        const auto named = query(handle, 1, info, sizeof info, &returned);
        if (named != 0)
          return 23;
        const auto name = reinterpret_cast<const UNICODE_STRING *>(info);
        if (name->Length != 34 ||
            std::wstring(name->Buffer, name->Length / 2) !=
                L"\\BaseNamedObjects")
          return 24;
        printf(" canonical-base=1");
        if (close(handle) != 0)
          return 25;
        ++closed;
      }
      puts("");
    }
  }
  DWORD after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 26;
  printf("complete cases=%zu opened=%zu closed=%zu before=%lu after=%lu\n",
         ordinal, opened, closed, before, after);
  return before == after && opened == closed ? 0 : 27;
}
