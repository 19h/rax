#define WIN32_LEAN_AND_MEAN
#include <array>
#include <cstdio>
#include <cwctype>
#include <string>
#include <windows.h>
#include <winternl.h>
using Open = LONG(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES);
struct Result {
  ULONG status, exception;
};
static Result invoke(Open fn, PHANDLE out, ACCESS_MASK access,
                     POBJECT_ATTRIBUTES attributes) {
  __try {
    return {ULONG(fn(out, access, attributes)), 0};
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    return {0xDEADBEEF, GetExceptionCode()};
  }
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto open = reinterpret_cast<Open>(
      GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "NtOpenKey"));
  if (!open)
    return 2;
  DWORD before = 0, after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 3;
  std::printf("PROFILE %s POINTER_BYTES %zu\n", argc > 1 ? argv[1] : "unknown",
              sizeof(void *));
  constexpr ACCESS_MASK grants[] = {
      0, 1, 2, 0x20000, 0x20019, 0xF003F, 0x2000000, 0x20119, 0x20219};
  unsigned calls = 0, opened = 0, closed = 0;
  for (unsigned spelling = 0; spelling < 4; ++spelling) {
    std::wstring path =
        L"\\Registry\\Machine\\Software\\Policies\\Microsoft\\Windows\\Display";
    if (spelling != 0)
      path.push_back(L'\\');
    if (spelling == 2)
      for (auto &c : path)
        c = wchar_t(towupper(c));
    if (spelling == 3)
      for (auto &c : path)
        c = wchar_t(towlower(c));
    UNICODE_STRING name{};
    name.Buffer = path.data();
    name.Length = USHORT(path.size() * sizeof(wchar_t));
    name.MaximumLength = name.Length + sizeof(wchar_t);
    for (ULONG flags : {0u, 0x40u, 0x200u, 0x240u})
      for (ACCESS_MASK grant : grants) {
        OBJECT_ATTRIBUTES attributes{};
        attributes.Length = sizeof(attributes);
        attributes.ObjectName = &name;
        attributes.Attributes = flags;
        HANDLE handle =
            reinterpret_cast<HANDLE>(static_cast<uintptr_t>(0xA5A5A5A5));
        SetLastError(0x12345678);
        auto result = invoke(open, &handle, grant, &attributes);
        DWORD error = GetLastError();
        std::printf("OPEN spelling=%u flags=%08lX grant=%08lX status=%08lX "
                    "exception=%08lX handle=%016llX last-error=%08lX\n",
                    spelling, flags, grant, result.status, result.exception,
                    static_cast<unsigned long long>(
                        reinterpret_cast<uintptr_t>(handle)),
                    error);
        ++calls;
        if (result.exception == 0 && LONG(result.status) >= 0) {
          ++opened;
          if (!CloseHandle(handle))
            return 4;
          ++closed;
        }
      }
  }
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 5;
  std::printf("SUMMARY calls=%u opened=%u closed=%u handles-before=%lu "
              "handles-after=%lu\n",
              calls, opened, closed, before, after);
  return opened == closed && calls == 144 ? 0 : 6;
}
