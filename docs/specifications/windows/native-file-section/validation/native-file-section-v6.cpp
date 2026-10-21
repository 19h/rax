#define NOMINMAX
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#include <windows.h>
#include <winternl.h>

using Create = LONG(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES,
                             PLARGE_INTEGER, ULONG, ULONG, HANDLE);
using QuerySection = LONG(NTAPI *)(HANDLE, ULONG, PVOID, SIZE_T, PSIZE_T);
using QueryObject = LONG(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);
using Map = LONG(NTAPI *)(HANDLE, HANDLE, PVOID *, ULONG_PTR, SIZE_T,
                          PLARGE_INTEGER, PSIZE_T, ULONG, ULONG, ULONG);
using Unmap = LONG(NTAPI *)(HANDLE, PVOID);
using QueryMemory = LONG(NTAPI *)(HANDLE, PVOID, ULONG, PVOID, SIZE_T, PSIZE_T);
using Close = LONG(NTAPI *)(HANDLE);

struct Case {
  ULONG file_access = 0x100021;
  ULONG section_access = 0xD;
  ULONG protection = PAGE_EXECUTE;
  ULONG attributes = SEC_IMAGE;
  int kind = 0;
  int maximum_kind = 0;
  LONGLONG maximum = 0;
  bool null_output = false;
};

static LONG invoke(Create create, PHANDLE out, const Case &test,
                   PLARGE_INTEGER maximum, HANDLE file) {
  __try {
    return create(out, test.section_access, nullptr, maximum, test.protection,
                  test.attributes, file);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    return static_cast<LONG>(GetExceptionCode());
  }
}

