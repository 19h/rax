#define WIN32_LEAN_AND_MEAN
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>

int main(int argc, char **argv) {
  if (argc != 2)
    return 2;
  std::printf("profile=%s pointer-bytes=%zu pid=%lu tid=%lu\n", argv[1],
              sizeof(void *), GetCurrentProcessId(), GetCurrentThreadId());
  HMODULE module = LoadLibraryW(L"win32u.dll");
  if (!module) {
    std::printf("load-error=%lu\n", GetLastError());
    return 3;
  }
  wchar_t path[32768]{};
  DWORD length = GetModuleFileNameW(module, path, 32768);
  if (!length || length >= 32768)
    return 4;
  std::printf("module-base=%p path=%ls\n", static_cast<void *>(module), path);
  for (const char *name : {"NtGdiInit", "NtGdiInit2", "NtGdiInitSpool"}) {
    auto function = GetProcAddress(module, name);
    if (!function) {
      std::printf("export=%s error=%lu\n", name, GetLastError());
      return 5;
    }
    const auto *bytes = reinterpret_cast<const unsigned char *>(function);
    MEMORY_BASIC_INFORMATION region{};
    if (VirtualQuery(bytes, &region, sizeof(region)) != sizeof(region) ||
        region.State != MEM_COMMIT ||
        reinterpret_cast<std::uintptr_t>(bytes) + 64 >
            reinterpret_cast<std::uintptr_t>(region.BaseAddress) +
                region.RegionSize)
      return 6;
    std::printf(
        "export=%s pointer=%p allocation=%p protect=%08lx type=%08lx bytes=",
        name, reinterpret_cast<void *>(function), region.AllocationBase,
        region.Protect, region.Type);
    for (std::size_t i = 0; i < 64; ++i)
      std::printf("%02x", bytes[i]);
    std::putchar('\n');
  }
  if (!FreeLibrary(module))
    return 7;
  std::puts("closed=1 invoked-services=0");
  return 0;
}
