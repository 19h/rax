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

struct Table {
  uint64_t list[2], functions, timestamp, minimum, maximum, base;
};
struct Work {
  Table *table;
  unsigned char remove;
};
DWORD WINAPI worker(void *data) {
  auto w = static_cast<Work *>(data);
  call("worker", GetCurrentProcess(), w->table, w->remove);
  return 0;
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 2;
  service = reinterpret_cast<SetInformation>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtSetInformationProcess"));
  if (!service)
    return 3;
  auto self = GetCurrentProcess();
  auto memory =
      VirtualAlloc(nullptr, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
  if (!memory)
    return 4;
  Table a{};
  a.base = reinterpret_cast<uintptr_t>(memory);
  a.maximum = a.base + 4096;
  printf("identity profile=%s width=%zu\n", argv[1], sizeof(void *));
  call("add-range", self, &a, 0);
  if (!VirtualFree(memory, 0, MEM_RELEASE))
    return 5;
  call("add-range-after-unmap", self, &a, 0);
  call("remove-range-after-unmap", self, &a, 1);
  call("add-range-after-remove", self, &a, 0);
  call("remove-range-again", self, &a, 1);
  auto descriptor = static_cast<Table *>(
      VirtualAlloc(nullptr, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE));
  if (!descriptor)
    return 6;
  descriptor->base = 0x40000000000ULL;
  descriptor->maximum = descriptor->base + 4096;
  Table copy = *descriptor;
  call("add-descriptor", self, descriptor, 0);
  if (!VirtualFree(descriptor, 0, MEM_RELEASE))
    return 7;
  call("remove-after-descriptor-free", self, &copy, 1);
  call("add-fresh-descriptor", self, &copy, 0);
  call("remove-fresh-descriptor", self, &copy, 1);
  Work w{&copy, 0};
  auto thread = CreateThread(nullptr, 0, worker, &w, 0, nullptr);
  if (!thread)
    return 8;
  WaitForSingleObject(thread, INFINITE);
  CloseHandle(thread);
  call("parent-remove-worker-range", self, &copy, 1);
  call("parent-add-range", self, &copy, 0);
  w.remove = 1;
  thread = CreateThread(nullptr, 0, worker, &w, 0, nullptr);
  if (!thread)
    return 9;
  WaitForSingleObject(thread, INFINITE);
  CloseHandle(thread);
  call("parent-remove-after-worker", self, &copy, 1);
  printf("complete cases=%u\n", ordinal);
  return 0;
}
