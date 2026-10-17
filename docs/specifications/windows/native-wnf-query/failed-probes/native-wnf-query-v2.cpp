#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <vector>
#include <windows.h>
#include <winternl.h>

using Create = LONG(NTAPI *)(uint64_t *, ULONG, ULONG, BOOLEAN, const GUID *,
                             ULONG, PSECURITY_DESCRIPTOR);
using Update = LONG(NTAPI *)(const uint64_t *, const void *, ULONG,
                             const GUID *, const void *, ULONG, ULONG);
using Query = LONG(NTAPI *)(const uint64_t *, const GUID *, const void *,
                            ULONG *, void *, ULONG *);
using Delete = LONG(NTAPI *)(const uint64_t *);
using Convert = BOOL(WINAPI *)(LPCWSTR, DWORD, PSECURITY_DESCRIPTOR *, PULONG);
using Version = LONG(NTAPI *)(PRTL_OSVERSIONINFOW);

static void fail(const char *operation) {
  printf("failure operation=%s last-error=%lu\n", operation, GetLastError());
  ExitProcess(23);
}

static LONG invoke(Query query, const uint64_t *name, const GUID *type,
                   const void *scope, ULONG *stamp, void *buffer, ULONG *length,
                   ULONG *escaped) {
  __try {
    return query(name, type, scope, stamp, buffer, length);
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    fail("profile");
  auto module = GetModuleHandleW(L"ntdll.dll");
  auto create =
      reinterpret_cast<Create>(GetProcAddress(module, "NtCreateWnfStateName"));
  auto update =
      reinterpret_cast<Update>(GetProcAddress(module, "NtUpdateWnfStateData"));
  auto query =
      reinterpret_cast<Query>(GetProcAddress(module, "NtQueryWnfStateData"));
  auto remove =
      reinterpret_cast<Delete>(GetProcAddress(module, "NtDeleteWnfStateName"));
  auto version =
      reinterpret_cast<Version>(GetProcAddress(module, "RtlGetVersion"));
  HANDLE token{};
  if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token))
    fail("primary-token");
  auto convert = reinterpret_cast<Convert>(
      GetProcAddress(GetModuleHandleW(L"advapi32.dll"),
                     "ConvertStringSecurityDescriptorToSecurityDescriptorW"));
  if (!create || !update || !query || !remove || !version || !convert)
    fail("exports");
  DWORD user_length = 0;
  GetTokenInformation(token, TokenUser, nullptr, 0, &user_length);
  if (!user_length || user_length > 65536)
    fail("user-length");
  std::vector<BYTE> user(user_length);
  if (!GetTokenInformation(token, TokenUser, user.data(), user_length,
                           &user_length))
    fail("user");
  auto user_sid = reinterpret_cast<TOKEN_USER *>(user.data())->User.Sid;
  BYTE world[SECURITY_MAX_SID_SIZE]{};
  DWORD world_length = sizeof world;
  if (!CreateWellKnownSid(WinWorldSid, nullptr, world, &world_length))
    fail("world-sid");
  BYTE invalid_sid[16]{};
  invalid_sid[0] = 2;
  PSECURITY_DESCRIPTOR security{};
  ULONG security_length{};
  if (!convert(L"O:SYG:SYD:(A;;0x3;;;WD)", 1, &security, &security_length))
    fail("security-descriptor");
  GUID type{0x4A341B78,
            0x2B2C,
            0x4D4E,
            {0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88}};
  GUID wrong{0x7A341B78,
             0x2B2C,
             0x4D4E,
             {0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x89}};
  GUID zero{};
  uint64_t owned[3]{};
  for (unsigned i = 0; i < 3; ++i) {
    LONG status = create(&owned[i], 3, 3, FALSE, i == 2 ? &type : nullptr, 4096,
                         security);
    if (status < 0) {
      printf("create-failure index=%u status=%08lX\n", i, ULONG(status));
      fail("create-private-process-state");
    }
  }
  const BYTE payload[16] = {'R',  'A',  'X',  0,    1,    2,    3,    4,
                            0x80, 0x90, 0xA0, 0xB0, 0xC0, 0xD0, 0xE0, 0xFF};
  for (unsigned i = 1; i < 3; ++i) {
    LONG status = update(&owned[i], payload, sizeof payload,
                         i == 2 ? &type : nullptr, nullptr, 0, 0);
    if (status < 0) {
      printf("update-failure index=%u status=%08lX\n", i, ULONG(status));
      fail("seed-private-state");
    }
  }
  SYSTEM_INFO system{};
  GetSystemInfo(&system);
  auto arena = static_cast<BYTE *>(VirtualAlloc(nullptr, system.dwPageSize * 2,
                                                MEM_RESERVE | MEM_COMMIT,
                                                PAGE_READWRITE));
  if (!arena)
    fail("output-arena");
  RTL_OSVERSIONINFOW os{};
  os.dwOSVersionInfoSize = sizeof os;
  if (version(&os) < 0)
    fail("version");
  const uint64_t names[] = {owned[0],
                            owned[1],
                            owned[2],
                            0x41911A28A3BC10B5ULL,
                            0,
                            UINT64_MAX,
                            0x41911A2863BC10B5ULL};
  const char *roles[] = {"owned-empty",   "owned-payload", "owned-typed",
                         "known-process", "zero",          "all-ones",
                         "unselected"};
  const GUID *types[] = {nullptr, &type, &wrong, &zero};
  const void *scopes[] = {nullptr, user_sid, world, invalid_sid};
  const ULONG lengths[] = {0, 1, 3, 4, 7, 8, 15, 16, 17, 64, 4096, 0xFFFFFFFF};
  const unsigned cases = 7 * 4 * 4 * 12;
  printf("identity profile=%s pointer-bytes=%zu build=%lu.%lu.%lu page=%lu "
         "cases=%u owned=3\n",
         argv[1], sizeof(void *), os.dwMajorVersion, os.dwMinorVersion,
         os.dwBuildNumber, system.dwPageSize, cases);
  unsigned ordinal = 0;
  for (unsigned name = 0; name < 7; ++name)
    for (unsigned type_index = 0; type_index < 4; ++type_index)
      for (unsigned scope = 0; scope < 4; ++scope)
        for (ULONG capacity : lengths) {
          memset(arena, 0xA5, system.dwPageSize * 2);
          ULONG stamp = 0xA5A5A5A5, length = capacity, escaped = 0;
          SetLastError(0x7E11ABCD);
          LONG result = invoke(query, &names[name], types[type_index],
                               scopes[scope], &stamp, arena, &length, &escaped);
          DWORD error = GetLastError();
          printf(
              "case ordinal=%u name=%s type=%u scope=%u capacity=%08lX "
              "return=%08lX "
              "escaped=%08lX stamp=%08lX length=%08lX last-error=%08lX bytes=",
              ordinal++, roles[name], type_index, scope, capacity,
              ULONG(result), escaped, stamp, length, error);
          for (unsigned i = 0; i < 64; ++i)
            printf("%02X", arena[i]);
          puts("");
        }
  for (unsigned i = 0; i < 3; ++i) {
    LONG status = remove(&owned[i]);
    printf("delete index=%u status=%08lX\n", i, ULONG(status));
    if (status < 0)
      fail("delete-private-state");
  }
  if (!VirtualFree(arena, 0, MEM_RELEASE) || LocalFree(security) != nullptr ||
      !CloseHandle(token))
    fail("cleanup");
  printf("complete cases=%u deleted-states=3 released-arenas=1 "
         "released-descriptors=1 closed-handles=1\n",
         ordinal);
  return 0;
}
