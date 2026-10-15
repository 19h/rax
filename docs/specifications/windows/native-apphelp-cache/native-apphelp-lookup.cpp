#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
#include <winternl.h>

using Lookup = LONG(NTAPI *)(ULONG, PVOID);

// Exact primary declarations from pinned PHNT ntmisc.h. AHC_INFO_CLASS is
// ULONG.
struct AhcLookupData {
  ULONG InfoClass;
  UINT HintFlags;
  UNICODE_STRING PackageAlias;
  HANDLE FileHandle;
  HANDLE ProcessHandle;
  USHORT ExeType;
  USHORT Padding;
  UNICODE_STRING ExeSignature;
  const WCHAR *Environment;
  UINT EnvironmentSize;
};
struct AhcUpdateData {
  ULONG InfoClass;
  UNICODE_STRING PackageAlias;
  HANDLE FileHandle;
  UNICODE_STRING ExeSignature;
  PVOID Data;
  ULONG DataSize;
};
struct AhcCacheData {
  HANDLE FileHandle;
  USHORT ExeType;
  USHORT Padding;
  UINT HintFlags;
  HANDLE ProcessHandle;
  UNICODE_STRING FileName;
  UNICODE_STRING Environment;
  UNICODE_STRING PackageAlias;
  ULONG CustomDataSize;
  BYTE *CustomData;
};
struct AhcCdbData {
  UNICODE_STRING Name;
};
struct AhcClearData {
  ULONG InfoClass;
};
struct AhcRemoveData {
  ULONG InfoClass;
  UNICODE_STRING PackageAlias;
  HANDLE FileHandle;
  UNICODE_STRING ExeSignature;
};
struct AhcHwIdData {
  BOOLEAN QueryResult;
  UNICODE_STRING HwId;
};
struct AhcServiceData {
  AhcLookupData Lookup;
  AhcUpdateData Update;
  AhcCacheData Cache;
  AhcCdbData LookupCdb;
  AhcClearData Clear;
  AhcRemoveData Remove;
  AhcHwIdData HwIdQuery;
  LONG DriverStatus;
  PVOID ParamsOut;
  ULONG ParamsOutSize;
};

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

static void dump(const char *phase, const void *input, SIZE_T size) {
  unsigned char bytes[512]{};
  SIZE_T got = 0;
  BOOL ok = ReadProcessMemory(GetCurrentProcess(), input, bytes, size, &got);
  printf("dump phase=%s address=%llX requested=%zu ok=%u got=%zu hex=", phase,
         static_cast<unsigned long long>(uintptr_t(input)), size, unsigned(ok),
         got);
  for (SIZE_T i = 0; i < got; ++i)
    printf("%02X", bytes[i]);
  puts("");
}

int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto call = reinterpret_cast<Lookup>(
      GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "NtApphelpCacheControl"));
  if (!call)
    fail("lookup-export");
  SYSTEM_INFO system{};
  GetSystemInfo(&system);
  SIZE_T page = system.dwPageSize;
  auto arena = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, page * 4, MEM_RESERVE, PAGE_NOACCESS));
  if (!arena || !VirtualAlloc(arena, page * 2, MEM_COMMIT, PAGE_READWRITE))
    fail("owned-arena");
  printf("layout pointer=%zu page=%zu size=%zu lookup=%zu update=%zu cache=%zu "
         "cdb=%zu remove=%zu hwid=%zu driver=%zu out=%zu out-size=%zu\n",
         sizeof(void *), page, sizeof(AhcServiceData), sizeof(AhcLookupData),
         sizeof(AhcUpdateData), sizeof(AhcCacheData),
         offsetof(AhcServiceData, LookupCdb), offsetof(AhcServiceData, Remove),
         offsetof(AhcServiceData, HwIdQuery),
         offsetof(AhcServiceData, DriverStatus),
         offsetof(AhcServiceData, ParamsOut),
         offsetof(AhcServiceData, ParamsOutSize));
  for (unsigned mode = 0; mode < 16; ++mode) {
    DWORD old = 0;
    VirtualProtect(arena, page * 2, PAGE_READWRITE, &old);
    memset(arena, 0xA5, page * 2);
    auto input = arena + 128;
    if (mode == 1)
      input = nullptr;
    if (mode == 2)
      input = reinterpret_cast<unsigned char *>(uintptr_t(1));
    if (mode == 3)
      input = arena + 129;
    if (mode == 4)
      input = arena + 132;
    if (mode == 5)
      input = arena + page * 2;
    if (mode == 6)
      input = arena + page * 2 - 8;
    if (mode == 7)
      input = arena + page * 2 - sizeof(AhcServiceData);
    if (mode >= 8 && mode <= 13)
      memset(input, 0, sizeof(AhcServiceData));
    if (mode == 14)
      memset(input, 0, sizeof(AhcServiceData));
    if (mode == 15)
      memset(input, 0xFF, sizeof(AhcServiceData));
    dump("before", input, 512);
    DWORD protection = mode == 8    ? PAGE_READONLY
                       : mode == 9  ? PAGE_READWRITE | PAGE_GUARD
                       : mode == 10 ? PAGE_READONLY | PAGE_GUARD
                       : mode == 11 ? PAGE_NOACCESS
                       : mode == 12 ? PAGE_EXECUTE_READ
                       : mode == 13 ? PAGE_EXECUTE_READWRITE
                                    : PAGE_READWRITE;
    if (mode >= 8 && mode <= 13 &&
        !VirtualProtect(arena, page, protection, &old))
      fail("input-protection");
    ULONG escaped = 0;
    LONG result = invoke(call, input, &escaped);
    MEMORY_BASIC_INFORMATION query{};
    if (!VirtualQuery(arena, &query, sizeof query))
      fail("input-query");
    printf("case=%u status=%08lX escaped=%08lX input=%llX protection=%08lX "
           "after-protection=%08lX\n",
           mode, ULONG(result), escaped,
           static_cast<unsigned long long>(uintptr_t(input)), protection,
           query.Protect);
    dump("after", input, 512);
  }
  puts("complete cases=16 operation=6");
  return 0;
}
