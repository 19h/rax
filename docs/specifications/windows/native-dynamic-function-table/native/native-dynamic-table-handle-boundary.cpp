#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <windows.h>
#include <winternl.h>

using SetInformation = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG);
struct Information {
  void *table;
  unsigned char remove;
};

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 3)
    return 2;
  unsigned mode = static_cast<unsigned>(strtoul(argv[2], nullptr, 10));
  if (mode > 14)
    return 3;
  auto set = reinterpret_cast<SetInformation>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtSetInformationProcess"));
  if (!set)
    return 4;
  alignas(16) unsigned char table[256]{};
  Information info{};
  info.table = mode >= 2 ? table : nullptr;
  info.remove = mode == 1 || mode == 3 ? 1 : mode == 4 ? 255 : 0;
  unsigned char before[sizeof info];
  memcpy(before, &info, sizeof info);
  HANDLE process = mode >= 7 ? reinterpret_cast<HANDLE>(uintptr_t(0xdeadbeec))
                             : GetCurrentProcess();
  if (mode >= 9) {
    const uint64_t handles[] = {0xffffffffULL,
                                0xffffffffffffffffULL,
                                0xfffffffffffffffeULL,
                                0xffffffffdeadbeefULL,
                                1ULL,
                                0ULL};
    process =
        reinterpret_cast<HANDLE>(static_cast<uintptr_t>(handles[mode - 9]));
  }
  void *input = mode == 5 || mode == 7 ? nullptr : &info;
  ULONG length = mode == 6 ? 0 : sizeof info;
  DWORD exception = 0;
  LONG status = LONG(0xcccccccc);
  SetLastError(0x4a2e19f7);
  __try {
    status = set(process, 53, input, length);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    exception = GetExceptionCode();
  }
  DWORD error = GetLastError();
  printf("profile=%s mode=%u width=%zu length=%lu status=%08lX exception=%08lX "
         "last-error=%08lX unchanged=%u\n",
         argv[1], mode, sizeof(void *), length,
         static_cast<unsigned long>(status), exception, error,
         memcmp(before, &info, sizeof info) == 0);
  return exception ? 1 : 0;
}
