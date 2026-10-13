#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>

using Query = LONG(NTAPI *)(HANDLE, const void *, ULONG, void *, SIZE_T,
                            SIZE_T *);
static Query query;
static LONG invoke(HANDLE process, const void *base, void *output,
                   SIZE_T length, SIZE_T *returned, ULONG *exception) {
  __try {
    return query(process, base, 14, output, length, returned);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
    return LONG(*exception);
  }
}
static ULONG image_size(HMODULE module) {
  auto bytes = reinterpret_cast<const unsigned char *>(module);
  auto dos = reinterpret_cast<const IMAGE_DOS_HEADER *>(bytes);
  auto nt = reinterpret_cast<const IMAGE_NT_HEADERS *>(bytes + dos->e_lfanew);
  return nt->OptionalHeader.SizeOfImage;
}
static void bytes(const unsigned char *buffer, unsigned length) {
  for (unsigned i = 0; i < length; ++i)
    printf("%02X", unsigned(buffer[i]));
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto ntdll = GetModuleHandleW(L"ntdll.dll");
  auto main = GetModuleHandleW(nullptr);
  query =
      reinterpret_cast<Query>(GetProcAddress(ntdll, "NtQueryVirtualMemory"));
  auto memory = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 16384, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  auto section = CreateFileMappingW(INVALID_HANDLE_VALUE, nullptr,
                                    PAGE_READWRITE, 0, 4096, nullptr);
  auto mapped = MapViewOfFile(section, FILE_MAP_WRITE, 0, 0, 4096);
  if (!query || !memory || !section || !mapped)
    return 20;
  printf("host pointer=%zu ntdll=%p main=%p ntdll-size=%lu main-size=%lu "
         "private=%p mapped=%p\n",
         sizeof(void *), ntdll, main, image_size(ntdll), image_size(main),
         memory, mapped);
  alignas(16) unsigned char output[128];
  memset(output, 0, sizeof(output));
  SIZE_T returned = ~SIZE_T(0);
  auto status = query(GetCurrentProcess(), ntdll, 6, output,
                      sizeof(void *) == 4 ? 12 : 24, &returned);
  printf("image-info status=%08lX returned=%zu bytes=", status, returned);
  bytes(output, sizeof(void *) == 4 ? 12 : 24);
  puts("");
  unsigned ordinal = 0;
  for (unsigned target = 0; target < 10; ++target)
    for (ULONG type : {0u, 1u, 2u, 3u, 0x80000000u, 0xffffffffu})
      for (ULONG flags : {0u, 1u, 2u, 3u, 0xffffffffu})
        for (SIZE_T length : {0u, 1u, 4u, 8u, 12u, 15u, 16u, 17u, 20u, 23u, 24u,
                              25u, 32u, 64u}) {
          const void *address = nullptr;
          switch (target) {
          case 0:
            address = ntdll;
            break;
          case 1:
            address = main;
            break;
          case 2:
            address = reinterpret_cast<unsigned char *>(ntdll) + 1;
            break;
          case 3:
            address =
                reinterpret_cast<unsigned char *>(ntdll) + image_size(ntdll);
            break;
          case 4:
            address = memory + 8192;
            break;
          case 5:
            address = nullptr;
            break;
          case 6:
            address = reinterpret_cast<void *>(uintptr_t(1));
            break;
          case 7:
            address = reinterpret_cast<void *>(~uintptr_t(0));
            break;
          case 8:
            address =
                reinterpret_cast<unsigned char *>(main) + image_size(main) - 1;
            break;
          case 9:
            address = mapped;
            break;
          }
          memset(output, 0xa5, sizeof(output));
          memcpy(output, &type, 4);
          memcpy(output + 4, &flags, 4);
          returned = ~SIZE_T(0);
          ULONG exception = 0;
          status = invoke(GetCurrentProcess(), address, output, length,
                          &returned, &exception);
          printf("case=%u target=%u type=%08lX flags=%08lX length=%zu "
                 "status=%08lX exception=%08lX returned=%llX bytes=",
                 ordinal++, target, type, flags, length, status, exception,
                 static_cast<unsigned long long>(returned));
          bytes(output, 64);
          puts("");
        }
  if (!UnmapViewOfFile(mapped) || !CloseHandle(section) ||
      !VirtualFree(memory, 0, MEM_RELEASE))
    return 21;
  printf("complete cases=%u\n", ordinal);
  return 0;
}
