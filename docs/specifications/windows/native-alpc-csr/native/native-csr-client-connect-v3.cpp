#define WIN32_LEAN_AND_MEAN
#include <array>
#include <cstdio>
#include <cstring>
#include <windows.h>
#include <winternl.h>
using Connect = NTSTATUS(NTAPI *)(PWSTR, ULONG, PVOID, PULONG, PBOOLEAN);
struct Result {
  ULONG status, exception;
};
static Result invoke(Connect fn, PWSTR directory, ULONG server, void *data,
                     PULONG length, PBOOLEAN inside) {
  __try {
    return {static_cast<ULONG>(fn(directory, server, data, length, inside)), 0};
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    return {0xDEADBEEF, GetExceptionCode()};
  }
}
int main(int argc, char **argv) {
  auto module = GetModuleHandleW(L"ntdll.dll");
  auto connect = reinterpret_cast<Connect>(
      GetProcAddress(module, "CsrClientConnectToServer"));
  if (!connect)
    return 2;
  DWORD before = 0, after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 3;
  std::printf("PROFILE %s POINTER_BYTES %zu\n", argc > 1 ? argv[1] : "unknown",
              sizeof(void *));
  unsigned calls = 0;
  wchar_t directory[] = L"\\Windows";
  using ProcessId = ULONG(NTAPI *)();
  using Allocate = void *(NTAPI *)(ULONG, ULONG);
  using Free = void(NTAPI *)(void *);
  auto process_id =
      reinterpret_cast<ProcessId>(GetProcAddress(module, "CsrGetProcessId"));
  auto allocate = reinterpret_cast<Allocate>(
      GetProcAddress(module, "CsrAllocateCaptureBuffer"));
  auto release =
      reinterpret_cast<Free>(GetProcAddress(module, "CsrFreeCaptureBuffer"));
  std::printf("BOOTSTRAP before-pid-available=%u before-pid=%lu\n",
              unsigned(process_id != nullptr), process_id ? process_id() : 0);
  BOOLEAN boot_inside = 0xA5;
  auto boot = invoke(connect, directory, 0, nullptr, nullptr, &boot_inside);
  std::printf(
      "BOOTSTRAP status=%08lX exception=%08lX inside=%u after-pid=%lu\n",
      boot.status, boot.exception, unsigned(boot_inside),
      process_id ? process_id() : 0);
  if (allocate && release) {
    void *capture = allocate(1, 584);
    std::printf("ALLOCATE requested-pointers=1 requested-data=584 ok=%u\n",
                unsigned(capture != nullptr));
    if (capture)
      release(capture);
  }

  // The observed guest operation has ServerDllIndex3 and584 zero input bytes.
  // It connects this disposable client only; no host handle/address is
  // supplied.
  for (unsigned sample = 0; sample < 2; ++sample) {
    alignas(16) std::array<BYTE, 1024> bytes{};
    std::memset(bytes.data() + 584, 0xA5, bytes.size() - 584);
    ULONG length = 584;
    BOOLEAN inside = 0xA5;
    auto result = invoke(connect, directory, 3, bytes.data(), &length, &inside);
    std::printf("CONNECT sample=%u server=3 supplied=584 status=%08lX "
                "exception=%08lX returned=%lu inside=%u data=",
                sample, result.status, result.exception, length,
                unsigned(inside));
    for (BYTE b : bytes)
      std::printf("%02X", unsigned(b));
    std::printf("\n");
    ++calls;
  }
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 4;
  std::printf("SUMMARY calls=%u handles-before=%lu handles-after=%lu\n", calls,
              before, after);
  // NTDLL may retain its newly initialized CSR connection until process exit.
  // No returned NTDLL-owned port is closed by this probe. Counts remain raw.
  return 0;
}
