#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cwchar>
#include <initializer_list>
#include <windows.h>
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
static LONG invoke(Query query, HANDLE token, void *out, ULONG bytes,
                   ULONG *ret, ULONG *escaped) {
  __try {
    return query(token, 44, out, bytes, ret);
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 3)
    return 21;
  auto query = reinterpret_cast<Query>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtQueryInformationToken"));
  HANDLE token = nullptr;
  if (!query || !OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token))
    return 22;
  alignas(16) BYTE reference[1024];
  memset(reference, 0xA5, sizeof reference);
  ULONG required = 0, escaped = 0;
  LONG status =
      invoke(query, token, reference, sizeof reference, &required, &escaped);
  if (status != 0 || escaped || required < sizeof(void *) * 2 || required > 512)
    return 23;
  auto info = reinterpret_cast<TOKEN_BNO_ISOLATION_INFORMATION *>(reference);
  printf("copy-reference profile=%s pointer-bytes=%zu required=%lu enabled=%u "
         "prefix-units=%zu\n",
         argv[1], sizeof(void *), required, unsigned(info->IsolationEnabled),
         info->IsolationPrefix ? wcslen(info->IsolationPrefix) : 0);
  SYSTEM_INFO system{};
  GetSystemInfo(&system);
  const SIZE_T page = system.dwPageSize;
  auto arena = static_cast<BYTE *>(VirtualAlloc(
      nullptr, page * 2, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!arena)
    return 24;
  const DWORD protections[] = {PAGE_NOACCESS, PAGE_READONLY,
                               PAGE_READWRITE | PAGE_GUARD,
                               PAGE_READONLY | PAGE_GUARD};
  ULONG ordinal = 0;
  for (ULONG remaining = 0; remaining <= required + 32; ++remaining) {
    for (DWORD protection : protections) {
      for (ULONG length : {required, required + 16, 0xFFFFFFFFul}) {
        DWORD old = 0;
        if (!VirtualProtect(arena, page * 2, PAGE_READWRITE, &old))
          return 25;
        memset(arena, 0xA5, page * 2);
        if (!VirtualProtect(arena + page, page, protection, &old))
          return 26;
        BYTE *output = arena + page - remaining;
        ULONG returned = 0xA5A5A5A5, exception = 0;
        SetLastError(0x7E11ABCD);
        status = invoke(query, token, output, length, &returned, &exception);
        DWORD error = GetLastError();
        MEMORY_BASIC_INFORMATION after{};
        if (!VirtualQuery(arena + page, &after, sizeof after))
          return 27;
        printf("copy-case ordinal=%lu remaining=%lu protection=%08lX "
               "after=%08lX length=%08lX status=%08lX returned=%08lX "
               "escaped=%08lX last-error=%08lX output=%llX prefix=",
               ordinal++, remaining, protection, after.Protect, length,
               ULONG(status), returned, exception, error,
               static_cast<unsigned long long>(
                   reinterpret_cast<uintptr_t>(output)));
        for (ULONG i = 0; i < remaining; ++i)
          printf("%02X", output[i]);
        puts("");
      }
    }
  }
  if (!CloseHandle(token) || !VirtualFree(arena, 0, MEM_RELEASE))
    return 28;
  printf("copy-complete cases=%lu closed-handles=1 released-arenas=1\n",
         ordinal);
  return 0;
}
