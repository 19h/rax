#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <windows.h>
#include <winternl.h>

using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using Set = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG);
struct Owned {
  HANDLE value = nullptr;
  Owned() = default;
  Owned(const Owned &) = delete;
  ~Owned() {
    if (value)
      CloseHandle(value);
  }
};
struct Revert {
  bool active = false;
  ~Revert() {
    if (active)
      RevertToSelf();
  }
};
static int unavailable(const char *stage) {
  printf("unavailable stage=%s error=%lu\n", stage, GetLastError());
  return 0;
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 2;
  auto module = GetModuleHandleW(L"ntdll.dll");
  auto query = reinterpret_cast<Query>(
      GetProcAddress(module, "NtQueryInformationToken"));
  auto set =
      reinterpret_cast<Set>(GetProcAddress(module, "NtSetInformationToken"));
  if (!query || !set)
    return 3;
  printf("identity profile=%s pointer-bytes=%zu "
         "private-token-session-controls=1\n",
         argv[1], sizeof(uintptr_t));
  Owned primary, authority;
  if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY | TOKEN_DUPLICATE,
                        &primary.value))
    return unavailable("open-primary");
  if (!DuplicateTokenEx(primary.value, TOKEN_ALL_ACCESS, nullptr,
                        SecurityImpersonation, TokenImpersonation,
                        &authority.value))
    return unavailable("private-authority");
  TOKEN_PRIVILEGES privilege{};
  privilege.PrivilegeCount = 1;
  if (!LookupPrivilegeValueW(nullptr, L"SeTcbPrivilege",
                             &privilege.Privileges[0].Luid))
    return unavailable("lookup-existing-privilege");
  privilege.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
  SetLastError(0);
  if (!AdjustTokenPrivileges(authority.value, FALSE, &privilege, 0, nullptr,
                             nullptr) ||
      GetLastError() != 0)
    return unavailable("enable-private-authority-privilege");
  Revert revert;
  if (!SetThreadToken(nullptr, authority.value))
    return unavailable("select-private-authority");
  revert.active = true;
  constexpr ULONG values[] = {0, 1, 7, 0x12345678, 0x80000000, 0xFFFFFFFF};
  unsigned successes = 0;
  for (ULONG wanted : values) {
    Owned target, duplicate;
    if (!DuplicateTokenEx(primary.value, TOKEN_ALL_ACCESS, nullptr,
                          SecurityImpersonation, TokenPrimary, &target.value))
      return 4;
    ULONG before = 0xA5A5A5A5, after = 0xA5A5A5A5, copy = 0xA5A5A5A5, n0 = 0,
          n1 = 0, n2 = 0;
    SetLastError(0x7E11ABCD);
    LONG first = query(target.value, 12, &before, 4, &n0);
    LONG status = set(target.value, 12, &wanted, 4);
    LONG second = query(target.value, 12, &after, 4, &n1);
    DWORD last = GetLastError();
    if (!DuplicateTokenEx(target.value, TOKEN_ALL_ACCESS, nullptr,
                          SecurityImpersonation, TokenPrimary,
                          &duplicate.value))
      return 5;
    LONG third = query(duplicate.value, 12, &copy, 4, &n2);
    printf("case requested=%08lX set-status=%08lX before-status=%08lX "
           "before=%08lX before-length=%lu after-status=%08lX after=%08lX "
           "after-length=%lu duplicate-status=%08lX duplicate=%08lX "
           "duplicate-length=%lu last-error=%08lX\n",
           wanted, ULONG(status), ULONG(first), before, n0, ULONG(second),
           after, n1, ULONG(third), copy, n2, last);
    if (first != 0 || second != 0 || third != 0 || n0 != 4 || n1 != 4 ||
        n2 != 4 || copy != after || after != (status >= 0 ? wanted : before) ||
        last != 0x7E11ABCD)
      return 6;
    successes += status >= 0;
  }
  if (!RevertToSelf())
    return 7;
  revert.active = false;
  printf(
      "complete cases=%zu successful-private-session-changes=%u reverted=1\n",
      sizeof(values) / sizeof(values[0]), successes);
  return 0;
}
