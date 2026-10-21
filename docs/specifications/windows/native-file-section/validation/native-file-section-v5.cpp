#define NOMINMAX
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>
#include <windows.h>
#include <winternl.h>

using Create = LONG(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES,
                             PLARGE_INTEGER, ULONG, ULONG, HANDLE);
using QuerySection = LONG(NTAPI *)(HANDLE, ULONG, PVOID, SIZE_T, PSIZE_T);
using QueryObject = LONG(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);
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
  int left = -1, right = -1;
};

static LONG invoke(Create create, PHANDLE out, const Case &test,
                   PLARGE_INTEGER maximum, HANDLE file,
                   POBJECT_ATTRIBUTES object) {
  __try {
    return create(out, test.section_access, object, maximum, test.protection,
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
  std::wstring directory = std::wstring(root) + L"native-file-section-v5-" +
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
  const int groups[21] = {0, 1, 1, 2, 2, 2, 3, 3, 3, 3, 4,
                          4, 4, 4, 4, 5, 5, 5, 6, 5, 4};
  for (int left = 0; left < 21; ++left) {
    Case c;
    c.left = left;
    if (left == 17)
      c.file_access = FILE_READ_DATA;
    cases.push_back(c);
    for (int right = left + 1; right < 21; ++right) {
      if (groups[left] == groups[right])
        continue;
      Case pair;
      pair.left = left;
      pair.right = right;
      if (left == 17 || right == 17)
        pair.file_access = FILE_READ_DATA;
      cases.push_back(pair);
    }
  }
  DWORD initial = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &initial))
    return 18;
  unsigned role = 0;
  for (const auto &original_test : cases) {
    Case test = original_test;
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
    auto memory = static_cast<unsigned char *>(VirtualAlloc(
        nullptr, 8 * 4096, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
    if (!memory)
      return 24;
    HANDLE sentinel = reinterpret_cast<HANDLE>(static_cast<uintptr_t>(-3));
    PHANDLE output = reinterpret_cast<PHANDLE>(memory);
    *output = sentinel;
    auto captured_maximum = reinterpret_cast<PLARGE_INTEGER>(memory + 4096);
    captured_maximum->QuadPart = test.maximum;
    if (maximum)
      maximum = reinterpret_cast<PLARGE_INTEGER>(
          memory + 4096 + (test.maximum_kind == 2 ? 1 : 0));
    auto object_storage =
        reinterpret_cast<POBJECT_ATTRIBUTES>(memory + 2 * 4096);
    *object_storage = {};
    object_storage->Length = sizeof(OBJECT_ATTRIBUTES);
    POBJECT_ATTRIBUTES object = nullptr;
    HANDLE argument_file = file;
    HANDLE event = nullptr;
    auto protect = [&](void *at, DWORD flags) {
      DWORD old = 0;
      if (!VirtualProtect(at, 4096, flags, &old))
        std::abort();
    };
    auto fault = [&](int id) {
      switch (id) {
      case -1:
        break;
      case 0:
        test.attributes = 0;
        break;
      case 1:
        test.protection = 0;
        break;
      case 2:
        test.protection = 1;
        break;
      case 3:
        output = nullptr;
        test.null_output = true;
        break;
      case 4:
        protect(memory, PAGE_NOACCESS);
        break;
      case 5:
        protect(memory, PAGE_READWRITE | PAGE_GUARD);
        break;
      case 6:
        maximum = captured_maximum;
        test.maximum_kind = 1;
        protect(captured_maximum, PAGE_NOACCESS);
        break;
      case 7:
        maximum = captured_maximum;
        test.maximum_kind = 1;
        protect(captured_maximum, PAGE_READWRITE | PAGE_GUARD);
        break;
      case 8:
        maximum = reinterpret_cast<PLARGE_INTEGER>(memory + 4096 + 1);
        test.maximum_kind = 2;
        break;
      case 9:
        maximum = captured_maximum;
        captured_maximum->QuadPart = -1;
        test.maximum = -1;
        test.maximum_kind = 1;
        break;
      case 10:
        object = object_storage;
        protect(object, PAGE_NOACCESS);
        break;
      case 11:
        object = object_storage;
        protect(object, PAGE_READWRITE | PAGE_GUARD);
        break;
      case 12:
        object = object_storage;
        object->Length = 0;
        break;
      case 13:
        object = object_storage;
        object->Attributes = 0xFFFFFFFF;
        break;
      case 14:
        object = object_storage;
        object->SecurityDescriptor = memory + 3 * 4096;
        protect(memory + 3 * 4096, PAGE_NOACCESS);
        break;
      case 15:
        argument_file =
            reinterpret_cast<HANDLE>(static_cast<uintptr_t>(0x123456));
        break;
      case 16:
        event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
        if (!event)
          std::abort();
        argument_file = event;
        break;
      case 17:
        break;
      case 18:
        test.section_access = 0x8000;
        break;
      case 19:
        argument_file = nullptr;
        break;
      case 20:
        object = object_storage;
        object->RootDirectory =
            reinterpret_cast<HANDLE>(static_cast<uintptr_t>(0x123456));
        break;
      default:
        std::abort();
      }
    };
    fault(test.left);
    fault(test.right);
    if (test.null_output)
      output = nullptr;
    LONG status = invoke(create, output, test, maximum, argument_file, object);
    auto protection_at = [](const void *at) {
      MEMORY_BASIC_INFORMATION info{};
      return at && VirtualQuery(at, &info, sizeof(info)) == sizeof(info)
                 ? info.Protect
                 : 0UL;
    };
    DWORD output_protection = protection_at(output);
    DWORD maximum_protection = protection_at(maximum);
    DWORD object_protection = protection_at(object);
    HANDLE section = sentinel;
    if (output && output_protection &&
        !(output_protection & (PAGE_GUARD | PAGE_NOACCESS)))
      std::memcpy(&section, output, sizeof(section));
    std::printf("fault role=%u left=%d right=%d output-protection=%08lX "
                "maximum-protection=%08lX object-protection=%08lX output=%s\n",
                role, test.left, test.right, output_protection,
                maximum_protection, object_protection,
                section == sentinel ? "sentinel"
                : section           ? "published"
                                    : "zero");
    LONG basic_status = static_cast<LONG>(0xDEADBEEF);
    LONG object_status = static_cast<LONG>(0xDEADBEEF);
    SIZE_T returned = 0;
    ULONG object_returned = 0;
    alignas(8) std::array<unsigned char, 64> basic{}, object_info{};
    bool valid = section && section != reinterpret_cast<HANDLE>(
                                           static_cast<uintptr_t>(-3));
    if (valid) {
      basic_status = query(section, 0, basic.data(),
                           static_cast<ULONG>(basic.size()), &returned);
      auto query_object =
          reinterpret_cast<QueryObject>(GetProcAddress(ntdll, "NtQueryObject"));
      object_status =
          query_object(section, 0, object_info.data(), 56, &object_returned);
    }
    ULONG grant = 0;
    std::memcpy(&grant, object_info.data() + 4, sizeof(grant));
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
    if (event)
      CloseHandle(event);
    if (!VirtualFree(memory, 0, MEM_RELEASE))
      return 25;
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