int main(int argc, char **argv) {
  if (argc != 2)
    return 10;
  HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
  auto create =
      reinterpret_cast<Create>(GetProcAddress(ntdll, "NtCreateSection"));
  auto query =
      reinterpret_cast<QuerySection>(GetProcAddress(ntdll, "NtQuerySection"));
  auto close = reinterpret_cast<Close>(GetProcAddress(ntdll, "NtClose"));
  if (!create || !query || !close)
    return 11;
  wchar_t root[MAX_PATH]{};
  if (!GetTempPathW(MAX_PATH, root))
    return 12;
  std::wstring directory = std::wstring(root) + L"native-file-section-v6-" +
                           std::to_wstring(GetCurrentProcessId());
  if (!CreateDirectoryW(directory.c_str(), nullptr))
    return 13;
  wchar_t system[MAX_PATH]{};
  if (!GetSystemDirectoryW(system, MAX_PATH))
    return 14;
  std::array<std::wstring, 3> paths{directory + L"\\image.dll",
                                    directory + L"\\plain.bin",
                                    directory + L"\\empty.bin"};
  std::wstring original = std::wstring(system) + L"\\version.dll";
  if (!CopyFileW(original.c_str(), paths[0].c_str(), TRUE))
    return 15;
  for (int kind : {1, 2}) {
    HANDLE h = CreateFileW(paths[kind].c_str(), GENERIC_WRITE, 7, nullptr,
                           CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (h == INVALID_HANDLE_VALUE)
      return 16;
    DWORD written = 0;
    bool ok = kind == 2 ||
              (WriteFile(h, "plain", 5, &written, nullptr) && written == 5);
    CloseHandle(h);
    if (!ok)
      return 17;
  }
  std::vector<Case> cases(1);
  for (int kind : {0, 1, 2})
    for (ULONG access : std::array<ULONG, 10>{
             0UL, 1UL, 0x20UL, 0x100000UL, 0x100021UL, GENERIC_READ,
             GENERIC_WRITE, 0x20000UL, 0x80UL, GENERIC_READ | GENERIC_WRITE}) {
      Case c;
      c.kind = kind;
      c.file_access = access;
      cases.push_back(c);
    }
  for (ULONG protection : {0UL, 1UL, 2UL, 4UL, 8UL, 0x10UL, 0x20UL, 0x40UL,
                           0x80UL, 0x100UL, 0x200UL, 0x400UL}) {
    Case c;
    c.protection = protection;
    cases.push_back(c);
  }
  for (ULONG attributes : std::array<ULONG, 8>{
           0UL, SEC_IMAGE, SEC_IMAGE | SEC_NOCACHE, SEC_IMAGE | 1UL,
           SEC_IMAGE | 2UL, SEC_IMAGE | SEC_LARGE_PAGES, SEC_COMMIT,
           SEC_RESERVE}) {
    Case c;
    c.attributes = attributes;
    cases.push_back(c);
  }
  for (ULONG access : std::array<ULONG, 14>{
           0UL, 1UL, 2UL, 4UL, 8UL, 0xDUL, 0x1FUL, 0xF001FUL, GENERIC_READ,
           GENERIC_WRITE, GENERIC_EXECUTE, GENERIC_ALL, MAXIMUM_ALLOWED,
           ACCESS_SYSTEM_SECURITY}) {
    Case c;
    c.section_access = access;
    cases.push_back(c);
  }
  for (ULONG access :
       std::array<ULONG, 11>{0, 1, 2, 4, 0x20, 0x21, 0x22, 0x23, GENERIC_READ,
                             GENERIC_READ | GENERIC_EXECUTE, GENERIC_ALL}) {
    for (ULONG protection :
         std::array<ULONG, 7>{2, 4, 8, 0x10, 0x20, 0x40, 0x80}) {
      Case c;
      c.file_access = access;
      c.protection = protection;
      cases.push_back(c);
    }
  }
  for (LONGLONG size : {0LL, 1LL, 4096LL, 65536LL, -1LL}) {
    Case c;
    c.maximum_kind = 1;
    c.maximum = size;
    cases.push_back(c);
  }
  {
    Case c;
    c.maximum_kind = 2;
    cases.push_back(c);
    c.maximum_kind = 0;
    c.null_output = true;
    cases.push_back(c);
  }
  DWORD initial = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &initial))
    return 18;
  unsigned role = 0;
  for (const auto &test : cases) {
    HANDLE file = CreateFileW(paths[test.kind].c_str(), test.file_access, 7,
                              nullptr, OPEN_EXISTING, 0, nullptr);
    if (file == INVALID_HANDLE_VALUE) {
      std::printf("case role=%u open-error=%lu\n", role++, GetLastError());
      continue;
    }
    alignas(8) std::array<unsigned char, 24> size_bytes{};
    auto maximum = reinterpret_cast<PLARGE_INTEGER>(size_bytes.data());
    maximum->QuadPart = test.maximum;
    if (test.maximum_kind == 0)
      maximum = nullptr;
    else if (test.maximum_kind == 2)
      maximum = reinterpret_cast<PLARGE_INTEGER>(size_bytes.data() + 1);
    HANDLE section = reinterpret_cast<HANDLE>(static_cast<uintptr_t>(-3));
    LONG status = invoke(create, test.null_output ? nullptr : &section, test,
                         maximum, file);
    LONG basic_status = static_cast<LONG>(0xDEADBEEF);
    LONG object_status = static_cast<LONG>(0xDEADBEEF);
    SIZE_T returned = 0;
    ULONG object_returned = 0;
    alignas(8) std::array<unsigned char, 64> basic{}, object{};
    bool valid = section && section != reinterpret_cast<HANDLE>(
                                           static_cast<uintptr_t>(-3));
    if (valid) {
      basic_status = query(section, 0, basic.data(),
                           static_cast<ULONG>(basic.size()), &returned);
      auto query_object =
          reinterpret_cast<QueryObject>(GetProcAddress(ntdll, "NtQueryObject"));
      object_status =
          query_object(section, 0, object.data(), 56, &object_returned);
    }
    LONG map_status = static_cast<LONG>(0xDEADBEEF);
    SIZE_T view_size = 0;
    LONG image_status = static_cast<LONG>(0xDEADBEEF);
    SIZE_T image_size = 0;
    ULONG image_flags = 0;
    PVOID view = nullptr;
    ULONG state = 0, protection = 0;
    if (valid) {
      auto map =
          reinterpret_cast<Map>(GetProcAddress(ntdll, "NtMapViewOfSection"));
      auto unmap = reinterpret_cast<Unmap>(
          GetProcAddress(ntdll, "NtUnmapViewOfSection"));
      HANDLE current = reinterpret_cast<HANDLE>(static_cast<intptr_t>(-1));
      map_status = map(section, current, &view, 0, 0, nullptr, &view_size, 2, 0,
                       PAGE_READONLY);
      if (map_status >= 0 && view) {
        MEMORY_BASIC_INFORMATION memory{};
        if (VirtualQuery(view, &memory, sizeof(memory)) == sizeof(memory)) {
          state = memory.State;
          protection = memory.Protect;
        }
        auto query_memory = reinterpret_cast<QueryMemory>(
            GetProcAddress(ntdll, "NtQueryVirtualMemory"));
        alignas(8) std::array<unsigned char, 32> information{};
        SIZE_T information_bytes = 0;
        image_status =
            query_memory(current, view, 6, information.data(),
                         sizeof(void *) == 8 ? 24 : 12, &information_bytes);
        if (image_status >= 0) {
          std::memcpy(&image_size, information.data() + sizeof(void *),
                      sizeof(image_size));
          std::memcpy(&image_flags, information.data() + 2 * sizeof(void *),
                      sizeof(image_flags));
        }
        if (unmap(current, view) != 0)
          return 23;
      }
    }
    std::printf(
        "view role=%u status=%08lX bytes=%llu state=%08lX protection=%08lX\n",
        role, static_cast<ULONG>(map_status),
        static_cast<unsigned long long>(view_size), state, protection);
    std::printf(
        "image-information role=%u status=%08lX bytes=%llu flags=%08lX\n", role,
        static_cast<ULONG>(image_status),
        static_cast<unsigned long long>(image_size), image_flags);
    ULONG grant = 0;
    std::memcpy(&grant, object.data() + 4, sizeof(grant));
    std::printf("case role=%u kind=%d file-access=%08lX section-access=%08lX "
                "protection=%08lX attributes=%08lX maximum-kind=%d "
                "maximum=%lld null-output=%u status=%08lX valid=%u "
                "basic-status=%08lX object-status=%08lX grant=%08lX basic=",
                role++, test.kind, test.file_access, test.section_access,
                test.protection, test.attributes, test.maximum_kind,
                test.maximum, static_cast<unsigned>(test.null_output),
                static_cast<ULONG>(status), static_cast<unsigned>(valid),
                static_cast<ULONG>(basic_status),
                static_cast<ULONG>(object_status), grant);
    for (unsigned char byte : basic)
      std::printf("%02X", byte);
    std::putchar('\n');
    if (valid)
      close(section);
    CloseHandle(file);
  }
  DWORD final_count = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &final_count))
    return 19;
  for (const auto &path : paths)
    if (!DeleteFileW(path.c_str()))
      return 20;
  if (!RemoveDirectoryW(directory.c_str()))
    return 21;
  std::printf(
      "summary profile=%s cases=%u handles-before=%lu handles-after=%lu\n",
      argv[1], role, initial, final_count);
  return initial == final_count ? 0 : 22;
}
