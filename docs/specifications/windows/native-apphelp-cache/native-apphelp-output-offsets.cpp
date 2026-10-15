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
  if (!arena || !VirtualAlloc(arena, 8192, MEM_COMMIT, PAGE_READWRITE))
    fail("owned-arena");
  const WCHAR name[] = L"KERNELBASE.dll";
  unsigned begin = sizeof(void *) == 8 ? 208 : 104;
  unsigned end = sizeof(void *) == 8 ? 288 : 160;
  unsigned step = sizeof(void *);
  unsigned ordinal = 0;
  unsigned out_begin = sizeof(void *) == 8 ? 376 : 172;
  unsigned out_end = sizeof(void *) == 8 ? 376 : 224;
  for (unsigned out_offset = out_begin; out_offset <= out_end;
       out_offset += step) {
    for (unsigned offset = begin; offset <= end; offset += step) {
      memset(arena, 0xA5, 8192);
      unsigned char *input = arena + 128;
      memset(input, 0, 512);
      UNICODE_STRING string{};
      string.Length = sizeof(name) - sizeof(WCHAR);
      string.MaximumLength = sizeof(name);
      string.Buffer = reinterpret_cast<WCHAR *>(arena + 4096);
      memcpy(string.Buffer, name, sizeof(name));
      memcpy(input + offset, &string, sizeof string);
      uintptr_t output = uintptr_t(arena + 6144);
      ULONG output_size = 4;
      memcpy(input + out_offset, &output, sizeof output);
      memcpy(input + out_offset + sizeof(void *), &output_size, 4);
      unsigned char before[512];
      memcpy(before, input, 512);
      ULONG escaped = 0;
      LONG status = invoke(call, input, &escaped);
      printf("case=%u width=%zu offset=%u length=%u maximum=%u pointer=%llX "
             "status=%08lX escaped=%08lX changes=",
             ordinal++, sizeof(void *), offset, string.Length,
             string.MaximumLength,
             static_cast<unsigned long long>(uintptr_t(string.Buffer)),
             ULONG(status), escaped);
      for (unsigned i = 0; i < 512; ++i)
        if (before[i] != input[i])
          printf("%u:%02X>%02X,", i, before[i], input[i]);
      ULONG value = 0;
      memcpy(&value, arena + 6144, 4);
      printf(" out-offset=%u output=%08lX\n", out_offset, value);
    }
  }
  printf("complete cases=%u operation=6\n", ordinal);
  return 0;
}
