#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>
#include <windows.h>

using Query = LONG(NTAPI *)(ULONG, PULONG, PVOID);

struct Pages {
  unsigned char *base = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 8192, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  Pages() {
    DWORD old = 0;
    if (!base || !VirtualProtect(base + 4096, 4096, PAGE_NOACCESS, &old))
      std::abort();
  }
  ~Pages() {
    if (!VirtualFree(base, 0, MEM_RELEASE))
      std::abort();
  }
  void protect(DWORD protection) {
    DWORD old = 0;
    if (!VirtualProtect(base, 4096, protection, &old))
      std::abort();
  }
};

int main() {
  auto module = GetModuleHandleW(L"ntdll.dll");
  auto query =
      reinterpret_cast<Query>(GetProcAddress(module, "NtGetMUIRegistryInfo"));
  if (!query)
    return 2;
  std::printf("architecture-bytes=%zu\n", sizeof(void *));
  ULONG needed = 0;
  LONG sizing = query(0, &needed, nullptr);
  if (sizing < 0 || needed == 0 || needed > 1048576)
    return 3;
  std::vector<unsigned char> snapshot(needed);
  LONG captured = query(0, &needed, snapshot.data());
  if (captured < 0 || needed != snapshot.size())
    return 4;
  FILE *file = nullptr;
  if (fopen_s(&file, "mui.bin", "wb") || !file)
    return 5;
  auto written = std::fwrite(snapshot.data(), 1, snapshot.size(), file);
  if (std::fclose(file) || written != snapshot.size())
    return 6;
  std::printf("snapshot status=%08lx bytes=%lu\n",
              static_cast<unsigned long>(captured), needed);
  Pages sizes, outputs;
  const ULONG lengths[] = {0,          1,      3,          4,    32,
                           needed - 1, needed, needed + 1, 8192, 0xffffffff};
  // Only query flags and a documented invalid bit. Never clear or commit the
  // host cache.
  for (ULONG flags : {0UL, 1UL, 4UL}) {
    for (unsigned size_kind = 0; size_kind != 7; ++size_kind) {
      for (unsigned output_kind = 0; output_kind != 8; ++output_kind) {
        for (ULONG length : lengths) {
          sizes.protect(PAGE_READWRITE);
          outputs.protect(PAGE_READWRITE);
          std::memset(sizes.base, 0xa5, 4096);
          std::memset(outputs.base, 0xa5, 4096);
          unsigned char *size_address = sizes.base + 128;
          if (size_kind == 1)
            size_address = nullptr;
          if (size_kind == 2)
            size_address = reinterpret_cast<unsigned char *>(1);
          if (size_kind == 3)
            size_address = sizes.base + 129;
          if (size_kind == 5)
            size_address = sizes.base + 4096;
          if (size_kind == 6)
            size_address = sizes.base + 4094;
          if (size_kind == 0 || size_kind == 3 || size_kind == 4)
            std::memcpy(size_address, &length, 4);
          if (size_kind == 6)
            std::memcpy(size_address, &length, 2);
          auto output = outputs.base + 64;
          if (output_kind == 1)
            output = nullptr;
          if (output_kind == 2)
            output = reinterpret_cast<unsigned char *>(1);
          if (output_kind == 3)
            output = outputs.base + 65;
          if (output_kind == 4)
            output = outputs.base + 4096;
          if (output_kind == 5)
            output = outputs.base + 4080;
          if (output_kind == 6)
            output = size_address;
          if (output_kind == 7)
            outputs.protect(PAGE_READONLY);
          if (size_kind == 4)
            sizes.protect(PAGE_READONLY);
          SetLastError(0x12345678);
          LONG status =
              query(flags, reinterpret_cast<PULONG>(size_address), output);
          DWORD error = GetLastError();
          ULONG after = 0;
          if (size_kind == 0 || size_kind == 3 || size_kind == 4)
            std::memcpy(&after, size_address, 4);
          if (size_kind == 6)
            std::memcpy(&after, size_address, 2);
          unsigned changed = 0;
          for (unsigned index = 0; index != 4096; ++index)
            changed += outputs.base[index] != 0xa5;
          std::printf("row flags=%lx size-kind=%u output-kind=%u length=%lu "
                      "status=%08lx size-after=%lu output-changed=%u "
                      "last-error=%08lx\n",
                      flags, size_kind, output_kind, length,
                      static_cast<unsigned long>(status), after, changed,
                      error);
        }
      }
    }
  }
  return 0;
}
