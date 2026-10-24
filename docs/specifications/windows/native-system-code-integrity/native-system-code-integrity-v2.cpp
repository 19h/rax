#define WIN32_LEAN_AND_MEAN
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>

using Query = LONG(NTAPI *)(ULONG, void *, ULONG, ULONG *);
using LastStatus = LONG(NTAPI *)();
static Query query;
static LastStatus last_status;

static void observe(unsigned sample, const char *kind, unsigned offset,
                    ULONG length, ULONG embedded, const char *returned_kind) {
  alignas(16) unsigned char stack[96];
  std::memset(stack, 0xA5, sizeof(stack));
  auto pages = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 8192, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  auto returned_page = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 4096, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!pages || !returned_page) {
    ExitProcess(4);
  }
  std::memset(pages, 0xA5, 8192);
  std::memset(returned_page, 0xFE, 4096);
  unsigned char *output = stack + offset;
  if (std::strcmp(kind, "null") == 0) {
    output = nullptr;
  } else if (std::strcmp(kind, "one") == 0) {
    output = reinterpret_cast<unsigned char *>(1);
  } else if (std::strcmp(kind, "stack") != 0) {
    output = pages + (std::strncmp(kind, "cross-", 6) == 0 ? 4092 : 0);
  }
  if (reinterpret_cast<std::uintptr_t>(output) > 1) {
    std::memcpy(output, &embedded, sizeof(embedded));
  }
  DWORD ignored = 0;
  DWORD protection = PAGE_READWRITE;
  if (std::strcmp(kind, "readonly-guard") == 0) {
    protection = PAGE_READONLY | PAGE_GUARD;
  } else if (std::strcmp(kind, "readonly") == 0 ||
             std::strcmp(kind, "cross-readonly") == 0) {
    protection = PAGE_READONLY;
  } else if (std::strcmp(kind, "noaccess") == 0 ||
             std::strcmp(kind, "cross-noaccess") == 0) {
    protection = PAGE_NOACCESS;
  } else if (std::strcmp(kind, "guard") == 0) {
    protection |= PAGE_GUARD;
  }
  auto protected_page =
      pages + (std::strncmp(kind, "cross-", 6) == 0 ? 4096 : 0);
  if (!VirtualProtect(protected_page, 4096, protection, &ignored)) {
    ExitProcess(5);
  }
  auto returned = reinterpret_cast<ULONG *>(returned_page);
  if (std::strcmp(returned_kind, "null") == 0) {
    returned = nullptr;
  } else if (std::strcmp(returned_kind, "one") == 0) {
    returned = reinterpret_cast<ULONG *>(1);
  } else if (std::strcmp(returned_kind, "unaligned") == 0) {
    returned = reinterpret_cast<ULONG *>(returned_page + 1);
  } else if (std::strcmp(returned_kind, "alias-length") == 0) {
    returned = reinterpret_cast<ULONG *>(output);
  } else if (std::strcmp(returned_kind, "alias-options") == 0) {
    returned =
        reinterpret_cast<ULONG *>(reinterpret_cast<std::uintptr_t>(output) + 4);
  } else if (std::strcmp(returned_kind, "readonly") == 0 ||
             std::strcmp(returned_kind, "guard") == 0 ||
             std::strcmp(returned_kind, "readonly-guard") == 0) {
    DWORD mode =
        std::strcmp(returned_kind, "guard") == 0
            ? PAGE_READWRITE | PAGE_GUARD
            : PAGE_READONLY | (std::strcmp(returned_kind, "readonly-guard") == 0
                                   ? PAGE_GUARD
                                   : 0);
    if (!VirtualProtect(returned_page, 4096, mode, &ignored)) {
      ExitProcess(6);
    }
  }
  SetLastError(0x12345678);
  LONG before = last_status();
  LONG status = query(103, output, length, returned);
  DWORD error = GetLastError();
  LONG after = last_status();
  MEMORY_BASIC_INFORMATION output_region{}, returned_region{};
  VirtualQuery(protected_page, &output_region, sizeof(output_region));
  VirtualQuery(returned_page, &returned_region, sizeof(returned_region));
  unsigned char data[16]{};
  ULONG returned_value = 0;
  SIZE_T got = 0;
  BOOL readable =
      ReadProcessMemory(GetCurrentProcess(), output, data, sizeof(data), &got);
  BOOL returned_readable =
      ReadProcessMemory(GetCurrentProcess(), returned, &returned_value,
                        sizeof(returned_value), &got);
  std::printf(
      "QUERY sample=%u buffer=%s offset=%u length=%lu embedded=%08lX "
      "returned=%s status=%08lX error=%08lX nt-before=%08lX nt-after=%08lX "
      "output-protect=%08lX return-protect=%08lX readable=%u data=",
      sample, kind, offset, length, embedded, returned_kind,
      static_cast<ULONG>(status), error, static_cast<ULONG>(before),
      static_cast<ULONG>(after), output_region.Protect, returned_region.Protect,
      static_cast<unsigned>(readable != 0));
  if (readable) {
    for (unsigned char byte : data) {
      std::printf("%02X", static_cast<unsigned>(byte));
    }
  }
  std::printf(" return-readable=%u return-value=%08lX\n",
              static_cast<unsigned>(returned_readable != 0), returned_value);
  VirtualFree(returned_page, 0, MEM_RELEASE);
  VirtualFree(pages, 0, MEM_RELEASE);
}

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  auto ntdll = GetModuleHandleW(L"ntdll.dll");
  query = reinterpret_cast<Query>(
      GetProcAddress(ntdll, "NtQuerySystemInformation"));
  last_status =
      reinterpret_cast<LastStatus>(GetProcAddress(ntdll, "RtlGetLastNtStatus"));
  if (!query || !last_status) {
    return 2;
  }
  std::printf("PROFILE %s pointer_bytes=%zu\n", argc > 1 ? argv[1] : "unknown",
              sizeof(void *));
  for (unsigned sample = 0; sample < 3; ++sample) {
    for (const char *kind : {"stack", "readonly-guard", "guard",
                             "cross-readonly", "cross-noaccess"}) {
      for (ULONG length : {0UL, 8UL, 16UL, 0xFFFFFFFFUL}) {
        for (const char *returned :
             {"valid", "null", "guard", "readonly-guard"}) {
          observe(sample, kind, 0, length, 8, returned);
        }
      }
    }
  }
  return 0;
}
