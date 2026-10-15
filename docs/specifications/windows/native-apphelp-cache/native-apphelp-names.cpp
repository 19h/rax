#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
#include <winternl.h>

using Lookup = LONG(NTAPI *)(ULONG, PVOID);

static void fail(const char *operation) {
  printf("failure=%s error=%lu\n", operation, GetLastError());
  ExitProcess(21);
}

static LONG invoke(Lookup call, void *input, ULONG *escaped) {
  __try {
    return call(6, input);
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}

int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto call = reinterpret_cast<Lookup>(
      GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "NtApphelpCacheControl"));
  if (!call)
    fail("lookup-export");
  auto arena = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 65536, MEM_RESERVE, PAGE_NOACCESS));
  if (!arena || !VirtualAlloc(arena, 12288, MEM_COMMIT, PAGE_READWRITE))
    fail("owned-arena");
  const WCHAR *names[] = {L"KERNELBASE.dll",
                          L"kernelbase.dll",
                          L"kernel32.dll",
                          L"ntdll.dll",
                          L"user32.dll",
                          L"apphelp.dll",
                          L"AcLayers.dll",
                          L"AcGenral.dll",
                          L"AcXtrnal.dll",
                          L"AcSpecfc.dll",
                          L"AcWinRT.dll",
                          L"msvcrt.dll",
                          L"ucrtbase.dll",
                          L"mscoree.dll",
                          L"ole32.dll",
                          L"shell32.dll",
                          L"foo.dll",
                          L"foo",
                          L"",
                          L".",
                          L"..",
                          L"\\??\\C:\\Windows\\System32\\KERNELBASE.dll",
                          L"C:\\Windows\\System32\\KERNELBASE.dll",
                          L"sysmain.sdb",
                          L"__COMPAT_LAYER"};
  unsigned name_offset = sizeof(void *) == 8 ? 248 : 132;
  unsigned out_offset = sizeof(void *) == 8 ? 376 : 196;
  unsigned ordinal = 0;
  for (unsigned index = 0; index < sizeof(names) / sizeof(names[0]); ++index) {
    memset(arena, 0xA5, 12288);
    auto input = arena + 128;
    memset(input, 0, 512);
    UNICODE_STRING name{};
    name.Length = USHORT(wcslen(names[index]) * sizeof(WCHAR));
    name.MaximumLength = name.Length + sizeof(WCHAR);
    name.Buffer = reinterpret_cast<WCHAR *>(arena + 4096);
    memcpy(name.Buffer, names[index], name.MaximumLength);
    memcpy(input + name_offset, &name, sizeof name);
    uintptr_t output = uintptr_t(arena + 8192);
    ULONG size = 4;
    memcpy(input + out_offset, &output, sizeof output);
    memcpy(input + out_offset + sizeof(void *), &size, 4);
    unsigned char before[512];
    memcpy(before, input, sizeof before);
    ULONG escaped = 0;
    LONG status = invoke(call, input, &escaped);
    ULONG value = 0;
    memcpy(&value, arena + 8192, 4);
    printf("case=%u name-index=%u width=%zu length=%u maximum=%u status=%08lX "
           "escaped=%08lX value=%08lX changes=",
           ordinal++, index, sizeof(void *), name.Length, name.MaximumLength,
           ULONG(status), escaped, value);
    for (unsigned i = 0; i < 512; ++i)
      if (before[i] != input[i])
        printf("%u:%02X>%02X,", i, before[i], input[i]);
    printf(" name-utf16=");
    for (unsigned i = 0; i < name.Length / 2; ++i)
      printf("%04X", unsigned(names[index][i]));
    puts("");
  }
  printf("complete cases=%u operation=6\n", ordinal);
  return 0;
}
