#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
struct Io {
  union {
    LONG status;
    void *pointer;
  };
  ULONG_PTR information;
};
using Query = LONG(NTAPI *)(HANDLE, Io *, void *, ULONG, ULONG);
static LONG invoke(Query q, HANDLE h, Io *io, void *out, ULONG len,
                   bool *returned) {
  *returned = false;
  __try {
    LONG status = q(h, io, out, len, 4);
    *returned = true;
    return status;
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    return static_cast<LONG>(GetExceptionCode());
  }
}
static void protect(void *p, SIZE_T n, DWORD value) {
  DWORD old = 0;
  if (!VirtualProtect(p, n, value, &old))
    ExitProcess(20);
}
static DWORD protection(void *p) {
  MEMORY_BASIC_INFORMATION m = {};
  if (!VirtualQuery(p, &m, sizeof(m)))
    ExitProcess(21);
  return m.Protect;
}
static void hex(const unsigned char *p, size_t n) {
  for (size_t i = 0; i < n; ++i)
    printf("%02X", static_cast<unsigned>(p[i]));
}
int main() {
  static_assert(sizeof(Io) == 2 * sizeof(void *));
  auto query = reinterpret_cast<Query>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtQueryVolumeInformationFile"));
  if (!query)
    return 2;
  SYSTEM_INFO system = {};
  GetSystemInfo(&system);
  if (system.dwPageSize != 4096)
    return 3;
  auto *ib = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 12288, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  auto *ob = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 12288, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!ib || !ob)
    return 4;
  HANDLE con = CreateFileW(L"CONIN$", GENERIC_READ | GENERIC_WRITE,
                           FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr,
                           OPEN_EXISTING, 0, nullptr);
  HANDLE nul = CreateFileW(L"NUL", GENERIC_READ | GENERIC_WRITE,
                           FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr,
                           OPEN_EXISTING, 0, nullptr);
  HANDLE disk =
      CreateFileW(L"C:\\Windows\\System32\\ntdll.dll", GENERIC_READ,
                  FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                  nullptr, OPEN_EXISTING, 0, nullptr);
  HANDLE event = CreateEventW(nullptr, FALSE, FALSE, nullptr), rd = nullptr,
         wr = nullptr;
  if (!CreatePipe(&rd, &wr, nullptr, 0))
    return 5;
  if (con == INVALID_HANDLE_VALUE || nul == INVALID_HANDLE_VALUE ||
      disk == INVALID_HANDLE_VALUE || !event)
    return 6;
  struct Role {
    const char *name;
    HANDLE handle;
    bool owned;
  };
  const Role roles[] = {
      {"conin", con, true},
      {"nul", nul, true},
      {"disk", disk, true},
      {"pipe-read", rd, true},
      {"event", event, true},
      {"invalid", reinterpret_cast<HANDLE>(static_cast<ULONG_PTR>(0xDEAD)),
       false}};
  const char *cases[] = {"valid",
                         "io-null",
                         "out-null",
                         "both-null",
                         "io-bad",
                         "out-bad",
                         "io-readonly",
                         "out-readonly",
                         "io-guard",
                         "out-guard",
                         "io-noaccess",
                         "out-noaccess",
                         "io-unaligned",
                         "out-unaligned",
                         "same",
                         "out-io-plus4",
                         "io-out-plus4",
                         "io-cross-noaccess",
                         "out-cross-noaccess",
                         "both-guard",
                         "io-readonly-guard",
                         "out-readonly-guard"};
  const ULONG lengths[] = {7, 8};
  unsigned calls = 0, closed = 0;
  for (const auto &role : roles) {
    for (unsigned ci = 0; ci < 22; ++ci)
      for (ULONG len : lengths) {
        protect(ib, 12288, PAGE_READWRITE);
        protect(ob, 12288, PAGE_READWRITE);
        memset(ib, 0xA5, 12288);
        memset(ob, 0xA5, 12288);
        unsigned char *ip = ib + 4096 + 256, *op = ob + 4096 + 256;
        switch (ci) {
        case 1:
          ip = nullptr;
          break;
        case 2:
          op = nullptr;
          break;
        case 3:
          ip = nullptr;
          op = nullptr;
          break;
        case 4:
          ip = reinterpret_cast<unsigned char *>(
              static_cast<ULONG_PTR>(0xDEAD0000));
          break;
        case 5:
          op = reinterpret_cast<unsigned char *>(
              static_cast<ULONG_PTR>(0xDEAD0000));
          break;
        case 6:
          protect(ib + 4096, 4096, PAGE_READONLY);
          break;
        case 7:
          protect(ob + 4096, 4096, PAGE_READONLY);
          break;
        case 8:
          protect(ib + 4096, 4096, PAGE_READWRITE | PAGE_GUARD);
          break;
        case 9:
          protect(ob + 4096, 4096, PAGE_READWRITE | PAGE_GUARD);
          break;
        case 10:
          protect(ib + 4096, 4096, PAGE_NOACCESS);
          break;
        case 11:
          protect(ob + 4096, 4096, PAGE_NOACCESS);
          break;
        case 12:
          ++ip;
          break;
        case 13:
          ++op;
          break;
        case 14:
          op = ip;
          break;
        case 15:
          op = ip + 4;
          break;
        case 16:
          ip = op + 4;
          break;
        case 17:
          ip = ib + 8192 - 8;
          protect(ib + 8192, 4096, PAGE_NOACCESS);
          break;
        case 18:
          op = ob + 8192 - 4;
          protect(ob + 8192, 4096, PAGE_NOACCESS);
          break;
        case 19:
          protect(ib + 4096, 4096, PAGE_READWRITE | PAGE_GUARD);
          protect(ob + 4096, 4096, PAGE_READWRITE | PAGE_GUARD);
          break;
        }
        if (ci == 20)
          protect(ib + 4096, 4096, PAGE_READONLY | PAGE_GUARD);
        if (ci == 21)
          protect(ob + 4096, 4096, PAGE_READONLY | PAGE_GUARD);
        bool returned = false;
        LONG status = invoke(query, role.handle, reinterpret_cast<Io *>(ip), op,
                             len, &returned);
        ++calls;
        DWORD iprot = protection(ib + 4096), oprot = protection(ob + 4096),
              itail = protection(ib + 8192), otail = protection(ob + 8192);
        protect(ib, 12288, PAGE_READWRITE);
        protect(ob, 12288, PAGE_READWRITE);
        printf("role=%s case=%s len=%lu class=4 status=%08lX returned=%u "
               "io-protect=%lX out-protect=%lX io-tail=%lX out-tail=%lX io=",
               role.name, cases[ci], len, static_cast<unsigned long>(status),
               returned, iprot, oprot, itail, otail);
        hex(ib + 4096 + 240, 96);
        printf(" out=");
        hex(ob + 4096 + 240, 96);
        printf(" io-edge=");
        hex(ib + 8192 - 32, 64);
        printf(" out-edge=");
        hex(ob + 8192 - 32, 64);
        printf("\n");
      }
    if (role.owned) {
      if (!CloseHandle(role.handle))
        return 7;
      ++closed;
    }
  }
  if (!CloseHandle(wr))
    return 8;
  ++closed;
  if (!VirtualFree(ib, 0, MEM_RELEASE) || !VirtualFree(ob, 0, MEM_RELEASE))
    return 9;
  printf("summary calls=%u owned=6 closed=%u allocations=2 releases=2 "
         "iosb-bytes=%zu\n",
         calls, closed, sizeof(Io));
  return calls == 264 && closed == 6 ? 0 : 10;
}
