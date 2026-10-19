#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cwchar>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>

struct Owned {
  HANDLE handle = nullptr;
  ~Owned() {
    if (handle)
      CloseHandle(handle);
  }
};
struct ThreadScope {
  bool active = false;
  ~ThreadScope() {
    if (active)
      RevertToSelf();
  }
};
struct Fqbn {
  ULONG64 version;
  UNICODE_STRING name;
};
struct Octet {
  void *value;
  ULONG length;
};
struct Attribute {
  UNICODE_STRING name;
  USHORT type;
  USHORT reserved;
  ULONG flags;
  ULONG count;
  void *values;
};
struct Information {
  USHORT version;
  USHORT reserved;
  ULONG count;
  Attribute *attributes;
};
struct Operation {
  Information *information;
  ULONG *operations;
};
using Query = LONG(NTAPI *)(HANDLE, const UNICODE_STRING *, ULONG, PVOID, ULONG,
                            PULONG);
using Set = LONG(NTAPI *)(HANDLE, ULONG, PVOID, ULONG);
static Query query_attributes;
static Set set_information;

static UNICODE_STRING unicode(const wchar_t *text) {
  UNICODE_STRING value{};
  value.Buffer = const_cast<wchar_t *>(text);
  value.Length = USHORT(wcslen(text) * 2);
  value.MaximumLength = value.Length + 2;
  return value;
}
static LONG set(HANDLE token, Operation *operation, ULONG *exception) {
  *exception = 0;
  LONG status = LONG(0xDEADBEEF);
  __try {
    status = set_information(token, 39, operation, sizeof *operation);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
  }
  return status;
}

int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto ntdll = GetModuleHandleW(L"ntdll.dll");
  query_attributes = reinterpret_cast<Query>(
      GetProcAddress(ntdll, "NtQuerySecurityAttributesToken"));
  set_information =
      reinterpret_cast<Set>(GetProcAddress(ntdll, "NtSetInformationToken"));
  Owned source, private_privileges;
  if (!query_attributes || !set_information ||
      !OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY | TOKEN_DUPLICATE,
                        &source.handle))
    return 2;
  std::array<unsigned char, 4096> before{}, after{};
  DWORD before_bytes = 0, after_bytes = 0;
  if (!GetTokenInformation(source.handle, TokenPrivileges, before.data(),
                           DWORD(before.size()), &before_bytes))
    return 3;
  if (!DuplicateTokenEx(source.handle, TOKEN_ALL_ACCESS, nullptr,
                        SecurityImpersonation, TokenImpersonation,
                        &private_privileges.handle))
    return 4;
  TOKEN_PRIVILEGES privilege{};
  privilege.PrivilegeCount = 1;
  if (!LookupPrivilegeValueW(nullptr, SE_TCB_NAME,
                             &privilege.Privileges[0].Luid))
    return 5;
  privilege.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
  SetLastError(0);
  if (!AdjustTokenPrivileges(private_privileges.handle, FALSE, &privilege, 0,
                             nullptr, nullptr) ||
      GetLastError() != 0)
    return 6;
  ThreadScope thread;
  if (!SetThreadToken(nullptr, private_privileges.handle))
    return 7;
  thread.active = true;
  LONGLONG integers[] = {-1, (-9223372036854775807ll - 1),
                         9223372036854775807ll};
  ULONG64 unsigned_values[] = {0, 0x0123456789ABCDEFull, 0xFFFFFFFFFFFFFFFFull};
  UNICODE_STRING strings[] = {unicode(L"a"), unicode(L"\u03A9\u20AC")};
  Fqbn fqbn[] = {{0x0123456789ABCDEFull, unicode(L"fixture")}};
  unsigned char system_sid[] = {1, 1, 0, 0, 0, 0, 0, 5, 18, 0, 0, 0};
  unsigned char octets[] = {0, 1, 0x7F, 0x80, 0xFF};
  Octet sid[] = {{system_sid, sizeof system_sid}};
  ULONG64 boolean[] = {0, 1};
  Octet blobs[] = {{octets, sizeof octets}};
  unsigned ordinal = 0, successful_types = 0;
  for (USHORT type : {USHORT(1), USHORT(2), USHORT(3), USHORT(4), USHORT(5),
                      USHORT(6), USHORT(16)}) {
    Owned target;
    if (!DuplicateTokenEx(source.handle, TOKEN_ALL_ACCESS, nullptr,
                          SecurityImpersonation, TokenPrimary, &target.handle))
      return 8;
    Attribute attribute{};
    attribute.name = unicode(L"Assist.Fixture");
    attribute.type = type;
    attribute.flags = type == 3 || type == 4 ? 2 : 0;
    switch (type) {
    case 1:
      attribute.count = 3;
      attribute.values = integers;
      break;
    case 2:
      attribute.count = 3;
      attribute.values = unsigned_values;
      break;
    case 3:
      attribute.count = 2;
      attribute.values = strings;
      break;
    case 4:
      attribute.count = 1;
      attribute.values = fqbn;
      break;
    case 5:
      attribute.count = 1;
      attribute.values = sid;
      break;
    case 6:
      attribute.count = 2;
      attribute.values = boolean;
      break;
    case 16:
      attribute.count = 1;
      attribute.values = blobs;
      break;
    }
    Information information{1, 0, 1, &attribute};
    ULONG add = 2;
    Operation operation{&information, &add};
    ULONG exception = 0;
    LONG status = set(target.handle, &operation, &exception);
    printf("fixture type=%u set=%08lX exception=%08lX width=%zu "
           "attribute-bytes=%zu information-bytes=%zu fqbn-bytes=%zu "
           "octet-bytes=%zu\n",
           unsigned(type), ULONG(status), exception, sizeof(void *),
           sizeof(Attribute), sizeof(Information), sizeof(Fqbn), sizeof(Octet));
    if (status < 0 || exception != 0)
      continue;
    ++successful_types;
    for (ULONG length : {0ul, 16ul, 64ul, 104ul, 128ul, 256ul, 512ul, 4096ul}) {
      alignas(16) unsigned char output[4096];
      memset(output, 0xA5, sizeof output);
      ULONG required = 0x11223344;
      SetLastError(0xC0DE);
      LONG query = query_attributes(target.handle, &attribute.name, 1, output,
                                    length, &required);
      DWORD last_error = GetLastError();
      printf("case=%u type=%u width=%zu length=%08lX status=%08lX "
             "required=%08lX last-error=%08lX output=%p bytes=",
             ordinal++, unsigned(type), sizeof(void *), length, ULONG(query),
             required, last_error, output);
      for (unsigned i = 0; i < (length == 0 ? 32 : length); ++i)
        printf("%02X", output[i]);
      puts("");
    }
  }
  if (!RevertToSelf())
    return 9;
  thread.active = false;
  if (!GetTokenInformation(source.handle, TokenPrivileges, after.data(),
                           DWORD(after.size()), &after_bytes) ||
      before_bytes != after_bytes ||
      memcmp(before.data(), after.data(), before_bytes) != 0)
    return 10;
  printf(
      "complete cases=%u successful-types=%u primary-privileges-unchanged=1\n",
      ordinal, successful_types);
  return successful_types == 7 ? 0 : 11;
}
