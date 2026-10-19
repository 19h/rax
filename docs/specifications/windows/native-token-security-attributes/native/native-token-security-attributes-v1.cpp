#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cwchar>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>

using Query = LONG(NTAPI *)(HANDLE, const UNICODE_STRING *, ULONG, PVOID, ULONG,
                            PULONG);
static Query query_attributes;

static LONG invoke(HANDLE token, const UNICODE_STRING *names, ULONG count,
                   void *output, ULONG length, ULONG *required,
                   ULONG *exception) {
  LONG status = LONG(0xDEADBEEF);
  *exception = 0;
  __try {
    status = query_attributes(token, names, count, output, length, required);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
  }
  return status;
}

int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  query_attributes = reinterpret_cast<Query>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtQuerySecurityAttributesToken"));
  if (!query_attributes)
    return 2;
  HANDLE token = nullptr;
  if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token))
    return 3;
  DWORD initial_handles = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &initial_handles))
    return 4;
  const wchar_t *names[] = {L"", L"WIN://SYSAPPID", L"WIN://PKG",
                            L"WIN://BNOISOLATION", L"Assist.Probe.Absent"};
  unsigned ordinal = 0;
  for (unsigned mode = 0; mode <= 5; ++mode) {
    UNICODE_STRING name{};
    if (mode != 0) {
      name.Buffer = const_cast<wchar_t *>(names[mode - 1]);
      name.Length = USHORT(std::wcslen(name.Buffer) * sizeof(wchar_t));
      name.MaximumLength = name.Length + sizeof(wchar_t);
    }
    for (ULONG length : {0ul, 1ul, 4ul, 8ul, 11ul, 12ul, 15ul, 16ul, 32ul, 64ul,
                         128ul, 512ul, 4096ul}) {
      auto output = static_cast<unsigned char *>(VirtualAlloc(
          nullptr, 8192, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
      if (!output)
        return 5;
      memset(output, 0xA5, 8192);
      ULONG required = 0x11223344;
      ULONG exception = 0;
      SetLastError(0xC0DE);
      LONG status =
          invoke(token, mode == 0 ? nullptr : &name, mode == 0 ? 0 : 1, output,
                 length, &required, &exception);
      DWORD last_error = GetLastError();
      DWORD handles = 0;
      if (!GetProcessHandleCount(GetCurrentProcess(), &handles) ||
          handles != initial_handles)
        return 6;
      printf("case=%u mode=%u width=%zu length=%08lX status=%08lX "
             "exception=%08lX required=%08lX last-error=%08lX output=%p "
             "handles=%lu name=",
             ordinal++, mode, sizeof(void *), length, ULONG(status), exception,
             required, last_error, output, handles);
      for (unsigned i = 0; i < name.Length / sizeof(wchar_t); ++i)
        printf("%04X", unsigned(name.Buffer[i]));
      printf(" bytes=");
      for (unsigned i = 0; i < (length == 0 ? 32 : length); ++i)
        printf("%02X", output[i]);
      puts("");
      if (!VirtualFree(output, 0, MEM_RELEASE))
        return 7;
    }
  }
  if (!CloseHandle(token))
    return 8;
  printf("complete cases=%u\n", ordinal);
  return 0;
}
