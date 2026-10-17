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

static void snapshot(unsigned index, const void *address) {
  MEMORY_BASIC_INFORMATION info{};
  DWORD protection =
      VirtualQuery(address, &info, sizeof info) ? info.Protect : 0;
  printf(" p%u-protect=%08lX p%u-cells=", index, protection, index);
  for (unsigned i = 0; i < 8; ++i) {
    BYTE value{};
    SIZE_T got{};
    auto p = reinterpret_cast<const void *>(
        reinterpret_cast<uintptr_t>(address) + i);
    if (ReadProcessMemory(GetCurrentProcess(), p, &value, 1, &got) && got == 1)
      printf("%02X", value);
    else
      printf("??");
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
  if (!convert(L"O:SYG:SYD:(A;;0x10003;;;WD)", 1, &security, &security_length))
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
  GUID system_type{0x48f96463,
                   0x52d6,
                   0x4e85,
                   {0x9c, 0x02, 0xf2, 0xc1, 0x4e, 0x17, 0x26, 0x4e}};
  uint64_t owned[3]{};
  for (unsigned i = 0; i < 3; ++i) {
    LONG status = create(&owned[i], 3, 0, FALSE, i == 2 ? &type : nullptr, 4096,
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
  auto arena = static_cast<BYTE *>(VirtualAlloc(nullptr, system.dwPageSize * 12,
                                                MEM_RESERVE | MEM_COMMIT,
                                                PAGE_READWRITE));
  if (!arena)
    fail("output-arena");
  RTL_OSVERSIONINFOW os{};
  os.dwOSVersionInfoSize = sizeof os;
  if (version(&os) < 0)
    fail("version");
  const uint64_t names[] = {owned[0], owned[1], 0x0280032EA3BC0875ULL, 0};
  const char *roles[] = {"owned-empty", "owned-payload", "captured-system",
                         "zero"};
  const ULONG capacities[] = {0, 1, 8, 16, 64, 0xFFFFFFFF};
  const DWORD protections[] = {PAGE_NOACCESS, PAGE_READONLY,
                               PAGE_READWRITE | PAGE_GUARD,
                               PAGE_READONLY | PAGE_GUARD};
  const unsigned cases = 4 * 6 * 65;
  printf("identity profile=%s pointer-bytes=%zu build=%lu.%lu.%lu page=%lu "
         "cases=%u owned=3\n",
         argv[1], sizeof(void *), os.dwMajorVersion, os.dwMinorVersion,
         os.dwBuildNumber, system.dwPageSize, cases);
  unsigned ordinal = 0;
  for (unsigned name = 0; name < 4; ++name)
    for (ULONG capacity : capacities)
      for (unsigned role = 0; role < 65; ++role) {
        DWORD old{};
        SIZE_T page = system.dwPageSize;
        if (!VirtualProtect(arena, page * 12, PAGE_READWRITE, &old))
          fail("reset-protection");
        memset(arena, 0xA5, page * 12);
        void *p[6];
        for (unsigned i = 0; i < 6; ++i)
          p[i] = arena + page * 2 * i;
        memcpy(p[0], &names[name], 8);
        const GUID *selected = name == 2 ? &system_type : &type;
        memcpy(p[1], selected, 16);
        memcpy(p[2], user_sid, GetLengthSid(user_sid));
        void *scope_storage = p[2];
        p[2] = nullptr;
        int moved = -1;
        if (role >= 1 && role <= 24) {
          unsigned field = (role - 1) / 4, kind = (role - 1) % 4;
          if (field == 2)
            p[field] = scope_storage;
          if (kind == 0)
            p[field] = nullptr;
          else {
            p[field] = static_cast<BYTE *>(p[field]) + (kind == 3 ? 4 : kind);
            moved = int(field);
          }
        } else if (role >= 25 && role <= 48) {
          unsigned field = (role - 25) / 4;
          if (field == 2)
            p[field] = scope_storage;
        } else if (role >= 49 && role <= 54) {
          unsigned field = role - 49;
          if (field == 2)
            p[field] = scope_storage;
          p[field] = static_cast<BYTE *>(p[field]) + page - 2;
          moved = int(field);
        } else if (role == 55)
          p[5] = p[3];
        else if (role == 56)
          p[5] = p[4];
        else if (role == 57)
          p[3] = p[4];
        else if (role == 58)
          p[3] = p[0];
        else if (role == 59)
          p[5] = p[0];
        else if (role == 60)
          p[3] = p[5] = p[4];
        if (moved == 0)
          memcpy(p[0], &names[name], 8);
        if (moved == 1)
          memcpy(p[1], selected, 16);
        if (moved == 2)
          memcpy(p[2], user_sid, GetLengthSid(user_sid));
        if (p[5])
          memcpy(p[5], &capacity, 4);
        if (role >= 25 && role <= 48) {
          unsigned field = (role - 25) / 4;
          if (!VirtualProtect(p[field], page, protections[(role - 25) % 4],
                              &old))
            fail("pointer-protection");
        }
        if (role >= 49 && role <= 54) {
          unsigned field = role - 49;
          if (!VirtualProtect(arena + page * (2 * field + 1), page,
                              PAGE_NOACCESS, &old))
            fail("crossing-protection");
        }
        if (role >= 61) {
          unsigned first = (role == 61 || role == 62) ? 0 : 3;
          unsigned second = (role == 61 || role == 63) ? 5 : 4;
          if (!VirtualProtect(arena + page * 2 * first, page,
                              PAGE_READWRITE | PAGE_GUARD, &old) ||
              !VirtualProtect(arena + page * 2 * second, page,
                              PAGE_READWRITE | PAGE_GUARD, &old))
            fail("dual-guard-protection");
        }
        ULONG escaped = 0;
        SetLastError(0x7E11ABCD);
        LONG status = invoke(query, static_cast<const uint64_t *>(p[0]),
                             static_cast<const GUID *>(p[1]), p[2],
                             static_cast<ULONG *>(p[3]), p[4],
                             static_cast<ULONG *>(p[5]), &escaped);
        DWORD error = GetLastError();
        printf("case ordinal=%u name=%s capacity=%08lX role=%u return=%08lX "
               "escaped=%08lX last-error=%08lX",
               ordinal++, roles[name], capacity, role, ULONG(status), escaped,
               error);
        for (unsigned i = 0; i < 6; ++i)
          snapshot(i, p[i]);
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
