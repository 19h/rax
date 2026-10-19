#define WIN32_LEAN_AND_MEAN
#include <array>
#include <cstdio>
#include <cwchar>
#include <string>
#include <windows.h>
#include <winternl.h>

using OpenKey = NTSTATUS(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES);
using Close = NTSTATUS(NTAPI *)(HANDLE);

int main(int argc, char **argv) {
  const char *profile = argc > 1 ? argv[1] : "unknown";
  auto module = GetModuleHandleW(L"ntdll.dll");
  auto open = reinterpret_cast<OpenKey>(GetProcAddress(module, "NtOpenKey"));
  auto close = reinterpret_cast<Close>(GetProcAddress(module, "NtClose"));
  if (!open || !close)
    return 2;
  DWORD before = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 3;
  const std::wstring root = L"\\Registry\\Machine\\System\\CurrentControlSet\\C"
                            L"ontrol\\Srp\\GP\\DLL";
  const std::array<std::wstring, 8> paths = {
      root,
      root + L"\\",
      root + L"\\missing-rax-child",
      root + L"\\\\missing-rax-child\\nested",
      root + L"Suffix",
      L"\\registry\\machine\\system\\currentcontrolset\\control\\safeboot\\opti"
      L"on",
      root + L"\\.",
      root + L"\\..",
  };
  unsigned cases = 0;
  std::printf("PROFILE %s POINTER_BYTES %zu\n", profile, sizeof(void *));
  for (unsigned sample = 0; sample < 2; ++sample) {
    for (unsigned index = 0; index < paths.size(); ++index) {
      for (ACCESS_MASK desired :
           {ACCESS_MASK(1), ACCESS_MASK(3), ACCESS_MASK(0x20019),
            ACCESS_MASK(0x20119), ACCESS_MASK(0x20219)}) {
        const auto &path = paths[index];
        UNICODE_STRING name{};
        name.Length = static_cast<USHORT>(path.size() * sizeof(wchar_t));
        name.MaximumLength = name.Length;
        name.Buffer = const_cast<PWSTR>(path.data());
        OBJECT_ATTRIBUTES attrs{};
        attrs.Length = sizeof(attrs);
        attrs.ObjectName = &name;
        attrs.Attributes = 0x40;
        HANDLE handle =
            reinterpret_cast<HANDLE>(static_cast<uintptr_t>(0xABABABAB));
        auto status = open(&handle, desired, &attrs);
        std::printf(
            "OPEN sample=%u path=%u desired=%08lX status=%08lX out=%016llX\n",
            sample, index, static_cast<unsigned long>(desired),
            static_cast<unsigned long>(status),
            static_cast<unsigned long long>(
                reinterpret_cast<uintptr_t>(handle)));
        if (status >= 0 && handle) {
          if (close(handle) < 0)
            return 4;
        }
        ++cases;
      }
    }
  }
  DWORD after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 5;
  std::printf("SUMMARY cases=%u handles-before=%lu handles-after=%lu\n", cases,
              before, after);
  return before == after ? 0 : 6;
}
