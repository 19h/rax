#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
#include <winternl.h>

using SetInformation = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG);
struct Information {
  void *table;
  unsigned char remove;
};
SetInformation service;
unsigned ordinal;

void call(const char *label, HANDLE process, void *table,
          unsigned char remove) {
  Information info{};
  info.table = table;
  info.remove = remove;
  unsigned char before[sizeof info];
  memcpy(before, &info, sizeof info);
  LONG status = LONG(0xcccccccc);
  DWORD exception = 0;
  SetLastError(0x4a2e19f7);
  __try {
    status = service(process, 53, &info, sizeof info);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    exception = GetExceptionCode();
  }
  DWORD error = GetLastError();
  printf(
      "case ordinal=%u label=%s status=%08lX exception=%08lX last-error=%08lX "
      "input-unchanged=%u\n",
      ordinal++, label, static_cast<unsigned long>(status), exception, error,
      memcmp(before, &info, sizeof info) == 0);
}

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 2;
  service = reinterpret_cast<SetInformation>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtSetInformationProcess"));
  if (!service)
    return 3;
  alignas(16) unsigned char a[256]{}, b[256]{};
  auto self = GetCurrentProcess();
  printf("identity profile=%s width=%zu\n", argv[1], sizeof(void *));
  call("add-A", self, a, 0);
  call("duplicate-A", self, a, 0);
  call("remove-A-first", self, a, 1);
  call("remove-A-second", self, a, 1);
  call("remove-A-third", self, a, 1);
  call("remove-B-never-added", self, b, 1);
  call("add-B", self, b, 0);
  call("remove-B-255", self, b, 255);
  call("remove-B-2", self, b, 2);
  call("add-A-again", self, a, 0);
  memset(a, 0xa5, sizeof a);
  call("remove-A-mutated", self, a, 1);
  memset(a, 0, sizeof a);
  struct HandleCase {
    const char *name;
    HANDLE handle;
  } handles[] = {
      {"null", nullptr},
      {"current-thread", GetCurrentThread()},
      {"self-zero-rights", OpenProcess(0, FALSE, GetCurrentProcessId())},
      {"self-set-information",
       OpenProcess(PROCESS_SET_INFORMATION, FALSE, GetCurrentProcessId())},
      {"self-query-limited", OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,
                                         FALSE, GetCurrentProcessId())}};
  for (const auto &entry : handles) {
    printf("handle name=%s available=%u\n", entry.name,
           entry.handle != nullptr);
    call(entry.name, entry.handle, a, 0);
    call("cleanup-self", self, a, 1);
  }
  for (const auto &entry : handles)
    if (entry.handle && entry.handle != GetCurrentThread())
      CloseHandle(entry.handle);
  bool a_unchanged = true, b_unchanged = true;
  for (auto value : a)
    a_unchanged &= value == 0;
  for (auto value : b)
    b_unchanged &= value == 0;
  printf("complete cases=%u A-unchanged=%u B-unchanged=%u\n", ordinal,
         a_unchanged, b_unchanged);
  return 0;
}
