#define WIN32_LEAN_AND_MEAN
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
#include <winternl.h>

using Init = std::uintptr_t(WINAPI *)(void);
static Init init;
static void hex(const unsigned char *p, std::size_t n) {
  for (std::size_t i = 0; i < n; ++i)
    std::printf("%02x", p[i]);
}
static bool dump(const char *name, const void *pointer, std::size_t n) {
  MEMORY_BASIC_INFORMATION region{};
  auto p = reinterpret_cast<std::uintptr_t>(pointer);
  if (VirtualQuery(pointer, &region, sizeof(region)) != sizeof(region) ||
      region.State != MEM_COMMIT ||
      (region.Protect & (PAGE_NOACCESS | PAGE_GUARD)) ||
      p < reinterpret_cast<std::uintptr_t>(region.BaseAddress) ||
      n > region.RegionSize -
              (p - reinterpret_cast<std::uintptr_t>(region.BaseAddress)))
    return false;
  std::printf(
      "memory=%s pointer=%p allocation=%p protect=%08lx type=%08lx bytes=",
      name, pointer, region.AllocationBase, region.Protect, region.Type);
  hex(static_cast<const unsigned char *>(pointer), n);
  std::putchar('\n');
  return true;
}
static bool peb(const char *phase) {
  PROCESS_BASIC_INFORMATION info{};
  ULONG returned = 0;
  auto query = reinterpret_cast<NTSTATUS(NTAPI *)(HANDLE, PROCESSINFOCLASS,
                                                  PVOID, ULONG, PULONG)>(
      GetProcAddress(GetModuleHandleW(L"ntdll.dll"),
                     "NtQueryInformationProcess"));
  if (!query)
    return false;
  auto status = query(GetCurrentProcess(), ProcessBasicInformation, &info,
                      sizeof(info), &returned);
  std::printf("peb-phase=%s status=%08lx returned=%lu\n", phase,
              static_cast<unsigned long>(status), returned);
  return status >= 0 && dump(phase, info.PebBaseAddress, 512);
}
static void calls(const char *phase) {
  for (unsigned i = 0; i < 8; ++i) {
    SetLastError(0x12345678);
    std::printf("before-call=%s index=%u\n", phase, i);
    auto result = init();
    auto error = GetLastError();
    std::printf("call=%s index=%u tid=%lu return=%016llx error=%08lx\n", phase,
                i, GetCurrentThreadId(),
                static_cast<unsigned long long>(result), error);
  }
}
static DWORD WINAPI worker(void *) {
  calls("worker");
  return 0;
}
static LONG WINAPI observe_exception(EXCEPTION_POINTERS *p) {
  std::printf(
      "exception-code=%08lx address=%p flags=%08lx arguments=%lu\n",
      p->ExceptionRecord->ExceptionCode, p->ExceptionRecord->ExceptionAddress,
      p->ExceptionRecord->ExceptionFlags, p->ExceptionRecord->NumberParameters);
  return EXCEPTION_CONTINUE_SEARCH;
}
int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  AddVectoredExceptionHandler(1, observe_exception);
  if (argc != 2)
    return 2;
  std::printf("profile=%s pointer-bytes=%zu pid=%lu tid=%lu\n", argv[1],
              sizeof(void *), GetCurrentProcessId(), GetCurrentThreadId());
  auto module = LoadLibraryW(L"win32u.dll");
  if (!module)
    return 3;
  init = reinterpret_cast<Init>(GetProcAddress(module, "NtGdiInit2"));
  if (!init || !dump("NtGdiInit2", reinterpret_cast<void *>(init), 64) ||
      !peb("before"))
    return 4;
#if defined(_M_IX86)
  auto bytes = reinterpret_cast<const unsigned char *>(init);
  if (bytes[0] != 0xb8 || bytes[5] != 0xba || bytes[10] != 0xff ||
      bytes[11] != 0xd2 || bytes[12] != 0xc3)
    return 5;
  std::uint32_t thunk = 0;
  std::memcpy(&thunk, bytes + 6, sizeof(thunk));
  if (!dump("x86-transition",
            reinterpret_cast<void *>(static_cast<std::uintptr_t>(thunk)), 64))
    return 6;
#endif
  calls("main-first");
  if (!peb("after-first"))
    return 7;
  auto thread = CreateThread(nullptr, 0, worker, nullptr, 0, nullptr);
  if (!thread || WaitForSingleObject(thread, 30000) != WAIT_OBJECT_0 ||
      !CloseHandle(thread))
    return 8;
  calls("main-after-worker");
  auto gdi = LoadLibraryW(L"gdi32full.dll");
  std::printf("gdi-load=%d error=%08lx\n", gdi != nullptr, GetLastError());
  if (!gdi)
    return 9;
  calls("main-after-gdi");
  if (!peb("after-gdi") || !FreeLibrary(gdi) || !FreeLibrary(module))
    return 10;
  std::puts("closed-module-references=2 closed-threads=1 calls=32");
  return 0;
}
