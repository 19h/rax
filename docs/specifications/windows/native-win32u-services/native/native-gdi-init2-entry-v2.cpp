#define WIN32_LEAN_AND_MEAN
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
#include <winternl.h>
#if !defined(_M_ARM64)
#error This instruction observer requires native ARM64.
#endif
static unsigned char *entry;
static std::uint32_t original[2];
static unsigned events;
static unsigned char *peb;
static void snapshot(const char *name) {
  std::uintptr_t callback = 0, table = 0;
  std::memcpy(&callback, peb + 0x58, sizeof(callback));
  std::memcpy(&table, peb + 0xf8, sizeof(table));
  std::printf("phase=%s callback=%016llx table=%016llx\n", name,
              static_cast<unsigned long long>(callback),
              static_cast<unsigned long long>(table));
}
static bool patch(std::uint32_t first, std::uint32_t second) {
  DWORD old = 0, ignored = 0;
  if (!VirtualProtect(entry, 8, PAGE_EXECUTE_READWRITE, &old))
    return false;
  std::uint32_t words[2] = {first, second};
  std::memcpy(entry, words, 8);
  if (!FlushInstructionCache(GetCurrentProcess(), entry, 8))
    return false;
  return VirtualProtect(entry, 8, old, &ignored) != 0;
}
static LONG WINAPI observe(EXCEPTION_POINTERS *p) {
  if (p->ExceptionRecord->ExceptionCode != EXCEPTION_BREAKPOINT)
    return EXCEPTION_CONTINUE_SEARCH;
  auto pc =
      reinterpret_cast<std::uintptr_t>(p->ExceptionRecord->ExceptionAddress);
  if (!events && pc == reinterpret_cast<std::uintptr_t>(entry)) {
    snapshot("before-first-init2-svc");
    if (!patch(original[0], 0xd43e0000))
      return EXCEPTION_CONTINUE_SEARCH;
    p->ContextRecord->Pc = pc;
    ++events;
    return EXCEPTION_CONTINUE_EXECUTION;
  }
  if (events == 1 && pc == reinterpret_cast<std::uintptr_t>(entry + 4)) {
    snapshot("after-first-init2-svc");
    std::printf("service-return=%016llx\n",
                static_cast<unsigned long long>(p->ContextRecord->X0));
    if (!patch(original[0], original[1]))
      return EXCEPTION_CONTINUE_SEARCH;
    p->ContextRecord->Pc = pc;
    ++events;
    return EXCEPTION_CONTINUE_EXECUTION;
  }
  return EXCEPTION_CONTINUE_SEARCH;
}
int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 2;
  std::printf("profile=%s pid=%lu\n", argv[1], GetCurrentProcessId());
  auto query = reinterpret_cast<NTSTATUS(NTAPI *)(HANDLE, PROCESSINFOCLASS,
                                                  PVOID, ULONG, PULONG)>(
      GetProcAddress(GetModuleHandleW(L"ntdll.dll"),
                     "NtQueryInformationProcess"));
  PROCESS_BASIC_INFORMATION info{};
  ULONG length = 0;
  if (!query || query(GetCurrentProcess(), ProcessBasicInformation, &info,
                      sizeof(info), &length) < 0)
    return 3;
  peb = reinterpret_cast<unsigned char *>(info.PebBaseAddress);
  auto win32u = LoadLibraryW(L"win32u.dll");
  if (!win32u)
    return 4;
  entry =
      reinterpret_cast<unsigned char *>(GetProcAddress(win32u, "NtGdiInit2"));
  if (!entry)
    return 5;
  std::memcpy(original, entry, 8);
  if (original[0] != 0xd4025fa1 || original[1] != 0xd65f03c0)
    return 6;
  auto handler = AddVectoredExceptionHandler(1, observe);
  if (!handler || !patch(0xd43e0000, original[1]))
    return 7;
  snapshot("before-load-user32");
  auto user = LoadLibraryW(L"user32.dll");
  std::printf("user32-loaded=%d events=%u\n", user != nullptr, events);
  if (!user)
    return 8;
  snapshot("after-load-user32");
  auto fn = reinterpret_cast<std::uintptr_t(WINAPI *)(void)>(entry);
  for (unsigned n = 0; n < 4; ++n)
    std::printf("call=%u return=%016llx\n", n,
                static_cast<unsigned long long>(fn()));
  if (events != 2 || std::memcmp(entry, original, 8) ||
      !RemoveVectoredExceptionHandler(handler))
    return 9;
  if (!FreeLibrary(user) || !FreeLibrary(win32u))
    return 10;
  std::puts(
      "restored-instructions=1 closed-module-references=2 removed-observers=1");
  return 0;
}
