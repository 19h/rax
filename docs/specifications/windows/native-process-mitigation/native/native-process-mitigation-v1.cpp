#define WIN32_LEAN_AND_MEAN
#include <array>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>

using Query = NTSTATUS(NTAPI *)(HANDLE, PROCESSINFOCLASS, PVOID, ULONG, PULONG);
struct Result {
  unsigned long status, exception;
};
static Result invoke(Query query, HANDLE process, void *output, ULONG length,
                     PULONG returned) {
  __try {
    return {static_cast<unsigned long>(query(process,
                                             static_cast<PROCESSINFOCLASS>(52),
                                             output, length, returned)),
            0};
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    return {0xDEADBEEF, GetExceptionCode()};
  }
}
int main(int argc, char **argv) {
  auto query = reinterpret_cast<Query>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtQueryInformationProcess"));
  if (!query)
    return 2;
  DWORD before = 0, after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 3;
  std::printf("PROFILE %s POINTER_BYTES %zu\n", argc > 1 ? argv[1] : "unknown",
              sizeof(void *));
  unsigned cases = 0;
  for (ULONG policy = 0; policy < 32; ++policy) {
    for (ULONG length : {0UL, 1UL, 4UL, 7UL, 8UL, 9UL, 16UL}) {
      for (unsigned use_return = 0; use_return < 2; ++use_return) {
        alignas(16) std::array<BYTE, 64> data;
        data.fill(0xA5);
        std::memcpy(data.data(), &policy, 4);
        ULONG returned = 0xBBBBBBBB;
        auto result = invoke(query, GetCurrentProcess(), data.data(), length,
                             use_return ? &returned : nullptr);
        std::printf("QUERY policy=%lu length=%lu return-pointer=%u "
                    "status=%08lX exception=%08lX returned=%08lX data=",
                    policy, length, use_return, result.status, result.exception,
                    returned);
        for (BYTE b : data)
          std::printf("%02X", unsigned(b));
        std::printf("\n");
        ++cases;
      }
    }
  }
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 4;
  std::printf("SUMMARY cases=%u handles-before=%lu handles-after=%lu\n", cases,
              before, after);
  return before == after ? 0 : 5;
}
