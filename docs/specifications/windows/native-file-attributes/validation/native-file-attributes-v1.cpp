#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <windows.h>
#include <winternl.h>

using Query = LONG(NTAPI *)(void *, void *);
static Query basic_query;
static Query full_query;
struct Basic {
  LARGE_INTEGER creation, access, write, change;
  ULONG attributes;
};
struct Full {
  LARGE_INTEGER creation, access, write, change, allocation, size;
  ULONG attributes;
};
static_assert(sizeof(Basic) == 40);
static_assert(sizeof(Full) == 56);
static LONG invoke(bool full, void *attributes, void *output,
                   ULONG *exception) {
  *exception = 0;
  __try {
    return (full ? full_query : basic_query)(attributes, output);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
    return static_cast<LONG>(0xBAADF00D);
  }
}
static void bytes(const unsigned char *data, size_t size) {
  for (size_t i = 0; i < size; ++i)
    printf("%02X", data[i]);
}
static std::wstring nt(const std::wstring &path) { return L"\\??\\" + path; }
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto ntdll = GetModuleHandleW(L"ntdll.dll");
  basic_query =
      reinterpret_cast<Query>(GetProcAddress(ntdll, "NtQueryAttributesFile"));
  full_query = reinterpret_cast<Query>(
      GetProcAddress(ntdll, "NtQueryFullAttributesFile"));
  if (!basic_query || !full_query)
    return 1;
  printf(
      "profile=%s pointer=%zu attributes=%zu unicode=%zu basic=%zu full=%zu\n",
      argc > 1 ? argv[1] : "unknown", sizeof(void *), sizeof(OBJECT_ATTRIBUTES),
      sizeof(UNICODE_STRING), sizeof(Basic), sizeof(Full));
  DWORD before = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 2;
  wchar_t cwd[4096];
  DWORD length = GetCurrentDirectoryW(4096, cwd);
  if (length == 0 || length >= 4096)
    return 3;
  std::wstring file = std::wstring(cwd) + L"\\attributes-fixture.bin";
  std::wstring directory = std::wstring(cwd) + L"\\attributes-dir";
  HANDLE created =
      CreateFileW(file.c_str(), GENERIC_READ | GENERIC_WRITE,
                  FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                  nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
  if (created == INVALID_HANDLE_VALUE ||
      !CreateDirectoryW(directory.c_str(), nullptr))
    return 4;
  DWORD written = 0;
  if (!WriteFile(created, "hello", 5, &written, nullptr) || written != 5)
    return 5;
  FILE_BASIC_INFO times{};
  times.CreationTime.QuadPart = 132000000000000001LL;
  times.LastAccessTime.QuadPart = 132000000000000002LL;
  times.LastWriteTime.QuadPart = 132000000000000003LL;
  times.ChangeTime.QuadPart = 132000000000000004LL;
  times.FileAttributes = FILE_ATTRIBUTE_NORMAL;
  if (!SetFileInformationByHandle(created, FileBasicInfo, &times,
                                  sizeof(times)))
    return 6;
  if (!CloseHandle(created))
    return 7;
  HANDLE root =
      CreateFileW(cwd, FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES,
                  FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                  nullptr, OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS, nullptr);
  HANDLE event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
  if (root == INVALID_HANDLE_VALUE || !event)
    return 8;
  auto memory = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 16384, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!memory)
    return 9;
  for (bool full : {false, true}) {
    for (unsigned role = 1; role <= 42; ++role) {
      DWORD old = 0;
      if (!VirtualProtect(memory, 16384, PAGE_READWRITE, &old))
        return 10;
      memset(memory, 0xA5, 16384);
      std::wstring path = nt(file);
      if (role == 2)
        path += L".missing";
      if (role == 3)
        path = nt(std::wstring(cwd) + L"\\MissingParent\\child");
      if (role == 4)
        path += L"\\child";
      if (role == 5)
        path = nt(directory);
      if (role == 6)
        path = L"\\??\\C:\\WINDOWS\\SYSTEM32\\VERSION.dll";
      if (role == 7 || role == 10 || role == 23)
        path.clear();
      if (role == 8 || role == 9 || role == 12 || role == 13)
        path = L"attributes-fixture.bin";
      UNICODE_STRING name{};
      name.Length = static_cast<USHORT>(path.size() * 2);
      name.MaximumLength = name.Length;
      name.Buffer = path.data();
      OBJECT_ATTRIBUTES attributes{};
      attributes.Length = sizeof(attributes);
      attributes.ObjectName = &name;
      attributes.Attributes = 0x840;
      if (role == 9 || role == 10 || role == 11)
        attributes.RootDirectory = root;
      if (role == 12)
        attributes.RootDirectory = event;
      if (role == 13 || role == 31)
        attributes.RootDirectory = reinterpret_cast<HANDLE>(0x6FFC);
      if (role == 15)
        attributes.ObjectName = nullptr;
      if (role == 16)
        attributes.Length = 0;
      if (role == 17)
        attributes.Length = sizeof(attributes) - 1;
      if (role == 18)
        attributes.Length = sizeof(attributes) + 1;
      if (role == 19)
        attributes.Attributes = 0;
      if (role == 20)
        attributes.Attributes = 0xFFFFFFFF;
      if (role == 21)
        name.Length = 1;
      if (role == 22)
        name.Buffer = nullptr;
      if (role == 23)
        name.Buffer = nullptr;
      void *input = &attributes;
      auto output = memory + 128;
      auto window = output;
      if (role == 14 || role == 30)
        input = nullptr;
      if (role == 24 || role == 30 || role == 31)
        output = nullptr;
      if (role >= 25 && role <= 27)
        output += 1u << (role - 25);
      if (role == 28) {
        output = memory + 4096 - 20;
        window = memory + 4096 - 64;
        if (!VirtualProtect(memory + 4096, 4096, PAGE_NOACCESS, &old))
          return 11;
      }
      if (role == 29) {
        output = memory + 4096;
        attributes.RootDirectory = reinterpret_cast<HANDLE>(0x6FFC);
        if (!VirtualProtect(memory + 4096, 4096, PAGE_NOACCESS, &old))
          return 12;
      }
      if (role == 32) {
        memcpy(memory + 513, &attributes, sizeof(attributes));
        input = memory + 513;
      }
      if (role == 33) {
        input = memory + 4096 - sizeof(attributes) / 2;
        memcpy(input, &attributes, sizeof(attributes));
        if (!VirtualProtect(memory + 4096, 4096, PAGE_NOACCESS, &old))
          return 13;
      }
      if (role == 34) {
        attributes.ObjectName = reinterpret_cast<UNICODE_STRING *>(
            memory + 4096 - sizeof(name) / 2);
        memcpy(attributes.ObjectName, &name, sizeof(name));
        if (!VirtualProtect(memory + 4096, 4096, PAGE_NOACCESS, &old))
          return 14;
      }
      if (role == 35) {
        name.Buffer = reinterpret_cast<wchar_t *>(memory + 4096);
        if (!VirtualProtect(memory + 4096, 4096, PAGE_NOACCESS, &old))
          return 15;
      }
      if (role == 36) {
        output = memory + 4096;
        if (!VirtualProtect(memory + 4096, 4096, PAGE_READWRITE | PAGE_GUARD,
                            &old))
          return 16;
      }
      if (role == 37)
        attributes.ObjectName =
            reinterpret_cast<UNICODE_STRING *>(memory + 513),
        memcpy(memory + 513, &name, sizeof(name));
      if (role == 38) {
        memcpy(output, &attributes, sizeof(attributes));
        input = output;
      }
      if (role == 39) {
        memcpy(output, &name, sizeof(name));
        attributes.ObjectName = reinterpret_cast<UNICODE_STRING *>(output);
      }
      if (role == 40) {
        memcpy(output, path.data(), path.size() * 2);
        name.Buffer = reinterpret_cast<wchar_t *>(output);
      }
      if (role == 41 &&
          !SetFileAttributesW(file.c_str(), FILE_ATTRIBUTE_READONLY))
        return 17;
      HANDLE exclusive = INVALID_HANDLE_VALUE;
      if (role == 42) {
        if (!SetFileAttributesW(file.c_str(), FILE_ATTRIBUTE_NORMAL))
          return 18;
        exclusive = CreateFileW(file.c_str(), GENERIC_READ, 0, nullptr,
                                OPEN_EXISTING, 0, nullptr);
        if (exclusive == INVALID_HANDLE_VALUE)
          return 19;
      }
      ULONG exception = 0;
      LONG status = invoke(full, input, output, &exception);
      MEMORY_BASIC_INFORMATION protection{};
      if (!VirtualQuery(memory + 4096, &protection, sizeof(protection)))
        return 20;
      printf("case full=%u role=%u status=%08lX exception=%08lX "
             "second-protect=%08lX bytes=",
             full, role, status, exception, protection.Protect);
      bytes(window, 96);
      printf("\n");
      if (exclusive != INVALID_HANDLE_VALUE && !CloseHandle(exclusive))
        return 21;
      if (role == 41 &&
          !SetFileAttributesW(file.c_str(), FILE_ATTRIBUTE_NORMAL))
        return 22;
    }
  }
  if (!VirtualFree(memory, 0, MEM_RELEASE) || !CloseHandle(event) ||
      !CloseHandle(root) || !DeleteFileW(file.c_str()) ||
      !RemoveDirectoryW(directory.c_str()))
    return 23;
  DWORD after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 24;
  printf("finished=1 handle-count-before=%lu handle-count-after=%lu\n", before,
         after);
  return before == after ? 0 : 25;
}
