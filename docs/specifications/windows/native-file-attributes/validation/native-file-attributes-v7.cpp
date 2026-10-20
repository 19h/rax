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
    for (unsigned role = 1; role <= 162; ++role) {
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
        output = memory + 4096 - 24;
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
      if (role >= 43 && role <= 47) {
        output = memory + 4096;
        if (!VirtualProtect(memory + 4096, 4096, PAGE_READWRITE | PAGE_GUARD,
                            &old))
          return 26;
        if (role == 43)
          attributes.Length = 0;
        if (role == 44)
          name.Length = 1;
        if (role == 45)
          attributes.Attributes = 0xFFFFFFFF;
        if (role == 46) {
          memcpy(memory + 513, &attributes, sizeof(attributes));
          input = memory + 513;
        }
        if (role == 47)
          attributes.ObjectName = nullptr;
      }
      if (role == 48 || role == 92)
        attributes.SecurityDescriptor = reinterpret_cast<void *>(1);
      if (role == 49 || role == 93)
        attributes.SecurityQualityOfService = reinterpret_cast<void *>(1);
      if (role == 50)
        attributes.SecurityDescriptor = memory + 2048;
      if (role == 51) {
        memset(memory + 2048, 0, 12);
        *reinterpret_cast<ULONG *>(memory + 2048) = 12;
        *reinterpret_cast<ULONG *>(memory + 2052) = 2;
        attributes.SecurityQualityOfService = memory + 2048;
      }
      if (role == 52)
        name.MaximumLength = 0;
      if (role == 53) {
        memcpy(memory + 1025, path.data(), path.size() * 2);
        name.Buffer = reinterpret_cast<wchar_t *>(memory + 1025);
      }
      if (role == 54)
        attributes.ObjectName = reinterpret_cast<UNICODE_STRING *>(1);
      if (role == 55) {
        name.Length = 65534;
        name.MaximumLength = 65534;
        name.Buffer = reinterpret_cast<wchar_t *>(memory + 1024);
      }
      if (role == 56) {
        attributes.RootDirectory = reinterpret_cast<HANDLE>(0x6FFC);
        name.Length = 0;
        name.Buffer = nullptr;
      }
      if (role == 57)
        attributes.RootDirectory = reinterpret_cast<HANDLE>(0x6FFC);
      if (role == 58)
        attributes.RootDirectory = event;
      if (role == 59) {
        for (auto &unit : path)
          if (unit >= L'a' && unit <= L'z')
            unit -= L'a' - L'A';
        attributes.Attributes = 0;
      }
      if (role >= 60 && role <= 91)
        attributes.Attributes = ULONG(1) << (role - 60);
      if (role == 92 || role == 93 || role == 94) {
        output = memory + 4096;
        if (!VirtualProtect(memory + 4096, 4096, PAGE_READWRITE | PAGE_GUARD,
                            &old))
          return 27;
        if (role == 94) {
          attributes.RootDirectory = reinterpret_cast<HANDLE>(0x6FFC);
          name.Length = 0;
          name.Buffer = nullptr;
        }
      }

      SECURITY_DESCRIPTOR descriptor{};
      SECURITY_QUALITY_OF_SERVICE qos{};
      if (role >= 95 && role <= 103) {
        if (!InitializeSecurityDescriptor(&descriptor,
                                          SECURITY_DESCRIPTOR_REVISION))
          return 28;
        attributes.SecurityDescriptor = &descriptor;
        if (role == 96 || role == 100)
          descriptor.Revision = 0;
        if (role == 97)
          descriptor.Control = SE_SELF_RELATIVE;
        if (role == 98)
          descriptor.Control = SE_DACL_PRESENT;
        if (role == 99) {
          descriptor.Control = SE_DACL_PRESENT;
          descriptor.Dacl = reinterpret_cast<ACL *>(1);
        }
        if (role == 101)
          descriptor.Owner = reinterpret_cast<SID *>(1);
        if (role == 102)
          descriptor.Group = reinterpret_cast<SID *>(1);
        if (role == 103) {
          descriptor.Control = SE_SACL_PRESENT;
          descriptor.Sacl = reinterpret_cast<ACL *>(1);
        }
      }
      if (role == 100 || (role >= 114 && role <= 116)) {
        output = memory + 4096;
        if (!VirtualProtect(memory + 4096, 4096, PAGE_READWRITE | PAGE_GUARD,
                            &old))
          return 29;
      }
      if (role >= 104 && role <= 116) {
        qos.Length = sizeof(qos);
        qos.ImpersonationLevel = SecurityImpersonation;
        attributes.SecurityQualityOfService = &qos;
        if (role == 104 || role == 114)
          qos.Length = 0;
        if (role == 105)
          qos.Length = 11;
        if (role == 106)
          qos.Length = 13;
        if (role >= 107 && role <= 111)
          qos.ImpersonationLevel =
              static_cast<SECURITY_IMPERSONATION_LEVEL>(role - 107);
        if (role == 112 || role == 115)
          qos.ContextTrackingMode = 2;
        if (role == 113 || role == 116)
          qos.EffectiveOnly = 2;
      }
      if (role >= 117 && role <= 133) {
        std::wstring suffix;
        switch (role) {
        case 117:
          suffix = L"\\.";
          break;
        case 118:
          suffix = L"\\..\\attributes-fixture.bin";
          break;
        case 119:
          suffix = L"\\";
          break;
        case 120:
          suffix = L".";
          break;
        case 121:
          suffix = L" ";
          break;
        case 122:
          path = nt(std::wstring(cwd)) + L"\\.\\attributes-fixture.bin";
          break;
        case 123:
          path = nt(std::wstring(cwd)) +
                 L"\\attributes-dir\\..\\attributes-fixture.bin";
          break;
        case 124:
          path = nt(std::wstring(cwd)) + L"\\\\attributes-fixture.bin";
          break;
        case 125:
          path = nt(file);
          path[4] = L'c';
          break;
        case 126:
          path = nt(file);
          for (auto &x : path)
            if (x == L'\\')
              x = L'/';
          break;
        case 127:
          path = L"\\DosDevices\\" + file;
          break;
        case 128:
          path = L"\\??\\C:";
          break;
        case 129:
          path = L"\\??\\C:\\";
          break;
        case 130:
          path = L"\\??\\Z:\\no-file";
          break;
        case 131:
          path = L"C:\\attributes-fixture.bin";
          break;
        case 132:
          path = L"\\attributes-fixture.bin";
          break;
        case 133:
          path = nt(file);
          path.push_back(0);
          path += L".missing";
          break;
        }
        if (role <= 121)
          path += suffix;
        name.Length = static_cast<USHORT>(path.size() * 2);
        name.MaximumLength = name.Length;
        name.Buffer = path.data();
      }
      HANDLE file_root = INVALID_HANDLE_VALUE;
      if (role >= 134 && role <= 162) {
        bool dir = role >= 139;
        DWORD access =
            (role == 139
                 ? 0
                 : (role == 140
                        ? FILE_READ_ATTRIBUTES
                        : (role == 141 ? FILE_LIST_DIRECTORY
                                       : (role == 142 ? FILE_TRAVERSE
                                                      : GENERIC_READ))));
        file_root = CreateFileW(
            dir ? cwd : file.c_str(), access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr,
            OPEN_EXISTING, dir ? FILE_FLAG_BACKUP_SEMANTICS : 0, nullptr);
        if (file_root == INVALID_HANDLE_VALUE)
          return 30;
        attributes.RootDirectory = file_root;
        if (role == 134 || role == 139 || role == 140 || role == 141 ||
            role == 142 || role == 143)
          path = L"";
        if (role == 135)
          path = L"child";
        if (role == 136)
          path = L"\\child";
        if (role == 137)
          path = nt(file);
        if (role == 138) {
          path = L"";
          attributes.ObjectName = nullptr;
        }
        name.Length = static_cast<USHORT>(path.size() * 2);
        name.MaximumLength = name.Length;
        name.Buffer = path.data();
      }

      if (role >= 144 && role <= 152) {
        output = memory + 4096;
        if (!VirtualProtect(memory + 4096, 4096, PAGE_READWRITE | PAGE_GUARD,
                            &old))
          return 32;
        if (role == 144)
          input = nullptr;
        if (role == 145)
          attributes.ObjectName = reinterpret_cast<UNICODE_STRING *>(1);
        if (role == 146 || role == 147)
          name.Buffer = reinterpret_cast<wchar_t *>(memory + 8192);
        if (role == 147)
          name.Length = 1;
        if (role == 146 || role == 147)
          if (!VirtualProtect(memory + 8192, 4096, PAGE_NOACCESS, &old))
            return 33;
        if (role >= 148 && role <= 150) {
          if (!InitializeSecurityDescriptor(&descriptor,
                                            SECURITY_DESCRIPTOR_REVISION))
            return 34;
          attributes.SecurityDescriptor = &descriptor;
          if (role == 148)
            descriptor.Owner = reinterpret_cast<SID *>(1);
          if (role == 149) {
            descriptor.Control = SE_DACL_PRESENT;
            descriptor.Dacl = reinterpret_cast<ACL *>(1);
          }
          if (role == 150) {
            descriptor.Control = SE_SACL_PRESENT;
            descriptor.Sacl = reinterpret_cast<ACL *>(1);
          }
        }
        if (role == 151)
          attributes.SecurityDescriptor = memory + 8192;
        if (role == 152)
          attributes.SecurityQualityOfService = memory + 8192;
        if (role == 151 || role == 152)
          if (!VirtualProtect(memory + 8192, 4096, PAGE_NOACCESS, &old))
            return 35;
      }
      if (role >= 153 && role <= 156) {
        output =
            memory + 4096 -
            (role == 153 ? 36 : (role == 154 ? 40 : (role == 155 ? 52 : 56)));
        window = memory + 4096 - 64;
        if (!VirtualProtect(memory + 4096, 4096, PAGE_NOACCESS, &old))
          return 36;
      }
      if (role >= 157 && role <= 162) {
        if (!InitializeSecurityDescriptor(&descriptor,
                                          SECURITY_DESCRIPTOR_REVISION))
          return 37;
        attributes.SecurityDescriptor = &descriptor;
        auto sid = reinterpret_cast<SID *>(memory + 2048);
        memset(sid, 0, 80);
        sid->Revision = (role == 157 ? 0 : 1);
        sid->SubAuthorityCount = (role == 158 ? 16 : 1);
        sid->IdentifierAuthority.Value[5] = 1;
        sid->SubAuthority[0] = 0;
        descriptor.Owner = sid;
        if (role >= 159) {
          descriptor.Owner = nullptr;
          auto acl = reinterpret_cast<ACL *>(memory + 2048);
          memset(acl, 0, 80);
          acl->AclRevision = (role == 159 ? 0 : 2);
          acl->AclSize = (role == 160 ? 0 : (role == 161 ? 7 : 8));
          descriptor.Control = SE_DACL_PRESENT;
          descriptor.Dacl = acl;
        }
      }
      ULONG exception = 0;
      LONG status = invoke(full, input, output, &exception);
      MEMORY_BASIC_INFORMATION protection{};
      if (!VirtualQuery(memory + 4096, &protection, sizeof(protection)))
        return 20;
      printf("case full=%u role=%u status=%08lX exception=%08lX "
             "second-protect=%08lX bytes=",
             full, role, status, exception, protection.Protect);
      bytes(window, 64);
      printf("\n");
      if (file_root != INVALID_HANDLE_VALUE && !CloseHandle(file_root))
        return 31;
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
