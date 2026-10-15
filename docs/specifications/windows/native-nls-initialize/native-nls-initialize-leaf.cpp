#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#include <initializer_list>
#include <stdio.h>

int main() {
  auto module = GetModuleHandleW(L"ntdll.dll");
  for (auto name :
       {"NtInitializeNlsFiles", "NtGetNlsSectionPtr", "NtUnmapViewOfSection"}) {
    auto address = GetProcAddress(module, name);
    if (!address)
      return 91;
    auto bytes = reinterpret_cast<const unsigned char *>(address);
    printf("leaf name=%s pointer-bytes=%zu hex=", name, sizeof(void *));
    for (unsigned index = 0; index < 32; ++index)
      printf("%02X", bytes[index]);
    printf("\n");
  }
  printf("complete leaves=3\n");
  return 0;
}
