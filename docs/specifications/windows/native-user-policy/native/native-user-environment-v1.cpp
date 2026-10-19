#define WIN32_LEAN_AND_MEAN
#include <array>
#include <cstdio>
#include <cwchar>
#include <cwctype>
#include <string>
#include <windows.h>
#include <winternl.h>

#include <sddl.h>

using OpenKey = NTSTATUS(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES);
using Close = NTSTATUS(NTAPI *)(HANDLE);

int main(int argc, char **argv) {
  const char *profile = argc > 1 ? argv[1] : "unknown";
  auto module = GetModuleHandleW(L"ntdll.dll");
  auto open = reinterpret_cast<OpenKey>(GetProcAddress(module, "NtOpenKey"));
  auto close = reinterpret_cast<Close>(GetProcAddress(module, "NtClose"));
  if (!open || !close)
    return 2;
  alignas(void *) std::array<BYTE, 4096> token_data{};
  HANDLE token = nullptr;
  DWORD token_bytes = 0;
  if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token))
    return 10;
  BOOL queried = GetTokenInformation(token, TokenUser, token_data.data(),
                                     DWORD(token_data.size()), &token_bytes);
  if (!CloseHandle(token) || !queried || token_bytes < sizeof(TOKEN_USER) + 8 ||
      token_bytes > token_data.size())
    return 11;
  auto user = reinterpret_cast<TOKEN_USER *>(token_data.data());
  uintptr_t sid_at = reinterpret_cast<uintptr_t>(user->User.Sid),
            data_at = reinterpret_cast<uintptr_t>(token_data.data());
  if (sid_at < data_at || sid_at - data_at > token_bytes - 8)
    return 12;
  DWORD sid_bytes = GetLengthSid(user->User.Sid);
  if (sid_bytes > token_bytes - (sid_at - data_at) ||
      !IsValidSid(user->User.Sid))
    return 13;
  LPWSTR text = nullptr;
  if (!ConvertSidToStringSidW(user->User.Sid, &text) || !text)
    return 14;
  std::wstring sid(text);
  if (LocalFree(text) != nullptr || sid.size() > 256)
    return 15;
  const std::wstring root = L"\\Registry\\User\\" + sid + L"\\Environment";
  auto lower = root;
  for (auto &unit : lower)
    unit = std::towlower(unit);
  DWORD before = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 3;
  const std::array<std::wstring, 8> paths = {
      root,
      root + L"\\",
      root + L"\\missing-rax-child",
      root + L"\\\\missing-rax-child\\nested",
      root + L"Suffix",
      lower,
      root + L"\\.",
      root + L"\\..",
  };
  unsigned cases = 0;
  std::printf("PROFILE %s POINTER_BYTES %zu\n", profile, sizeof(void *));
  std::printf("USER sid=%ls attributes=%08lX bytes=%lu\n", sid.c_str(),
              user->User.Attributes, sid_bytes);
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
