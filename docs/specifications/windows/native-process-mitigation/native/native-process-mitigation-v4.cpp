#define WIN32_LEAN_AND_MEAN
#include <array>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>
using Query = NTSTATUS(NTAPI *)(HANDLE, PROCESSINFOCLASS, PVOID, ULONG, PULONG);
struct Result {
  ULONG status, exception;
};
static Result invoke(Query query, HANDLE process, void *output, ULONG length,
                     PULONG returned) {
  __try {
    return {static_cast<ULONG>(query(process, static_cast<PROCESSINFOCLASS>(52),
                                     output, length, returned)),
            0};
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    return {0xDEADBEEF, GetExceptionCode()};
  }
}
static bool protect(void *address, SIZE_T size, DWORD flags) {
  DWORD old = 0;
  return VirtualProtect(address, size, flags, &old) != 0;
}
static DWORD protection(void *address) {
  MEMORY_BASIC_INFORMATION info{};
  return VirtualQuery(address, &info, sizeof(info)) ? info.Protect : 0xFFFFFFFF;
}
int main(int argc, char **argv) {
  auto query = reinterpret_cast<Query>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtQueryInformationProcess"));
  if (!query)
    return 2;
  SYSTEM_INFO sys{};
  GetSystemInfo(&sys);
  SIZE_T page = sys.dwPageSize;
  BYTE *o = static_cast<BYTE *>(VirtualAlloc(
      nullptr, 2 * page, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  BYTE *r = static_cast<BYTE *>(VirtualAlloc(
      nullptr, 2 * page, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!o || !r) {
    if (o)
      VirtualFree(o, 0, MEM_RELEASE);
    if (r)
      VirtualFree(r, 0, MEM_RELEASE);
    return 3;
  }
  std::array<HANDLE, 8> handles = {GetCurrentProcess(),
                                   nullptr,
                                   nullptr,
                                   nullptr,
                                   nullptr,
                                   GetCurrentThread(),
                                   reinterpret_cast<HANDLE>(uintptr_t(0xBAD)),
                                   nullptr};
  unsigned duplicates = 0, closed = 0;
  for (unsigned i = 1; i <= 4; ++i) {
    DWORD access = i == 1 ? 0x400 : i == 2 ? 0x1000 : i == 3 ? 0 : 0x20;
    if (!DuplicateHandle(GetCurrentProcess(), GetCurrentProcess(),
                         GetCurrentProcess(), &handles[i], access, FALSE, 0))
      return 4;
    ++duplicates;
  }
  DWORD before = 0, after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 5;
  std::printf("PROFILE %s POINTER_BYTES %zu PAGE_BYTES %zu\n",
              argc > 1 ? argv[1] : "unknown", sizeof(void *), page);
  const std::array<const char *, 24> names = {
      "valid",
      "out-null",
      "out-noaccess",
      "out-guard",
      "out-readonly",
      "out-readonly-guard",
      "ret-noaccess",
      "ret-guard",
      "ret-readonly",
      "ret-readonly-guard",
      "both-guard",
      "out-misaligned",
      "ret-misaligned",
      "alias-selector",
      "alias-flags",
      "out-flags-readonly-guard",
      "out-flags-guard",
      "ret-null",
      "out-guard-ret-readonly",
      "out-readonly-ret-guard",
      "out-misaligned-ret-noaccess",
      "out-noaccess-ret-guard",
      "both-readonly-guard",
      "out-flags-readonly-guard-ret-guard"};
  unsigned calls = 0;
  for (unsigned role = 0; role < handles.size(); ++role)
    for (unsigned c = 0; c < names.size(); ++c)
      for (ULONG length : {7UL, 8UL}) {
        if (!protect(o, 2 * page, PAGE_READWRITE) ||
            !protect(r, 2 * page, PAGE_READWRITE))
          return 6;
        std::memset(o, 0xA5, 2 * page);
        std::memset(r, 0xBB, 2 * page);
        BYTE *output = c == 11 || c == 20   ? o + 1
                       : c == 15 || c == 23 ? o + page - 4
                       : c == 16            ? o + page - 4
                                            : o;
        ULONG selector = 11;
        std::memcpy(output, &selector, 4);
        BYTE *returned = c == 12   ? r + 1
                         : c == 13 ? output
                         : c == 14 ? output + 4
                                   : r;
        if (c == 2 || c == 21) {
          if (!protect(o, page, PAGE_NOACCESS))
            return 7;
        }
        if (c == 3 || c == 10 || c == 18) {
          if (!protect(o, page, PAGE_READWRITE | PAGE_GUARD))
            return 7;
        }
        if (c == 4 || c == 19) {
          if (!protect(o, page, PAGE_READONLY))
            return 7;
        }
        if (c == 5 || c == 22) {
          if (!protect(o, page, PAGE_READONLY | PAGE_GUARD))
            return 7;
        }
        if (c == 6 || c == 20) {
          if (!protect(r, page, PAGE_NOACCESS))
            return 7;
        }
        if (c == 7 || c == 10 || c == 19 || c == 21 || c == 23) {
          if (!protect(r, page, PAGE_READWRITE | PAGE_GUARD))
            return 7;
        }
        if (c == 8 || c == 18) {
          if (!protect(r, page, PAGE_READONLY))
            return 7;
        }
        if (c == 9 || c == 22) {
          if (!protect(r, page, PAGE_READONLY | PAGE_GUARD))
            return 7;
        }
        if (c == 15 || c == 16 || c == 23) {
          if (!protect(o + page, page,
                       c == 16 ? PAGE_READWRITE | PAGE_GUARD
                               : PAGE_READONLY | PAGE_GUARD))
            return 7;
        }
        auto result =
            invoke(query, handles[role], c == 1 ? nullptr : output, length,
                   c == 17 ? nullptr : reinterpret_cast<PULONG>(returned));
        DWORD op = protection(o), on = protection(o + page), rp = protection(r);
        if (!protect(o, 2 * page, PAGE_READWRITE) ||
            !protect(r, 2 * page, PAGE_READWRITE))
          return 8;
        std::printf("QUERY role=%u case=%u name=%s length=%lu status=%08lX "
                    "exception=%08lX out-protection=%08lX "
                    "next-protection=%08lX ret-protection=%08lX out=",
                    role, c, names[c], length, result.status, result.exception,
                    op, on, rp);
        for (unsigned i = 0; i < 32; ++i)
          std::printf("%02X", unsigned(output[i]));
        std::printf(" ret=");
        for (unsigned i = 0; i < 16; ++i)
          std::printf("%02X", unsigned(r[i]));
        std::printf("\n");
        ++calls;
      }
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 9;
  for (unsigned i = 1; i <= 4; ++i) {
    if (!CloseHandle(handles[i]))
      return 10;
    ++closed;
  }
  if (!VirtualFree(o, 0, MEM_RELEASE) || !VirtualFree(r, 0, MEM_RELEASE))
    return 11;
  std::printf("SUMMARY calls=%u handles-before=%lu handles-after=%lu "
              "duplicates=%u closed=%u pages-allocated=4 pages-released=4\n",
              calls, before, after, duplicates, closed);
  return before == after && duplicates == closed ? 0 : 12;
}
