#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
#include <winternl.h>
using Access = LONG(NTAPI *)(PSECURITY_DESCRIPTOR, HANDLE, ACCESS_MASK,
                             PGENERIC_MAPPING, PPRIVILEGE_SET, PULONG,
                             PACCESS_MASK, PLONG);
using Convert = BOOL(WINAPI *)(LPCWSTR, DWORD, PSECURITY_DESCRIPTOR *, PULONG);
using Version = LONG(NTAPI *)(PRTL_OSVERSIONINFOW);
static void fail(const char *where) {
  printf("failure operation=%s error=%lu\n", where, GetLastError());
  ExitProcess(21);
}
static LONG invoke(Access fn, HANDLE token, void **p, ULONG *escaped) {
  __try {
    return fn(p[0], token, 1, static_cast<PGENERIC_MAPPING>(p[1]),
              static_cast<PPRIVILEGE_SET>(p[2]), static_cast<PULONG>(p[3]),
              static_cast<PACCESS_MASK>(p[4]), static_cast<PLONG>(p[5]));
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}
static void snapshot(unsigned index, const void *p) {
  MEMORY_BASIC_INFORMATION mbi{};
  DWORD protect = VirtualQuery(p, &mbi, sizeof mbi) ? mbi.Protect : 0;
  printf(" p%u-protect=%08lX p%u-cells=", index, protect, index);
  for (unsigned i = 0; i < 8; ++i) {
    BYTE value{};
    SIZE_T got{};
    auto address =
        reinterpret_cast<const void *>(reinterpret_cast<uintptr_t>(p) + i);
    if (ReadProcessMemory(GetCurrentProcess(), address, &value, 1, &got) &&
        got == 1)
      printf("%02X", value);
    else
      printf("??");
  }
}
int main(int argc, char **argv) {
  if (argc != 2)
    fail("profile");
  auto ntdll = GetModuleHandleW(L"ntdll.dll");
  auto fn = reinterpret_cast<Access>(GetProcAddress(ntdll, "NtAccessCheck"));
  auto version =
      reinterpret_cast<Version>(GetProcAddress(ntdll, "RtlGetVersion"));
  auto convert = reinterpret_cast<Convert>(
      GetProcAddress(GetModuleHandleW(L"advapi32.dll"),
                     "ConvertStringSecurityDescriptorToSecurityDescriptorW"));
  if (!fn || !version || !convert)
    fail("exports");
  RTL_OSVERSIONINFOW os{};
  os.dwOSVersionInfoSize = sizeof os;
  if (version(&os))
    fail("version");
  PSECURITY_DESCRIPTOR descriptor{};
  ULONG descriptor_bytes{};
  if (!convert(L"O:SYG:SYD:(A;;0x1;;;WD)", 1, &descriptor, &descriptor_bytes))
    fail("descriptor");
  HANDLE primary{}, denied{};
  if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &primary) ||
      !DuplicateHandle(GetCurrentProcess(), primary, GetCurrentProcess(),
                       &denied, 0, FALSE, 0))
    fail("tokens");
  HANDLE event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
  if (!event)
    fail("event");
  SYSTEM_INFO info{};
  GetSystemInfo(&info);
  SIZE_T page = info.dwPageSize;
  if (descriptor_bytes > page)
    fail("descriptor-span");
  auto arena = static_cast<BYTE *>(VirtualAlloc(
      nullptr, page * 12, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!arena)
    fail("arena");
  HANDLE handles[] = {reinterpret_cast<HANDLE>(LONG_PTR(-4)),
                      reinterpret_cast<HANDLE>(LONG_PTR(-5)),
                      reinterpret_cast<HANDLE>(LONG_PTR(-6)),
                      reinterpret_cast<HANDLE>(uintptr_t(0xDEAD)),
                      denied,
                      event};
  const char *kinds[] = {"primary", "thread-token", "effective",
                         "invalid", "no-access",    "event"};
  const ULONG lengths[] = {0, 8, 20, 56, 0xFFFFFFFF};
  const DWORD protections[] = {PAGE_NOACCESS, PAGE_READONLY,
                               PAGE_READWRITE | PAGE_GUARD,
                               PAGE_READONLY | PAGE_GUARD};
  const unsigned cases = 6 * 5 * 50;
  printf("identity profile=%s pointer-bytes=%zu build=%lu.%lu.%lu page=%zu "
         "cases=%u\n",
         argv[1], sizeof(void *), os.dwMajorVersion, os.dwMinorVersion,
         os.dwBuildNumber, page, cases);
  unsigned ordinal = 0;
  for (unsigned h = 0; h < 6; ++h)
    for (ULONG length : lengths)
      for (unsigned role = 0; role < 50; ++role) {
        DWORD old{};
        if (!VirtualProtect(arena, page * 12, PAGE_READWRITE, &old))
          fail("reset-protection");
        memset(arena, 0xA5, page * 12);
        void *p[6];
        for (unsigned i = 0; i < 6; ++i)
          p[i] = arena + page * (2 * i);
        memcpy(p[0], descriptor, descriptor_bytes);
        GENERIC_MAPPING mapping{1, 2, 4, 7};
        memcpy(p[1], &mapping, sizeof mapping);
        memcpy(p[3], &length, sizeof length);
        int moved = -1;
        if (role >= 1 && role <= 12) {
          unsigned field = (role - 1) / 2;
          if (role % 2)
            p[field] = nullptr;
          else {
            p[field] = static_cast<BYTE *>(p[field]) + 1;
            moved = int(field);
          }
        } else if (role >= 37 && role <= 42) {
          unsigned field = role - 37;
          p[field] = static_cast<BYTE *>(p[field]) + page - 2;
          moved = int(field);
        } else if (role == 43)
          p[5] = p[4];
        else if (role == 44)
          p[4] = p[2];
        else if (role == 45)
          p[5] = p[2];
        else if (role == 46)
          p[3] = p[2];
        else if (role == 47)
          p[3] = p[4] = p[5] = p[2];
        else if (role == 48)
          p[3] = static_cast<BYTE *>(p[2]) + 4;
        else if (role == 49)
          p[0] = p[1];
        if (moved == 0)
          memcpy(p[0], descriptor, descriptor_bytes);
        if (moved == 1)
          memcpy(p[1], &mapping, sizeof mapping);
        if (p[3])
          memcpy(p[3], &length, sizeof length);
        if (role >= 13 && role <= 36) {
          unsigned index = role - 13;
          if (!VirtualProtect(p[index / 4], page, protections[index % 4], &old))
            fail("controlled-protection");
        }
        if (role >= 37 && role <= 42 &&
            !VirtualProtect(arena + page * (2 * (role - 37) + 1), page,
                            PAGE_NOACCESS, &old))
          fail("controlled-crossing");
        ULONG escaped = 0;
        SetLastError(0x7E11ABCD);
        LONG status = invoke(fn, handles[h], p, &escaped);
        DWORD error = GetLastError();
        printf("case ordinal=%u token=%s length=%08lX role=%u return=%08lX "
               "escaped=%08lX last-error=%08lX",
               ordinal++, kinds[h], length, role, ULONG(status), escaped,
               error);
        for (unsigned i = 0; i < 6; ++i)
          snapshot(i, p[i]);
        puts("");
      }
  if (ordinal != cases || !CloseHandle(primary) || !CloseHandle(denied) ||
      !CloseHandle(event) || LocalFree(descriptor) != nullptr ||
      !VirtualFree(arena, 0, MEM_RELEASE))
    fail("cleanup");
  printf("complete cases=%u closed-handles=3 released-descriptors=1 "
         "released-arenas=1\n",
         ordinal);
  return 0;
}
