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
using ConvertDescriptor = BOOL(WINAPI *)(LPCWSTR, DWORD, PSECURITY_DESCRIPTOR *,
                                         PULONG);
using Version = LONG(NTAPI *)(PRTL_OSVERSIONINFOW);
static void fail(const char *where) {
  printf("failure operation=%s error=%lu\n", where, GetLastError());
  ExitProcess(21);
}
static LONG invoke(Access fn, PSECURITY_DESCRIPTOR sd, HANDLE token,
                   ACCESS_MASK desired, GENERIC_MAPPING *mapping,
                   void *privileges, ULONG *length, ACCESS_MASK *granted,
                   LONG *status, ULONG *escaped) {
  __try {
    return fn(sd, token, desired, mapping,
              reinterpret_cast<PPRIVILEGE_SET>(privileges), length, granted,
              status);
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}
int main(int argc, char **argv) {
  if (argc != 2)
    fail("profile");
  auto ntdll = GetModuleHandleW(L"ntdll.dll");
  auto fn = reinterpret_cast<Access>(GetProcAddress(ntdll, "NtAccessCheck"));
  auto version =
      reinterpret_cast<Version>(GetProcAddress(ntdll, "RtlGetVersion"));
  auto convert = reinterpret_cast<ConvertDescriptor>(
      GetProcAddress(GetModuleHandleW(L"advapi32.dll"),
                     "ConvertStringSecurityDescriptorToSecurityDescriptorW"));
  if (!fn || !version || !convert)
    fail("native-exports");
  RTL_OSVERSIONINFOW os{};
  os.dwOSVersionInfoSize = sizeof os;
  if (version(&os) != 0)
    fail("native-version");
  HANDLE primary{}, denied{}, impersonation{};
  if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY | TOKEN_DUPLICATE,
                        &primary))
    fail("primary-token");
  if (!DuplicateHandle(GetCurrentProcess(), primary, GetCurrentProcess(),
                       &denied, 0, FALSE, 0))
    fail("zero-access-token");
  if (!DuplicateToken(primary, SecurityImpersonation, &impersonation))
    fail("impersonation-token");
  DWORD app{}, returned{};
  if (!GetTokenInformation(primary, TokenIsAppContainer, &app, sizeof app,
                           &returned) ||
      returned != sizeof app)
    fail("primary-app-container");
  HANDLE handles[] = {reinterpret_cast<HANDLE>(LONG_PTR(-4)),
                      reinterpret_cast<HANDLE>(LONG_PTR(-5)),
                      reinterpret_cast<HANDLE>(LONG_PTR(-6)),
                      GetCurrentProcess(),
                      GetCurrentThread(),
                      nullptr,
                      reinterpret_cast<HANDLE>(uintptr_t(0xDEAD)),
                      primary,
                      denied,
                      impersonation};
  const char *kinds[] = {
      "primary", "thread-token", "effective",    "process",   "thread",
      "null",    "invalid",      "real-primary", "no-access", "impersonation"};
  const wchar_t *sddl[] = {L"O:SYG:SYD:",
                           L"O:SYG:SYD:(A;;0x1;;;WD)",
                           L"O:SYG:SYD:(D;;0x1;;;WD)(A;;0x3;;;WD)",
                           L"O:SYG:SY",
                           L"O:SYG:SYD:NO_ACCESS_CONTROL",
                           L"D:(A;;0x1;;;WD)"};
  const char *names[] = {"empty-dacl",
                         "allow-one",
                         "deny-one-allow-three",
                         "absent-dacl",
                         "null-dacl",
                         "missing-owner-group",
                         "captured-absolute-nonmember"};
  const ACCESS_MASK desired[] = {0,
                                 1,
                                 2,
                                 3,
                                 MAXIMUM_ALLOWED,
                                 GENERIC_READ,
                                 WRITE_DAC,
                                 READ_CONTROL,
                                 ACCESS_SYSTEM_SECURITY};
  const ULONG sizes[] = {0, 1, 4, 8, 20, 56, 64};
  const unsigned cases = 7 * 10 * 9 * 7;
  printf("identity profile=%s pointer-bytes=%zu build=%lu.%lu.%lu "
         "app-container=%lu cases=%u\n",
         argv[1], sizeof(void *), os.dwMajorVersion, os.dwMinorVersion,
         os.dwBuildNumber, app, cases);
  unsigned ordinal = 0;
  for (unsigned d = 0; d < 7; ++d) {
    PSECURITY_DESCRIPTOR sd{};
    ULONG sd_bytes{};
    SECURITY_DESCRIPTOR absolute{};
    alignas(8) BYTE acl_bytes[236]{};
    BYTE nonmember_sid[] = {
        1,    9,    0,    0,    0,    0,    0,    5,    0x20, 0,    0,
        0,    0xd2, 0x7f, 0xa5, 0x5f, 0xc1, 0x6d, 0xe5, 0x1f, 0x25, 0x75,
        0x21, 0xc3, 0xef, 0x08, 0x91, 0xda, 0x68, 0x77, 0x0b, 0x14, 0x90,
        0xc7, 0xcb, 0x81, 0xa0, 0x2c, 0x8b, 0xa3, 0xb8, 0x13, 0xe7, 0xaa};
    if (d == 6) {
      BOOL member = TRUE;
      auto acl = reinterpret_cast<PACL>(acl_bytes);
      if (!CheckTokenMembership(impersonation, nonmember_sid, &member) ||
          member ||
          !InitializeSecurityDescriptor(&absolute,
                                        SECURITY_DESCRIPTOR_REVISION) ||
          !InitializeAcl(acl, sizeof acl_bytes, ACL_REVISION) ||
          !AddAccessAllowedAce(acl, ACL_REVISION, 1, nonmember_sid) ||
          !SetSecurityDescriptorOwner(&absolute, nonmember_sid, FALSE) ||
          !SetSecurityDescriptorGroup(&absolute, nonmember_sid, FALSE) ||
          !SetSecurityDescriptorDacl(&absolute, TRUE, acl, FALSE))
        fail("captured-absolute-descriptor");
      sd = &absolute;
      sd_bytes = sizeof absolute;
    } else if (!convert(sddl[d], 1, &sd, &sd_bytes))
      fail("controlled-security-descriptor");
    printf("descriptor name=%s bytes=%lu raw=", names[d], sd_bytes);
    for (ULONG i = 0; i < sd_bytes; ++i)
      printf("%02X", static_cast<const BYTE *>(sd)[i]);
    puts("");
    for (unsigned h = 0; h < 10; ++h)
      for (ACCESS_MASK request : desired)
        for (ULONG size : sizes) {
          GENERIC_MAPPING mapping =
              d == 6 ? GENERIC_MAPPING{0x20001, 0x20000, 0x20000, 0x1f0001}
                     : GENERIC_MAPPING{1, 2, 4, 7};
          alignas(8) BYTE privileges[64];
          memset(privileges, 0xA5, sizeof privileges);
          ULONG length = size, escaped = 0;
          ACCESS_MASK granted = 0xA5A5A5A5;
          LONG access_status = LONG(0xA5A5A5A5);
          SetLastError(0x7E11ABCD);
          LONG result =
              invoke(fn, sd, handles[h], request, &mapping, privileges, &length,
                     &granted, &access_status, &escaped);
          DWORD error = GetLastError();
          printf("case ordinal=%u descriptor=%s token=%s desired=%08lX "
                 "length=%lu return=%08lX escaped=%08lX last-error=%08lX "
                 "returned=%lu granted=%08lX access-status=%08lX privileges=",
                 ordinal++, names[d], kinds[h], request, size, ULONG(result),
                 escaped, error, length, granted, ULONG(access_status));
          for (BYTE byte : privileges)
            printf("%02X", byte);
          puts("");
        }
    if (d != 6 && LocalFree(sd) != nullptr)
      fail("descriptor-cleanup");
  }
  if (ordinal != cases || !CloseHandle(primary) || !CloseHandle(denied) ||
      !CloseHandle(impersonation))
    fail("owned-handle-cleanup");
  printf("complete cases=%u closed-handles=3 released-descriptors=6\n",
         ordinal);
  return 0;
}
