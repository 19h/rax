#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <windows.h>
#include <winternl.h>

static ULONG selected_class = 0;

using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using Version = LONG(NTAPI *)(PRTL_OSVERSIONINFOW);

static void fail(const char *operation) {
  printf("failure operation=%s error=%lu\n", operation, GetLastError());
  ExitProcess(21);
}
static LONG invoke(Query query, HANDLE token, void *output, ULONG length,
                   ULONG *returned, ULONG *escaped) {
  __try {
    return query(token, selected_class, output, length, returned);
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}
static void readback(const char *name, const void *address) {
  BYTE data[4]{};
  SIZE_T got = 0;
  BOOL ok =
      ReadProcessMemory(GetCurrentProcess(), address, data, sizeof data, &got);
  printf(" %s-ok=%u %s-got=%zu %s=", name, unsigned(ok), name, got, name);
  for (SIZE_T i = 0; i < got; ++i)
    printf("%02X", data[i]);
  printf(" %s-cells=", name);
  for (SIZE_T i = 0; i < 4; ++i) {
    BYTE cell = 0;
    SIZE_T count = 0;
    auto address_value = reinterpret_cast<uintptr_t>(address) + i;
    if (ReadProcessMemory(GetCurrentProcess(),
                          reinterpret_cast<void *>(address_value), &cell, 1,
                          &count) &&
        count == 1)
      printf("%02X", cell);
    else
      printf("??");
  }
}
static DWORD protection(const void *address) {
  MEMORY_BASIC_INFORMATION info{};
  return VirtualQuery(address, &info, sizeof info) ? info.Protect : 0;
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 3)
    fail("profile-class");
  char *end = nullptr;
  selected_class = strtoul(argv[2], &end, 10);
  if (!end || *end || selected_class == 0)
    fail("class");
  auto query = reinterpret_cast<Query>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtQueryInformationToken"));
  auto version = reinterpret_cast<Version>(
      GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "RtlGetVersion"));
  RTL_OSVERSIONINFOW build{};
  build.dwOSVersionInfoSize = sizeof build;
  if (!query || !version || version(&build) != 0)
    fail("exports-version");
  SYSTEM_INFO info{};
  GetSystemInfo(&info);
  const SIZE_T page = info.dwPageSize;
  auto arena = static_cast<BYTE *>(VirtualAlloc(
      nullptr, page * 8, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!arena)
    fail("owned-arena");
  DWORD handles_before = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &handles_before))
    fail("initial-handle-count");
  HANDLE real = nullptr, no_access = nullptr, impersonation = nullptr;
  if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY | TOKEN_DUPLICATE,
                        &real))
    fail("open-query-primary-token");
  if (!DuplicateHandle(GetCurrentProcess(), real, GetCurrentProcess(),
                       &no_access, 0, FALSE, 0))
    fail("duplicate-zero-access-primary-token");
  if (!DuplicateTokenEx(real, TOKEN_QUERY, nullptr, SecurityImpersonation,
                        TokenImpersonation, &impersonation))
    fail("duplicate-owned-impersonation-token");
  HANDLE event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
  if (!event)
    fail("owned-event");
  ULONG session = 0xA5A5A5A5, session_bytes = 0;
  LONG session_status =
      query(real, selected_class, &session, sizeof session, &session_bytes);
  printf("reference class=%lu status=%08lX value=%08lX bytes=%lu\n",
         selected_class, ULONG(session_status), session, session_bytes);
  alignas(void *) BYTE reference[4096];
  memset(reference, 0xA5, sizeof reference);
  ULONG required = 0;
  LONG reference_status =
      query(real, selected_class, reference, sizeof reference, &required);
  if (selected_class != 1 || reference_status != 0 ||
      required > sizeof reference)
    fail("bounded-token-user-reference");
  auto user = reinterpret_cast<TOKEN_USER *>(reference);
  auto sid_address = reinterpret_cast<uintptr_t>(user->User.Sid);
  auto reference_address = reinterpret_cast<uintptr_t>(reference);
  if (sid_address < reference_address ||
      sid_address - reference_address > required || !IsValidSid(user->User.Sid))
    fail("reference-sid");
  DWORD sid_bytes = GetLengthSid(user->User.Sid);
  if (sid_bytes > required - (sid_address - reference_address) ||
      !IsValidSid(user->User.Sid))
    fail("reference-sid-extent");
  printf("user-reference required=%lu header=%zu sid-offset=%zu "
         "attributes=%08lX sid-bytes=%lu sid=",
         required, sizeof(TOKEN_USER), size_t(sid_address - reference_address),
         user->User.Attributes, sid_bytes);
  for (DWORD i = 0; i < sid_bytes; ++i)
    printf("%02X", reinterpret_cast<BYTE *>(user->User.Sid)[i]);
  puts("");
  const char *kinds[] = {"primary", "thread-token", "effective",    "null",
                         "process", "thread",       "invalid",      "event",
                         "real",    "no-access",    "impersonation"};
  HANDLE handles[] = {reinterpret_cast<HANDLE>(intptr_t(-4)),
                      reinterpret_cast<HANDLE>(intptr_t(-5)),
                      reinterpret_cast<HANDLE>(intptr_t(-6)),
                      nullptr,
                      GetCurrentProcess(),
                      GetCurrentThread(),
                      reinterpret_cast<HANDLE>(uintptr_t(0xDEAD)),
                      event,
                      real,
                      no_access,
                      impersonation};
  const ULONG lengths[] = {
      0,  1,  2,  3,  4,  5,  6,  7,  8,  9,   10,  11,   12,        13,
      14, 15, 16, 17, 18, 19, 20, 21, 22, 23,  24,  25,   26,        27,
      28, 29, 30, 31, 32, 33, 34, 35, 36, 37,  38,  39,   40,        41,
      42, 43, 44, 45, 46, 47, 48, 49, 50, 51,  52,  53,   54,        55,
      56, 57, 58, 59, 60, 61, 62, 63, 64, 128, 256, 4096, 0xFFFFFFFF};
  const ULONG special_lengths[] = {0, 19, 20, 21, 28, 32, 64, 4096, 0xFFFFFFFF};
  constexpr unsigned roles = 43;
  const unsigned cases =
      unsigned(sizeof handles / sizeof handles[0]) *
          unsigned(sizeof lengths / sizeof lengths[0]) +
      unsigned(sizeof handles / sizeof handles[0]) * roles *
          unsigned(sizeof special_lengths / sizeof special_lengths[0]);
  printf("identity profile=%s pointer-bytes=%zu build=%lu.%lu.%lu page=%zu "
         "cases=%u\n",
         argv[1], sizeof(uintptr_t), build.dwMajorVersion, build.dwMinorVersion,
         build.dwBuildNumber, page, cases);
  unsigned ordinal = 0;
  auto run = [&](const char *kind, HANDLE handle, ULONG length, unsigned role) {
    DWORD old = 0;
    if (!VirtualProtect(arena, page * 8, PAGE_READWRITE, &old))
      fail("reset-owned-protection");
    memset(arena, 0xA5, page * 8);
    BYTE *output = arena + page;
    BYTE *returned = arena + page * 5;
    BYTE *protect_at = nullptr;
    DWORD protect = PAGE_READWRITE;
    switch (role) {
    case 0:
      break;
    case 1:
      output = nullptr;
      break;
    case 2:
      output += 1;
      break;
    case 3:
      output += 2;
      break;
    case 4:
      output += 3;
      break;
    case 5:
      returned = nullptr;
      break;
    case 6:
      returned += 1;
      break;
    case 7:
      returned += 2;
      break;
    case 8:
      returned += 3;
      break;
    case 9:
      protect_at = output;
      protect = PAGE_NOACCESS;
      break;
    case 10:
      protect_at = output;
      protect = PAGE_READONLY;
      break;
    case 11:
      protect_at = output;
      protect = PAGE_READWRITE | PAGE_GUARD;
      break;
    case 12:
      protect_at = output;
      protect = PAGE_READONLY | PAGE_GUARD;
      break;
    case 13:
      protect_at = returned;
      protect = PAGE_NOACCESS;
      break;
    case 14:
      protect_at = returned;
      protect = PAGE_READONLY;
      break;
    case 15:
      protect_at = returned;
      protect = PAGE_READWRITE | PAGE_GUARD;
      break;
    case 16:
      protect_at = returned;
      protect = PAGE_READONLY | PAGE_GUARD;
      break;
    case 17:
      returned = output;
      break;
    case 18:
      returned = output + 1;
      break;
    case 19:
      output += page - 2;
      protect_at = arena + page * 2;
      protect = PAGE_NOACCESS;
      break;
    case 20:
      returned += page - 2;
      protect_at = arena + page * 6;
      protect = PAGE_NOACCESS;
      break;
    case 21:
      output = nullptr;
      returned = nullptr;
      break;
    case 22:
      output = reinterpret_cast<BYTE *>(uintptr_t(1));
      returned = nullptr;
      break;
    case 23:
      output = nullptr;
      returned += 1;
      break;
    case 24:
      output += page - 4;
      protect_at = arena + page * 2;
      protect = PAGE_NOACCESS;
      break;
    case 41:
    case 42:
      output = nullptr;
      protect_at = returned;
      protect = (role == 41 ? PAGE_READWRITE : PAGE_READONLY) | PAGE_GUARD;
      break;
    default:
      if (role < 25 || role > 40)
        fail("role");
      break;
    }
    if (role >= 25 && role <= 40) {
      const DWORD pair_protections[] = {PAGE_NOACCESS, PAGE_READONLY,
                                        PAGE_READWRITE | PAGE_GUARD,
                                        PAGE_READONLY | PAGE_GUARD};
      unsigned pair = role - 25;
      if (!VirtualProtect(output, page, pair_protections[pair / 4], &old) ||
          !VirtualProtect(returned, page, pair_protections[pair % 4], &old))
        fail("set-owned-paired-protection");
    }
    if (protect_at && !VirtualProtect(protect_at, page, protect, &old))
      fail("set-owned-case-protection");
    ULONG escaped = 0;
    SetLastError(0x7E11ABCD);
    LONG status = invoke(query, handle, output, length,
                         reinterpret_cast<ULONG *>(returned), &escaped);
    DWORD error = GetLastError();
    DWORD output_protect = protection(output),
          returned_protect = protection(returned);
    printf(
        "case ordinal=%u kind=%s role=%u handle=%llX length=%08lX "
        "status=%08lX escaped=%08lX last-error=%08lX output=%llX returned=%llX "
        "output-protect=%08lX returned-protect=%08lX",
        ordinal++, kind, role,
        static_cast<unsigned long long>(reinterpret_cast<uintptr_t>(handle)),
        length, ULONG(status), escaped, error,
        static_cast<unsigned long long>(reinterpret_cast<uintptr_t>(output)),
        static_cast<unsigned long long>(reinterpret_cast<uintptr_t>(returned)),
        output_protect, returned_protect);
    readback("output-bytes", output);
    readback("return-bytes", returned);
    BYTE payload[128]{};
    SIZE_T payload_bytes = 0;
    BOOL payload_ok = ReadProcessMemory(GetCurrentProcess(), output, payload,
                                        sizeof payload, &payload_bytes);
    printf(" payload-ok=%u payload-bytes=%zu payload=", unsigned(payload_ok),
           payload_bytes);
    for (SIZE_T i = 0; i < payload_bytes; ++i)
      printf("%02X", payload[i]);

    puts("");
  };
  for (unsigned h = 0; h < sizeof handles / sizeof handles[0]; ++h)
    for (ULONG length : lengths)
      run(kinds[h], handles[h], length, 0);
  for (unsigned h = 0; h < sizeof handles / sizeof handles[0]; ++h)
    for (unsigned role = 0; role < roles; ++role)
      for (ULONG length : special_lengths)
        run(kinds[h], handles[h], length, role);
  unsigned boundaries = 0;
  const DWORD protections[] = {PAGE_NOACCESS, PAGE_READONLY,
                               PAGE_READWRITE | PAGE_GUARD,
                               PAGE_READONLY | PAGE_GUARD};
  for (unsigned subject = 0; subject < 2; ++subject)
    for (unsigned side = 0; side < 2; ++side)
      for (DWORD protect : protections)
        for (unsigned remaining = 0; remaining <= 40; ++remaining) {
          DWORD old = 0;
          if (!VirtualProtect(arena, page * 8, PAGE_READWRITE, &old))
            fail("boundary-reset-protection");
          memset(arena, 0xA5, page * 8);
          BYTE *output = arena + page * 2 - remaining;
          ULONG *returned = reinterpret_cast<ULONG *>(arena + page * 5);
          if (!VirtualProtect(arena + page * (1 + side), page, protect, &old))
            fail("boundary-protect");
          ULONG escaped = 0;
          LONG status = invoke(query, subject == 0 ? real : impersonation,
                               output, 64, returned, &escaped);
          DWORD first_after = protection(arena + page),
                second_after = protection(arena + page * 2);
          printf(
              "boundary ordinal=%u subject=%u side=%u remaining=%u "
              "protection=%08lX "
              "status=%08lX escaped=%08lX first-after=%08lX second-after=%08lX "
              "returned=%08lX output=%llX cells=",
              boundaries++, subject, side, remaining, protect, ULONG(status),
              escaped, first_after, second_after, *returned,
              static_cast<unsigned long long>(
                  reinterpret_cast<uintptr_t>(output)));
          for (unsigned offset = 0; offset < 64; ++offset) {
            BYTE value = 0;
            SIZE_T got = 0;
            if (ReadProcessMemory(GetCurrentProcess(), output + offset, &value,
                                  1, &got) &&
                got == 1)
              printf("%02X", value);
            else
              printf("??");
          }
          puts("");
        }
  if (boundaries != 656)
    fail("boundary-count");
  printf("boundary-complete cases=%u\n", boundaries);
  if (ordinal != cases || !CloseHandle(real) || !CloseHandle(no_access) ||
      !CloseHandle(event) || !CloseHandle(impersonation) ||
      !VirtualFree(arena, 0, MEM_RELEASE))
    fail("complete-owned-cleanup");
  DWORD handles_after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &handles_after))
    fail("final-handle-count");
  printf("complete cases=%u closed-handles=4 released-arenas=1 "
         "handles-before=%lu handles-after=%lu\n",
         ordinal, handles_before, handles_after);
  if (handles_before != handles_after)
    fail("unbalanced-handles");
  return 0;
}
