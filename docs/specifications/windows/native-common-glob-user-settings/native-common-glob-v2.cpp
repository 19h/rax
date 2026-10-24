#define WIN32_LEAN_AND_MEAN
#include <array>
#include <cstdio>
#include <cwctype>
#include <string>
#include <vector>
#include <windows.h>
#include <winternl.h>

static constexpr wchar_t ROOT[] =
    L"System\\CurrentControlSet\\Control\\CommonGlobUserSettings";
static void hex(const void *data, size_t size) {
  auto p = static_cast<const unsigned char *>(data);
  for (size_t i = 0; i < size; ++i)
    std::printf("%02X", unsigned(p[i]));
}
static void text(const std::wstring &value) {
  hex(value.data(), value.size() * sizeof(wchar_t));
}
struct Budget {
  size_t keys = 0, values = 0, bytes = 0, paths = 0;
};
struct Info {
  DWORD children = 0, max_child = 0, values = 0, max_name = 0, max_data = 0;
  FILETIME time{};
  bool operator==(const Info &other) const {
    return children == other.children && max_child == other.max_child &&
           values == other.values && max_name == other.max_name &&
           max_data == other.max_data &&
           time.dwLowDateTime == other.time.dwLowDateTime &&
           time.dwHighDateTime == other.time.dwHighDateTime;
  }
};
static bool info(HKEY key, Info &out) {
  return RegQueryInfoKeyW(key, nullptr, nullptr, nullptr, &out.children,
                          &out.max_child, nullptr, &out.values, &out.max_name,
                          &out.max_data, nullptr, &out.time) == ERROR_SUCCESS;
}
struct OwnedKey {
  HKEY value = nullptr;
  ~OwnedKey() {
    if (value)
      RegCloseKey(value);
  }
};
static bool tree(const std::wstring &path, DWORD view, unsigned sample,
                 unsigned depth, Budget &budget) {
  if (depth > 32 || ++budget.keys > 1024 || path.size() > 32767)
    return false;
  const std::wstring native = L"\\Registry\\Machine\\" + path;
  budget.paths += native.size() * sizeof(wchar_t);
  if (budget.paths > (1u << 20))
    return false;
  std::vector<std::wstring> children;
  {
    OwnedKey key;
    const LSTATUS status = RegOpenKeyExW(
        HKEY_LOCAL_MACHINE, path.c_str(), 0,
        KEY_QUERY_VALUE | KEY_ENUMERATE_SUB_KEYS | view, &key.value);
    std::printf("TREE sample=%u view=%03lX depth=%u status=%08lX path=", sample,
                view, depth, DWORD(status));
    text(native);
    if (status != ERROR_SUCCESS) {
      std::printf("\n");
      return depth == 0 && status == ERROR_FILE_NOT_FOUND;
    }
    Info before{};
    if (!info(key.value, before) || before.children > 1024 ||
        before.max_child > 255 || before.values > 4096 ||
        before.max_name > 16383 || before.max_data > (1u << 20))
      return false;
    const auto time =
        (static_cast<unsigned long long>(before.time.dwHighDateTime) << 32) |
        before.time.dwLowDateTime;
    std::printf(" children=%lu values=%lu time=%016llX\n", before.children,
                before.values, time);
    std::vector<wchar_t> name(before.max_name + 1);
    std::vector<BYTE> data(before.max_data + 1);
    for (DWORD i = 0; i < before.values; ++i) {
      DWORD units = DWORD(name.size()), bytes = before.max_data, kind = 0;
      if (RegEnumValueW(key.value, i, name.data(), &units, nullptr, &kind,
                        data.data(), &bytes) != ERROR_SUCCESS ||
          units >= name.size() || bytes > before.max_data)
        return false;
      if (++budget.values > 4096)
        return false;
      budget.bytes += size_t(units) * 2 + bytes;
      if (budget.bytes > (16u << 20))
        return false;
      std::printf("VALUE sample=%u view=%03lX path=", sample, view);
      text(native);
      std::printf(" name=");
      hex(name.data(), size_t(units) * 2);
      std::printf(" kind=%lu bytes=%lu data=", kind, bytes);
      hex(data.data(), bytes);
      std::printf("\n");
    }
    std::vector<wchar_t> child(before.max_child + 1);
    for (DWORD i = 0; i < before.children; ++i) {
      DWORD units = DWORD(child.size());
      if (RegEnumKeyExW(key.value, i, child.data(), &units, nullptr, nullptr,
                        nullptr, nullptr) != ERROR_SUCCESS ||
          units == 0 || units >= child.size())
        return false;
      std::wstring component(child.data(), units);
      if (component.find_first_of(std::wstring(L"\\\0", 2)) !=
          std::wstring::npos)
        return false;
      children.push_back(component);
      std::printf("CHILD sample=%u view=%03lX parent=", sample, view);
      text(native);
      std::printf(" name=");
      text(component);
      std::printf("\n");
    }
    Info after{};
    if (!info(key.value, after) || !(before == after))
      return false;
  }
  for (const auto &name : children)
    if (!tree(path + L"\\" + name, view, sample, depth + 1, budget))
      return false;
  return true;
}
static bool capture(unsigned sample) {
  for (DWORD view : {0u, 0x100u, 0x200u}) {
    Budget budget;
    if (!tree(ROOT, view, sample, 0, budget))
      return false;
    std::printf("TREE_END sample=%u view=%03lX keys=%zu values=%zu bytes=%zu "
                "paths=%zu\n",
                sample, view, budget.keys, budget.values, budget.bytes,
                budget.paths);
  }
  return true;
}
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
  std::printf("PROFILE %s POINTER_BYTES %zu\n", argc > 1 ? argv[1] : "unknown",
              sizeof(void *));
  if (!capture(0))
    return 7;
  DWORD before = 0, after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 3;
  constexpr ACCESS_MASK grants[] = {
      0, 1, 2, 0x20000, 0x20019, 0xF003F, 0x2000000, 0x20119, 0x20219};
  unsigned calls = 0, opened = 0, closed = 0;
  for (unsigned spelling = 0; spelling < 4; ++spelling) {
    std::wstring path = std::wstring(L"\\Registry\\Machine\\") + ROOT;
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
  if (!capture(1))
    return 8;
  return opened == closed && calls == 144 && before == after ? 0 : 6;
}
