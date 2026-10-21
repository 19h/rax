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
int main() {
  static_assert(sizeof(Io) == 2 * sizeof(void *));
  auto query = reinterpret_cast<Query>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtQueryVolumeInformationFile"));
  if (!query)
    return 2;
  const BOOL allocated = AllocConsole();
  HANDLE disk =
      CreateFileW(L"C:\\Windows\\System32\\ntdll.dll", GENERIC_READ,
                  FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                  nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
  HANDLE dir =
      CreateFileW(L"C:\\Windows", FILE_READ_ATTRIBUTES,
                  FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                  nullptr, OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS, nullptr);
  HANDLE nul = CreateFileW(L"NUL", GENERIC_READ | GENERIC_WRITE,
                           FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr,
                           OPEN_EXISTING, 0, nullptr);
  HANDLE conin = CreateFileW(L"CONIN$", GENERIC_READ | GENERIC_WRITE,
                             FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr,
                             OPEN_EXISTING, 0, nullptr);
  HANDLE conout = CreateFileW(L"CONOUT$", GENERIC_READ | GENERIC_WRITE,
                              FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr,
                              OPEN_EXISTING, 0, nullptr);
  HANDLE event = CreateEventW(nullptr, FALSE, FALSE, nullptr), rd = nullptr,
         wr = nullptr;
  if (!CreatePipe(&rd, &wr, nullptr, 0))
    return 3;
  struct Role {
    const char *name;
    HANDLE handle;
    bool close;
  };
  Role roles[] = {{"disk", disk, true},
                  {"directory", dir, true},
                  {"nul", nul, true},
                  {"conin", conin, true},
                  {"conout", conout, true},
                  {"pipe-read", rd, true},
                  {"pipe-write", wr, true},
                  {"event", event, true},
                  {"stdin", GetStdHandle(STD_INPUT_HANDLE), false},
                  {"stdout", GetStdHandle(STD_OUTPUT_HANDLE), false},
                  {"process", GetCurrentProcess(), false},
                  {"null", nullptr, false},
                  {"invalid",
                   reinterpret_cast<HANDLE>(static_cast<ULONG_PTR>(0xDEAD)),
                   false}};
  unsigned calls = 0, closed = 0, owned = 0, duplicates = 0,
           duplicate_closes = 0;
  for (const auto &role : roles) {
    if (role.close && role.handle && role.handle != INVALID_HANDLE_VALUE)
      ++owned;
    const DWORD grants[] = {0, 0x100000, 0x80};
    for (DWORD grant : grants) {
      HANDLE duplicate = nullptr;
      if (!DuplicateHandle(GetCurrentProcess(), role.handle,
                           GetCurrentProcess(), &duplicate, grant, FALSE, 0)) {
        printf("duplicate role=%s access=%08lX error=%lu\n", role.name, grant,
               GetLastError());
        continue;
      }
      ++duplicates;
      const ULONG lengths[] = {0, 1, 7, 8, 9, 16};
      for (ULONG length : lengths) {
        Io iosb;
        unsigned char output[64];
        memset(&iosb, 0xA5, sizeof(iosb));
        memset(output, 0xA5, sizeof(output));
        const LONG status = query(duplicate, &iosb, output, length, 4);
        ++calls;
        printf("role=%s handle=%llX type=%lu access=%08lX len=%lu class=4 "
               "status=%08lX iosb=",
               role.name,
               static_cast<unsigned long long>(
                   reinterpret_cast<ULONG_PTR>(role.handle)),
               GetFileType(duplicate), grant, length,
               static_cast<unsigned long>(status));
        for (auto c :
             reinterpret_cast<const unsigned char (&)[sizeof(Io)]>(iosb))
          printf("%02X", static_cast<unsigned>(c));
        printf(" out=");
        for (auto c : output)
          printf("%02X", static_cast<unsigned>(c));
        printf("\n");
      }
      if (!CloseHandle(duplicate))
        return 7;
      ++duplicate_closes;
    }
    if (role.close && role.handle && role.handle != INVALID_HANDLE_VALUE) {
      if (!CloseHandle(role.handle))
        return 4;
      ++closed;
    }
  }
  if (allocated && !FreeConsole())
    return 5;
  printf("summary calls=%u owned=%u closed=%u allocated-console=%u\n", calls,
         owned, closed, static_cast<unsigned>(allocated));
  printf("duplicates opened=%u closed=%u\n", duplicates, duplicate_closes);
  return owned == closed && duplicates == duplicate_closes ? 0 : 6;
}
