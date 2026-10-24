#define WIN32_LEAN_AND_MEAN
#include <cstdint>
#include <cstdio>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>

using Register = ULONG_PTR(NTAPI *)(DWORD);
using Status = LONG(NTAPI *)();

static int observe(const char *phase) {
  HMODULE user = LoadLibraryW(L"user32.dll");
  HMODULE module = LoadLibraryW(L"win32u.dll");
  HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
  if (!user || !module || !ntdll) {
    return 2;
  }
  auto call =
      reinterpret_cast<Register>(GetProcAddress(module, "NtUserRegisterLPK"));
  auto status =
      reinterpret_cast<Status>(GetProcAddress(ntdll, "RtlGetLastNtStatus"));
  if (!call || !status) {
    return 3;
  }
  std::printf("STUB phase=%s bytes=", phase);
  auto leaf = reinterpret_cast<const unsigned char *>(call);
  for (unsigned i = 0; i < 32; ++i) {
    std::printf("%02X", static_cast<unsigned>(leaf[i]));
  }
  std::putchar('\n');
  const DWORD arguments[] = {0,          1,          2,          3,
                             0x10,       0x100,      0x10000,    0x10000000,
                             0x40000000, 0x7FFFFFFF, 0x80000000, 0x80000001,
                             0xFFFFFFFE, 0xFFFFFFFF, 0x13572468, 0};
  for (unsigned sample = 0; sample < 3; ++sample) {
    for (DWORD argument : arguments) {
      SetLastError(0x12345678);
      LONG before = status();
      ULONG_PTR value = call(argument);
      DWORD error = GetLastError();
      LONG after = status();
      std::printf("REGISTER phase=%s sample=%u argument=%08lX value=%llu "
                  "last_error=%08lX nt-before=%08lX nt-after=%08lX\n",
                  phase, sample, argument,
                  static_cast<unsigned long long>(value), error,
                  static_cast<ULONG>(before), static_cast<ULONG>(after));
    }
  }
  FreeLibrary(module);
  FreeLibrary(user);
  return 0;
}

static DWORD WINAPI worker(void *) {
  return static_cast<DWORD>(observe("worker"));
}

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  std::printf("PROFILE %s pointer_bytes=%zu\n", argc > 1 ? argv[1] : "unknown",
              sizeof(void *));
  int code = observe("main");
  if (code != 0) {
    return code;
  }
  HANDLE thread = CreateThread(nullptr, 0, worker, nullptr, 0, nullptr);
  if (!thread) {
    return 4;
  }
  DWORD wait = WaitForSingleObject(thread, 30000);
  DWORD result = 5;
  if (wait == WAIT_OBJECT_0) {
    GetExitCodeThread(thread, &result);
  }
  CloseHandle(thread);
  return static_cast<int>(result);
}
