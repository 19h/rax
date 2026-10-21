#include <cstddef>
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
  static_assert(offsetof(OVERLAPPED, Offset) == sizeof(Io));
  auto query = reinterpret_cast<Query>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtQueryVolumeInformationFile"));
  if (!query)
    return 2;
  HANDLE disk = CreateFileW(L"C:\\Windows\\System32\\ntdll.dll", GENERIC_READ,
                            7, nullptr, OPEN_EXISTING, 0, nullptr);
  HANDLE dir = CreateFileW(L"C:\\Windows", FILE_READ_ATTRIBUTES, 7, nullptr,
                           OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS, nullptr);
  HANDLE async =
      CreateFileW(L"private-volume-async.dat", GENERIC_READ | GENERIC_WRITE, 7,
                  nullptr, CREATE_NEW,
                  FILE_ATTRIBUTE_TEMPORARY | FILE_FLAG_DELETE_ON_CLOSE |
                      FILE_FLAG_OVERLAPPED,
                  nullptr);
  if (disk == INVALID_HANDLE_VALUE || dir == INVALID_HANDLE_VALUE ||
      async == INVALID_HANDLE_VALUE)
    return 3;
  struct Role {
    const char *name;
    HANDLE handle;
    bool owned;
  };
  const Role roles[] = {
      {"disk", disk, true},
      {"directory", dir, true},
      {"async-private", async, true},
      {"shared-output", GetStdHandle(STD_OUTPUT_HANDLE), false}};
  unsigned calls = 0, duplicates = 0, closes = 0, failures = 0;
  const DWORD grants[] = {0, 0x100000};
  for (const auto &role : roles)
    for (DWORD access : grants) {
      HANDLE h = nullptr;
      if (!DuplicateHandle(GetCurrentProcess(), role.handle,
                           GetCurrentProcess(), &h, access, FALSE, 0))
        return 4;
      ++duplicates;
      OVERLAPPED io = {};
      io.Internal = 0x103;
      unsigned char output[8];
      memset(output, 0xA5, 8);
      LONG status = query(h, reinterpret_cast<Io *>(&io), output, 8, 4);
      ++calls;
      DWORD count = 0, error = 0;
      BOOL complete = FALSE;
      unsigned polls = 0;
      do {
        complete = GetOverlappedResult(h, &io, &count, FALSE);
        error = complete ? 0 : GetLastError();
        if (error == ERROR_IO_INCOMPLETE)
          Sleep(1);
        ++polls;
      } while (error == ERROR_IO_INCOMPLETE && polls < 10000);
      bool ok = (status == 0 || status == 0x103) && complete && count == 8 &&
                io.Internal == 0 && io.InternalHigh == 8;
      failures += !ok;
      printf("role=%s access=%08lX status=%08lX complete=%u error=%lu "
             "bytes=%lu polls=%u internal=%llX information=%llu ok=%u out=",
             role.name, access, static_cast<unsigned long>(status),
             static_cast<unsigned>(complete), error, count, polls,
             static_cast<unsigned long long>(io.Internal),
             static_cast<unsigned long long>(io.InternalHigh), ok);
      for (auto c : output)
        printf("%02X", static_cast<unsigned>(c));
      printf("\n");
      if (!CloseHandle(h))
        return 5;
      ++closes;
    }
  for (const auto &role : roles)
    if (role.owned && !CloseHandle(role.handle))
      return 6;
  bool removed = GetFileAttributesW(L"private-volume-async.dat") ==
                     INVALID_FILE_ATTRIBUTES &&
                 GetLastError() == ERROR_FILE_NOT_FOUND;
  failures += !removed;
  printf("summary calls=%u duplicates=%u closes=%u original-owned=3 "
         "original-closed=3 temporary-removed=%u failures=%u\n",
         calls, duplicates, closes, removed, failures);
  return failures || duplicates != closes ? 1 : 0;
}
