#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <vector>
#include <windows.h>
#include <winternl.h>
using Open = LONG(NTAPI *)(HANDLE, ACCESS_MASK, HANDLE *);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using Close = LONG(NTAPI *)(HANDLE);
struct Entry {
  HANDLE handle;
  uintptr_t handles, pointers;
  ULONG access, type, attributes, reserved;
};
static Open open_token;
static Query query_process, query_token;
static Close close_handle;
static unsigned ordinal;
static void fail(const char *where) {
  printf("failure=%s error=%lu\n", where, GetLastError());
  ExitProcess(31);
}
static Entry entry(HANDLE handle) {
  std::vector<uintptr_t> buffer(32768);
  ULONG n = 0;
  if (query_process(GetCurrentProcess(), 51, buffer.data(),
                    ULONG(buffer.size() * sizeof(uintptr_t)), &n) < 0 ||
      buffer[0] > 8192)
    fail("handle-snapshot");
  auto first = reinterpret_cast<Entry *>(buffer.data() + 2);
  for (uintptr_t i = 0; i < buffer[0]; ++i)
    if (first[i].handle == handle)
      return first[i];
  fail("missing-handle");
  return {};
}
static void describe(const char *label, HANDLE handle) {
  auto e = entry(handle);
  ULONG type = 0, n = 0;
  LONG s = query_token(handle, 8, &type, sizeof(type), &n);
  printf(
      "handle label=%s value=%llX count=%llu pointers=%llu granted=%08lX "
      "attributes=%08lX token-type-status=%08lX token-type=%lu returned=%lu\n",
      label, static_cast<unsigned long long>(uintptr_t(handle)),
      static_cast<unsigned long long>(e.handles),
      static_cast<unsigned long long>(e.pointers), e.access, e.attributes,
      ULONG(s), type, n);
}
static HANDLE one(const char *label, HANDLE process, ULONG access) {
  HANDLE result = reinterpret_cast<HANDLE>(uintptr_t(0xa5a5a5a5));
  SetLastError(0x4a2e19f7);
  LONG s = open_token(process, access, &result);
  DWORD last = GetLastError();
  printf("open ordinal=%u label=%s requested=%08lX status=%08lX "
         "last-error=%08lX output=%llX\n",
         ordinal++, label, access, ULONG(s), last,
         static_cast<unsigned long long>(uintptr_t(result)));
  if (s >= 0) {
    describe(label, result);
    return result;
  }
  return nullptr;
}
static void close(const char *label, HANDLE handle) {
  SetLastError(0x4a2e19f7);
  LONG s = close_handle(handle);
  DWORD last = GetLastError();
  printf("close ordinal=%u label=%s status=%08lX last-error=%08lX\n", ordinal++,
         label, ULONG(s), last);
  if (s < 0)
    fail("close");
}
int main(int argc, char **argv) {
  if (argc != 2)
    return 2;
  HMODULE n = GetModuleHandleW(L"ntdll.dll");
  open_token = reinterpret_cast<Open>(GetProcAddress(n, "NtOpenProcessToken"));
  query_process =
      reinterpret_cast<Query>(GetProcAddress(n, "NtQueryInformationProcess"));
  query_token =
      reinterpret_cast<Query>(GetProcAddress(n, "NtQueryInformationToken"));
  close_handle = reinterpret_cast<Close>(GetProcAddress(n, "NtClose"));
  if (!open_token || !query_process || !query_token || !close_handle)
    fail("exports");
  printf("identity profile=%s width=%zu\n", argv[1], sizeof(void *));
  HANDLE first = one("first", GetCurrentProcess(), 8);
  if (!first)
    fail("first-token");
  std::vector<HANDLE> live;
  for (ULONG access : {1u, 2u, 8u, 0x20000u, 0x2000000u, 0x10000000u}) {
    HANDLE h = one("parallel", GetCurrentProcess(), access);
    if (!h)
      fail("parallel-token");
    live.push_back(h);
    describe("first-after-open", first);
  }
  close("first", first);
  for (HANDLE h : live)
    describe("after-first-close", h);
  HANDLE process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE,
                               GetCurrentProcessId());
  if (!process)
    fail("own-process");
  for (uintptr_t low = 0; low < 4; ++low) {
    HANDLE h = one("process-low-alias",
                   reinterpret_cast<HANDLE>(uintptr_t(process) | low), 8);
    if (h)
      close("alias-token", h);
  }
  if (sizeof(uintptr_t) == 8) {
    for (uint64_t high : {UINT64_C(0x100000000), UINT64_C(0xffffffff00000000),
                          UINT64_C(0x8000000000000000)}) {
      HANDLE h = one(
          "process-high-alias",
          reinterpret_cast<HANDLE>(uintptr_t(process) | uintptr_t(high)), 8);
      if (h)
        close("high-token", h);
    }
  }
  if (!CloseHandle(process))
    fail("close-process");
  HANDLE duplicate = nullptr;
  if (!DuplicateHandle(GetCurrentProcess(), live[2], GetCurrentProcess(),
                       &duplicate, 0, FALSE, DUPLICATE_SAME_ACCESS))
    fail("duplicate-token");
  describe("duplicate", duplicate);
  describe("original-with-duplicate", live[2]);
  close("original", live[2]);
  live[2] = nullptr;
  describe("duplicate-after-close", duplicate);
  close("tagged-duplicate", reinterpret_cast<HANDLE>(uintptr_t(duplicate) | 3));
  for (HANDLE h : live)
    if (h)
      close("remaining", h);
  HANDLE again = one("after-last-close", GetCurrentProcess(), 8);
  if (!again)
    fail("reopen");
  close("last", again);
  printf("complete operations=%u\n", ordinal);
}
