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
struct Id {
  unsigned long long volume;
  unsigned char id[16];
};
static void hex(const void *data, size_t n) {
  const auto *p = static_cast<const unsigned char *>(data);
  for (size_t i = 0; i < n; ++i)
    printf("%02X", static_cast<unsigned>(p[i]));
}
int main() {
  auto query = reinterpret_cast<Query>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtQueryVolumeInformationFile"));
  if (!query)
    return 2;
  HANDLE disk = CreateFileW(L"C:\\Windows\\System32\\ntdll.dll", GENERIC_READ,
                            7, nullptr, OPEN_EXISTING, 0, nullptr);
  HANDLE dir = CreateFileW(L"C:\\Windows", FILE_READ_ATTRIBUTES, 7, nullptr,
                           OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS, nullptr);
  HANDLE own = CreateFileW(L"private-volume-query.dat",
                           GENERIC_READ | GENERIC_WRITE, 7, nullptr, CREATE_NEW,
                           FILE_ATTRIBUTE_TEMPORARY | FILE_FLAG_DELETE_ON_CLOSE,
                           nullptr);
  if (disk == INVALID_HANDLE_VALUE || dir == INVALID_HANDLE_VALUE ||
      own == INVALID_HANDLE_VALUE)
    return 3;
  struct Role {
    const char *name;
    HANDLE handle;
    bool directory;
    bool owned;
  };
  const Role roles[] = {
      {"disk", disk, false, true},
      {"directory", dir, true, true},
      {"private", own, false, true},
      {"shared-output", GetStdHandle(STD_OUTPUT_HANDLE), false, false}};
  unsigned calls = 0, reopens = 0, closes = 0, failures = 0;
  for (const auto &role : roles) {
    LARGE_INTEGER before = {}, after = {}, seek = {};
    seek.QuadPart = 7;
    bool positioned = role.owned && !role.directory;
    if (positioned && !SetFilePointerEx(role.handle, seek, &before, FILE_BEGIN))
      return 4;
    Io original_io;
    unsigned char original[8];
    memset(&original_io, 0xA5, sizeof(original_io));
    memset(original, 0xA5, 8);
    LONG original_status = query(role.handle, &original_io, original, 8, 4);
    ++calls;
    const DWORD flag_values[] = {0, FILE_FLAG_BACKUP_SEMANTICS};
    for (DWORD flags : flag_values) {
      HANDLE reopened = ReOpenFile(role.handle, 0, 7, flags);
      DWORD error = reopened == INVALID_HANDLE_VALUE ? GetLastError() : 0;
      if (reopened == INVALID_HANDLE_VALUE) {
        printf("role=%s flags=%08lX reopen=0 error=%lu\n", role.name, flags,
               error);
        ++failures;
        continue;
      }
      ++reopens;
      Id id1 = {}, id2 = {};
      BOOL id_ok = GetFileInformationByHandleEx(role.handle, FileIdInfo, &id1,
                                                sizeof(id1)) &&
                   GetFileInformationByHandleEx(reopened, FileIdInfo, &id2,
                                                sizeof(id2)) &&
                   memcmp(&id1, &id2, sizeof(Id)) == 0;
      Io io;
      unsigned char output[8];
      memset(&io, 0xA5, sizeof(io));
      memset(output, 0xA5, 8);
      LONG status = query(reopened, &io, output, 8, 4);
      ++calls;
      bool cursor_ok =
          !positioned || (SetFilePointerEx(role.handle, LARGE_INTEGER{}, &after,
                                           FILE_CURRENT) &&
                          after.QuadPart == before.QuadPart);
      bool ok = status == 0 && original_status == 0 &&
                memcmp(original, output, 8) == 0 && id_ok && cursor_ok &&
                io.information == 8;
      failures += !ok;
      printf("role=%s flags=%08lX reopen=1 status=%08lX identity=%u cursor=%u "
             "ok=%u original-io=",
             role.name, flags, static_cast<unsigned long>(status),
             static_cast<unsigned>(id_ok), cursor_ok, ok);
      hex(&original_io, sizeof(original_io));
      printf(" io=");
      hex(&io, sizeof(io));
      printf(" out=");
      hex(output, 8);
      printf("\n");
      if (!CloseHandle(reopened))
        return 5;
      ++closes;
    }
  }
  for (const auto &role : roles)
    if (role.owned && !CloseHandle(role.handle))
      return 6;
  printf("summary calls=%u reopened=%u closed=%u failures=%u original-owned=3 "
         "original-closed=3\n",
         calls, reopens, closes, failures);
  return failures || reopens != closes ? 1 : 0;
}
