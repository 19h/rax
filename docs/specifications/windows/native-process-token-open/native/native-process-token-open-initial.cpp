#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <vector>
#include <windows.h>
#include <winternl.h>
using Open = LONG(NTAPI *)(HANDLE, ACCESS_MASK, HANDLE *);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
struct Entry {
  HANDLE handle;
  uintptr_t handles, pointers;
  ULONG access, type, attributes, reserved;
};
static Query process_query, object_query;
static Open token_open;
static void fail(const char *where) {
  printf("failure=%s error=%lu\n", where, GetLastError());
  ExitProcess(31);
}
static std::vector<Entry> entries() {
  std::vector<uintptr_t> buffer(32768);
  ULONG length = 0;
  LONG status =
      process_query(GetCurrentProcess(), 51, buffer.data(),
                    ULONG(buffer.size() * sizeof(uintptr_t)), &length);
  if (status < 0 || buffer[0] > 8192 ||
      length > buffer.size() * sizeof(uintptr_t))
    fail("handle-snapshot");
  auto first = reinterpret_cast<Entry *>(buffer.data() + 2);
  return {first, first + buffer[0]};
}
static LONG call(HANDLE process, ULONG access, HANDLE *out, ULONG &exception) {
  __try {
    return token_open(process, access, out);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    exception = GetExceptionCode();
    return LONG(exception);
  }
}
int main(int argc, char **argv) {
  if (argc != 2)
    return 2;
  HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
  token_open =
      reinterpret_cast<Open>(GetProcAddress(ntdll, "NtOpenProcessToken"));
  process_query = reinterpret_cast<Query>(
      GetProcAddress(ntdll, "NtQueryInformationProcess"));
  object_query =
      reinterpret_cast<Query>(GetProcAddress(ntdll, "NtQueryObject"));
  if (!token_open || !process_query || !object_query)
    fail("exports");
  auto memory = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 8192, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE));
  if (!memory)
    fail("arena");
  std::vector<HANDLE> handles{GetCurrentProcess(), nullptr, GetCurrentThread(),
                              reinterpret_cast<HANDLE>(uintptr_t(0x1234))};
  for (ULONG access : {0u, 0x10u, 0x400u, 0x1000u, 0x1400u, 0x1fffffu}) {
    HANDLE handle = OpenProcess(access, FALSE, GetCurrentProcessId());
    printf("process-access requested=%08lX opened=%u error=%lu\n", access,
           handle != nullptr, handle ? 0 : GetLastError());
    handles.push_back(handle);
  }
  handles.push_back(CreateEventW(nullptr, TRUE, FALSE, nullptr));
  for (uintptr_t value : {uintptr_t(0xffffffff), uintptr_t(-3), uintptr_t(-4),
                          uintptr_t(-5), uintptr_t(-6)})
    handles.push_back(reinterpret_cast<HANDLE>(value));
  const ULONG desired[] = {
      0,          1,          2,         4,         8,          0x10,
      0x20,       0x40,       0x80,      0x100,     0x1ff,      0x200,
      0x400,      0xffff,     0x10000,   0x20000,   0x40000,    0x80000,
      0xf0000,    0x100000,   0x1000000, 0x2000000, 0x10000000, 0x20000000,
      0x40000000, 0x80000000, 0xf01ff,   0xffffffff};
  unsigned ordinal = 0;
  printf("identity profile=%s width=%zu handles=%zu\n", argv[1], sizeof(void *),
         handles.size());
  for (unsigned family = 0; family < 2; ++family) {
    for (unsigned h = 0; h < handles.size(); ++h) {
      for (unsigned ai = 0; ai < sizeof(desired) / sizeof(desired[0]); ++ai) {
        if (family == 1 && desired[ai] != 8 && desired[ai] != 0x1000000 &&
            desired[ai] != 0xffffffff)
          continue;
        for (unsigned mode = 0; mode < (family == 0 ? 1u : 12u); ++mode) {
          DWORD old = 0;
          if (!VirtualProtect(memory, 8192, PAGE_READWRITE, &old))
            fail("restore");
          memset(memory, 0xa5, 8192);
          uintptr_t pointer = uintptr_t(memory + 128);
          switch (mode) {
          case 1:
            pointer = 0;
            break;
          case 2:
            pointer = 1;
            break;
          case 3:
            pointer += 1;
            break;
          case 4:
            if (!VirtualProtect(memory, 4096, PAGE_READONLY, &old))
              fail("readonly");
            break;
          case 5:
            if (!VirtualProtect(memory, 4096, PAGE_NOACCESS, &old))
              fail("noaccess");
            break;
          case 6:
            if (!VirtualProtect(memory, 4096, PAGE_READWRITE | PAGE_GUARD,
                                &old))
              fail("guard-rw");
            break;
          case 7:
            if (!VirtualProtect(memory, 4096, PAGE_READONLY | PAGE_GUARD, &old))
              fail("guard-ro");
            break;
          case 8:
            pointer = uintptr_t(memory + 4096 - sizeof(void *));
            if (!VirtualProtect(memory + 4096, 4096, PAGE_NOACCESS, &old))
              fail("adjacent");
            break;
          case 9:
            pointer = uintptr_t(memory + 4096 - sizeof(void *) + 1);
            if (!VirtualProtect(memory + 4096, 4096, PAGE_NOACCESS, &old))
              fail("cross");
            break;
          case 10:
            pointer = uintptr_t(memory + 4092);
            if (!VirtualProtect(memory + 4096, 4096,
                                PAGE_READWRITE | PAGE_GUARD, &old))
              fail("cross-guard");
            break;
          case 11:
            pointer = uintptr_t(memory + 8192);
            break;
          }
          auto before = entries();
          SetLastError(0x4a2e19f7);
          ULONG exception = 0;
          LONG status = call(handles[h], desired[ai],
                             reinterpret_cast<HANDLE *>(pointer), exception);
          DWORD last_error = GetLastError();
          MEMORY_BASIC_INFORMATION first{}, second{};
          VirtualQuery(memory, &first, sizeof(first));
          VirtualQuery(memory + 4096, &second, sizeof(second));
          if (!VirtualProtect(memory, 8192, PAGE_READWRITE, &old))
            fail("readback");
          auto after = entries();
          std::vector<Entry> born;
          for (auto e : after) {
            bool found = false;
            for (auto previous : before)
              if (e.handle == previous.handle)
                found = true;
            if (!found)
              born.push_back(e);
          }
          unsigned lost = 0;
          for (auto previous : before) {
            bool found = false;
            for (auto e : after)
              if (previous.handle == e.handle)
                found = true;
            if (!found)
              ++lost;
          }
          for (auto e : born) {
            unsigned char buffer[2048]{};
            ULONG returned = 0;
            if (object_query(e.handle, 2, buffer, sizeof(buffer), &returned) <
                0)
              fail("born-type");
            auto name = reinterpret_cast<UNICODE_STRING *>(buffer);
            if (name->Length != 10 || memcmp(name->Buffer, L"Token", 10))
              fail("born-not-token");
          }
          uintptr_t output = 0;
          if (pointer >= uintptr_t(memory) &&
              pointer + sizeof(output) <= uintptr_t(memory + 8192))
            memcpy(&output, reinterpret_cast<void *>(pointer), sizeof(output));
          printf("case ordinal=%u family=%u handle=%u desired=%08lX mode=%u "
                 "status=%08lX exception=%08lX last-error=%08lX after0=%lX "
                 "after1=%lX births=%zu output=%llX grants=",
                 ordinal++, family, h, desired[ai], mode, ULONG(status),
                 exception, last_error, first.Protect, second.Protect,
                 born.size(), static_cast<unsigned long long>(output));
          for (auto e : born)
            printf("%08lX,", e.access);
          printf(" lost=%u", lost);
          printf(" output-is-birth=%u prefix=",
                 born.size() == 1 && uintptr_t(born[0].handle) == output);
          for (unsigned i = 120; i < 144; ++i)
            printf("%02X", unsigned(memory[i]));
          printf(" boundary=");
          for (unsigned i = 4080; i < 4112; ++i)
            printf("%02X", unsigned(memory[i]));
          printf("\n");
          for (auto e : born)
            if (!CloseHandle(e.handle))
              fail("close-born");
        }
      }
    }
  }
  for (unsigned i = 4; i <= 10; ++i)
    if (handles[i] && !CloseHandle(handles[i]))
      fail("close-input");
  if (!VirtualFree(memory, 0, MEM_RELEASE))
    fail("free");
  printf("complete cases=%u\n", ordinal);
}
