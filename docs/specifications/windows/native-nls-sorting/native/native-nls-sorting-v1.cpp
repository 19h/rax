#define WIN32_LEAN_AND_MEAN
#include <array>
#include <cstdio>
#include <cwchar>
#include <cwctype>
#include <string>
#include <windows.h>
#include <winternl.h>

#include <algorithm>
#include <vector>

using OpenKey = NTSTATUS(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES);
using Close = NTSTATUS(NTAPI *)(HANDLE);

struct Metadata {
  DWORD children = 0, values = 0, max_name = 0, max_data = 0;
  FILETIME time{};
  bool operator==(const Metadata &o) const {
    return children == o.children && values == o.values &&
           max_name == o.max_name && max_data == o.max_data &&
           time.dwLowDateTime == o.time.dwLowDateTime &&
           time.dwHighDateTime == o.time.dwHighDateTime;
  }
};
struct RawValue {
  std::wstring name;
  DWORD kind;
  std::vector<BYTE> data;
  bool operator==(const RawValue &o) const {
    return name == o.name && kind == o.kind && data == o.data;
  }
};
static bool metadata(HKEY key, Metadata &m) {
  return RegQueryInfoKeyW(key, nullptr, nullptr, nullptr, &m.children, nullptr,
                          nullptr, &m.values, &m.max_name, &m.max_data, nullptr,
                          &m.time) == ERROR_SUCCESS &&
         m.values <= 4096 && m.max_name <= 16383 && m.max_data <= (1u << 20);
}
static bool enumerate(HKEY key, const Metadata &m, std::vector<RawValue> &out) {
  std::vector<wchar_t> name(m.max_name + 1);
  std::vector<BYTE> data(m.max_data);
  size_t total = 0;
  for (DWORD i = 0; i < m.values; ++i) {
    DWORD n = DWORD(name.size()), b = DWORD(data.size()), kind = 0;
    if (RegEnumValueW(key, i, name.data(), &n, nullptr, &kind, data.data(),
                      &b) != ERROR_SUCCESS ||
        n >= name.size() || b > data.size())
      return false;
    total += size_t(n) * 2 + b;
    if (total > (16u << 20))
      return false;
    out.push_back({std::wstring(name.data(), n), kind,
                   std::vector<BYTE>(data.begin(), data.begin() + b)});
  }
  std::sort(out.begin(), out.end(),
            [](const auto &a, const auto &b) { return a.name < b.name; });
  return true;
}
static int snapshot() {
  for (DWORD view : {DWORD(0), DWORD(0x100), DWORD(0x200)}) {
    HKEY key = nullptr;
    LONG status = RegOpenKeyExW(
        HKEY_LOCAL_MACHINE,
        L"System\\CurrentControlSet\\Control\\Nls\\Sorting\\Versions", 0,
        KEY_QUERY_VALUE | KEY_ENUMERATE_SUB_KEYS | view, &key);
    std::printf("SNAPSHOT view=%08lX open=%ld\n", view, status);
    if (status == ERROR_FILE_NOT_FOUND)
      continue;
    if (status != ERROR_SUCCESS)
      return 1;
    Metadata before{}, after{};
    std::vector<RawValue> first, second;
    bool ok = metadata(key, before) && enumerate(key, before, first) &&
              enumerate(key, before, second) && metadata(key, after) &&
              before == after && first == second;
    LONG closed = RegCloseKey(key);
    if (!ok || closed != ERROR_SUCCESS)
      return 2;
    std::printf("METADATA view=%08lX children=%lu values=%lu time=%08lX%08lX\n",
                view, before.children, before.values,
                before.time.dwHighDateTime, before.time.dwLowDateTime);
    for (const auto &value : first) {
      std::printf("VALUE view=%08lX kind=%lu name=", view, value.kind);
      for (wchar_t c : value.name)
        std::printf("%04X", unsigned(c));
      std::printf(" data=");
      for (BYTE b : value.data)
        std::printf("%02X", unsigned(b));
      std::printf("\n");
    }
  }
  return 0;
}

int main(int argc, char **argv) {
  const char *profile = argc > 1 ? argv[1] : "unknown";
  auto module = GetModuleHandleW(L"ntdll.dll");
  auto open = reinterpret_cast<OpenKey>(GetProcAddress(module, "NtOpenKey"));
  auto close = reinterpret_cast<Close>(GetProcAddress(module, "NtClose"));
  if (!open || !close)
    return 2;
  const std::wstring root = L"\\Registry\\Machine\\System\\CurrentControlSet\\C"
                            L"ontrol\\Nls\\Sorting\\Versions";
  auto lower = root;
  for (auto &unit : lower)
    unit = std::towlower(unit);
  if (snapshot() != 0)
    return 20;
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
