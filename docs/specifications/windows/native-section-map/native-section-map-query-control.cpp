#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <set>
#include <vector>
#include <windows.h>
#include <winternl.h>
using Open = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);
using Map = LONG(NTAPI *)(HANDLE, HANDLE, void **, ULONG_PTR, SIZE_T,
                          LARGE_INTEGER *, SIZE_T *, ULONG, ULONG, ULONG);
using Unmap = LONG(NTAPI *)(HANDLE, void *);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
static Map map_section;
static Unmap unmap_section;
static Query query_object;
static unsigned char *memory;
static HANDLE section_handles[13]{};
static HANDLE process_handles[7]{};
static unsigned count;
using QueryMemory = LONG(NTAPI *)(HANDLE, void *, ULONG, void *, SIZE_T,
                                  SIZE_T *);
static QueryMemory query_memory;
static uintptr_t high;
static ULONG copy_exception;
static void fail(const char *what) {
  printf("failure=%s error=%lu\n", what, GetLastError());
  ExitProcess(21);
}
static HANDLE open(Open fn, const wchar_t *path, ULONG access) {
  UNICODE_STRING name{};
  name.Length = USHORT(wcslen(path) * 2);
  name.MaximumLength = name.Length + 2;
  name.Buffer = const_cast<PWSTR>(path);
  OBJECT_ATTRIBUTES attrs{};
  attrs.Length = sizeof(attrs);
  attrs.ObjectName = &name;
  attrs.Attributes = 0x40;
  HANDLE handle = nullptr;
  LONG status = fn(&handle, access, &attrs);
  printf("input-open path=");
  for (unsigned i = 0; i < name.Length / 2; i++)
    printf("%04X", unsigned(path[i]));
  printf(" request=%08lX status=%08lX handle=%llX\n", access, ULONG(status),
         (unsigned long long)uintptr_t(handle));
  if (status < 0 || !handle)
    fail("open-input");
  return handle;
}
static ULONG grant(HANDLE handle) {
  ULONG basic[14]{}, returned = 0;
  if (query_object(handle, 0, basic, sizeof(basic), &returned) < 0 ||
      returned != sizeof(basic))
    fail("input-basic");
  return basic[1];
}
static std::set<uintptr_t> images() {
  std::set<uintptr_t> result;
  uintptr_t cursor = 0;
  unsigned regions = 0;
  while (cursor < high) {
    MEMORY_BASIC_INFORMATION info{};
    if (!VirtualQuery(reinterpret_cast<void *>(cursor), &info, sizeof(info)))
      fail("image-enumeration");
    uintptr_t next = uintptr_t(info.BaseAddress) + info.RegionSize;
    if (next <= cursor || ++regions > 16384)
      fail("image-enumeration-bounds");
    if (info.Type == MEM_IMAGE)
      result.insert(uintptr_t(info.AllocationBase));
    cursor = next;
  }
  return result;
}
static bool copy(void *out, const void *from, SIZE_T size) {
  copy_exception = 0;
  __try {
    memcpy(out, from, size);
    return true;
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    copy_exception = GetExceptionCode();
    return false;
  }
}
static void view(uintptr_t allocation, unsigned ordinal, uintptr_t image = 0) {
  uintptr_t cursor = allocation;
  SIZE_T extent = 0;
  unsigned regions = 0;
  for (;;) {
    MEMORY_BASIC_INFORMATION info{};
    if (!VirtualQuery(reinterpret_cast<void *>(cursor), &info, sizeof(info)))
      fail("view-query");
    if (uintptr_t(info.AllocationBase) != allocation)
      break;
    uintptr_t next = uintptr_t(info.BaseAddress) + info.RegionSize;
    if (next <= cursor || ++regions > 256 ||
        next - allocation > 64 * 1024 * 1024)
      fail("view-region-bounds");
    printf("view-region case=%u offset=%llX size=%llu state=%lX protect=%lX "
           "allocation-protect=%lX type=%lX allocation=%llX\n",
           ordinal,
           (unsigned long long)(uintptr_t(info.BaseAddress) - allocation),
           (unsigned long long)info.RegionSize, info.State, info.Protect,
           info.AllocationProtect, info.Type,
           (unsigned long long)uintptr_t(info.AllocationBase));
    cursor = next;
    extent = cursor - allocation;
  }
  unsigned char header_before[512]{}, header_after[512]{};
  uintptr_t observed_image = image ? image : allocation;
  bool before_ok = copy(header_before, reinterpret_cast<void *>(observed_image),
                        sizeof(header_before));
  alignas(16) unsigned char information[64]{};
  SIZE_T returned = 0;
  LONG status =
      query_memory(GetCurrentProcess(), reinterpret_cast<void *>(allocation), 6,
                   information, sizeof(void *) == 4 ? 12 : 24, &returned);
  printf("view-information case=%u status=%08lX returned=%zu data=", ordinal,
         ULONG(status), size_t(returned));
  for (unsigned i = 0; i < (sizeof(void *) == 4 ? 12u : 24u); i++)
    printf("%02X", unsigned(information[i]));
  puts("");
  bool after_ok = copy(header_after, reinterpret_cast<void *>(observed_image),
                       sizeof(header_after));
  printf("query-header-control case=%u before-readable=%u after-readable=%u "
         "equal=%u before=",
         ordinal, before_ok, after_ok,
         before_ok && after_ok &&
             !memcmp(header_before, header_after, sizeof(header_before)));
  for (unsigned i = 0; i < sizeof(header_before); i++)
    printf("%02X", unsigned(header_before[i]));
  printf(" after=");
  for (unsigned i = 0; i < sizeof(header_after); i++)
    printf("%02X", unsigned(header_after[i]));
  puts("");
  uintptr_t base = image ? image : allocation;
  printf(
      "view-extent case=%u allocation=%llX image=%llX size=%llu regions=%u\n",
      ordinal, (unsigned long long)allocation, (unsigned long long)base,
      (unsigned long long)extent, regions);
  if (base < allocation || base - allocation >= extent)
    fail("view-image-address");
  extent -= base - allocation;
  IMAGE_DOS_HEADER dos{};
  if (!copy(&dos, reinterpret_cast<void *>(base), sizeof(dos))) {
    printf("view-header-unreadable case=%u exception=%08lX\n", ordinal,
           copy_exception);
    return;
  }
  if (dos.e_magic != IMAGE_DOS_SIGNATURE || dos.e_lfanew < 64 ||
      dos.e_lfanew > 4096) {
    printf("view-header-invalid case=%u magic=%04X nt-offset=%lX\n", ordinal,
           unsigned(dos.e_magic), ULONG(dos.e_lfanew));
    return;
  }
  unsigned char nt[264]{};
  if (!copy(nt, reinterpret_cast<void *>(base + dos.e_lfanew), sizeof(nt)) ||
      memcmp(nt, "PE\0\0", 4))
    fail("view-nt-header");
  WORD machine = 0, magic = 0;
  ULONG size = 0, reloc_rva = 0, reloc_size = 0;
  ULONGLONG preferred = 0;
  memcpy(&machine, nt + 4, 2);
  memcpy(&magic, nt + 24, 2);
  memcpy(&size, nt + 80, 4);
  if (magic == 0x20B) {
    memcpy(&preferred, nt + 48, 8);
    memcpy(&reloc_rva, nt + 24 + 112 + 5 * 8, 4);
    memcpy(&reloc_size, nt + 24 + 112 + 5 * 8 + 4, 4);
  } else if (magic == 0x10B) {
    ULONG narrow = 0;
    memcpy(&narrow, nt + 52, 4);
    preferred = narrow;
    memcpy(&reloc_rva, nt + 24 + 96 + 5 * 8, 4);
    memcpy(&reloc_size, nt + 24 + 96 + 5 * 8 + 4, 4);
  } else
    fail("view-optional-header");
  printf("view case=%u base=%llX machine=%04X preferred=%llX size=%lu "
         "relocation-rva=%lX relocation-size=%lu\n",
         ordinal, (unsigned long long)base, unsigned(machine),
         (unsigned long long)preferred, size, reloc_rva, reloc_size);
  if (!size || size > 64 * 1024 * 1024 || reloc_rva > size ||
      reloc_size > size - reloc_rva || reloc_size > 1024 * 1024)
    fail("view-size-bounds");
  if (reloc_rva > extent || reloc_size > extent - reloc_rva) {
    printf("view-relocation-outside-view case=%u\n", ordinal);
    return;
  }
  ULONG at = reloc_rva, stop = at + reloc_size;
  unsigned observed = 0;
  while (at < stop && observed < 16) {
    IMAGE_BASE_RELOCATION block{};
    if (stop - at < sizeof(block) ||
        !copy(&block, reinterpret_cast<void *>(base + at), sizeof(block)) ||
        block.SizeOfBlock < sizeof(block) || (block.SizeOfBlock & 1) ||
        block.SizeOfBlock > stop - at)
      fail("view-relocation-block");
    for (ULONG i = 0;
         i < (block.SizeOfBlock - sizeof(block)) / 2 && observed < 16; i++) {
      WORD item = 0;
      if (!copy(&item,
                reinterpret_cast<void *>(base + at + sizeof(block) + 2 * i), 2))
        fail("view-relocation-entry");
      if (!(item >> 12))
        continue;
      uintptr_t target = block.VirtualAddress + (item & 0xFFF);
      ULONGLONG value = 0;
      unsigned width = (item >> 12) == 10 ? 8 : 4;
      if (target > extent || width > extent - target) {
        printf("view-relocation-target-outside-view case=%u rva=%llX\n",
               ordinal, (unsigned long long)target);
        observed++;
        continue;
      }
      if (!copy(&value, reinterpret_cast<void *>(base + target), width))
        fail("view-relocation-target");
      printf("view-relocation case=%u index=%u rva=%llX type=%u value=%llX\n",
             ordinal, observed++, (unsigned long long)target, item >> 12,
             (unsigned long long)value);
    }
    at += block.SizeOfBlock;
  }
}
struct Case {
  unsigned family = 0, section = 1, process = 0;
  unsigned base_mode = 0, size_mode = 0, offset_mode = 0;
  uintptr_t base = 0, zero_bits = 0;
  SIZE_T commit = 0, size = 0;
  LONGLONG offset = 0;
  bool use_offset = false;
  ULONG inherit = 1, allocation = 0x800000, protection = 0x80;
};
static uintptr_t pointer(unsigned mode, unsigned page) {
  uintptr_t normal = uintptr_t(memory + page * 4096 + 128);
  switch (mode) {
  case 1:
    return 0;
  case 2:
    return 1;
  case 3:
    return normal + 1;
  case 4:
    return normal + 2;
  case 5:
  case 6:
  case 7:
  case 9:
    return uintptr_t(memory + page * 4096);
  case 8:
    return uintptr_t(memory + (page + 1) * 4096 - sizeof(void *) / 2);
  case 10:
    return normal + 3;
  case 11:
    return normal + 4;
  default:
    return normal;
  }
}
static void protection(unsigned mode, unsigned page) {
  DWORD value = PAGE_READWRITE, old = 0;
  if (mode == 5)
    value |= PAGE_GUARD;
  if (mode == 6 || mode == 8)
    value = PAGE_NOACCESS;
  if (mode == 7)
    value = PAGE_READONLY;
  if (mode == 9)
    value = PAGE_READONLY | PAGE_GUARD;
  if (value != PAGE_READWRITE &&
      !VirtualProtect(memory + (page + (mode == 8)) * 4096, 4096, value, &old))
    fail("input-protection");
}
static void store(uintptr_t at, const void *value, SIZE_T size) {
  if (at >= uintptr_t(memory) && at - uintptr_t(memory) <= 8 * 4096 - size)
    memcpy(reinterpret_cast<void *>(at), value, size);
}
static ULONGLONG load(uintptr_t at, SIZE_T size) {
  ULONGLONG result = ~ULONGLONG(0);
  if (at >= uintptr_t(memory) && at - uintptr_t(memory) <= 8 * 4096 - size) {
    result = 0;
    memcpy(&result, reinterpret_cast<void *>(at), size);
  }
  return result;
}
static DWORD page_protection(unsigned page) {
  MEMORY_BASIC_INFORMATION info{};
  if (!VirtualQuery(memory + page * 4096, &info, sizeof(info)))
    fail("input-query");
  return info.Protect;
}
static LONG invoke(const Case *c, uintptr_t base, uintptr_t size,
                   uintptr_t offset, ULONG *exception) {
  __try {
    return map_section(section_handles[c->section], process_handles[c->process],
                       reinterpret_cast<void **>(base), c->zero_bits, c->commit,
                       c->use_offset ? reinterpret_cast<LARGE_INTEGER *>(offset)
                                     : nullptr,
                       reinterpret_cast<SIZE_T *>(size), c->inherit,
                       c->allocation, c->protection);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
    return LONG(*exception);
  }
}
static void one(const Case &c) {
  DWORD old = 0;
  if (!VirtualProtect(memory, 8 * 4096, PAGE_READWRITE, &old))
    fail("input-unprotect");
  memset(memory, 0xA5, 8 * 4096);
  uintptr_t bp = pointer(c.base_mode, 0), sp = pointer(c.size_mode, 2),
            op = pointer(c.offset_mode, 4);
  store(bp, &c.base, sizeof(void *));
  store(sp, &c.size, sizeof(SIZE_T));
  store(op, &c.offset, 8);
  protection(c.base_mode, 0);
  protection(c.size_mode, 2);
  if (c.use_offset)
    protection(c.offset_mode, 4);
  auto before = images();
  DWORD handles_before = 0, handles_after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &handles_before))
    fail("handle-count-before");
  ULONG exception = 0;
  LONG status = invoke(&c, bp, sp, op, &exception);
  DWORD p0 = page_protection(0), p2 = page_protection(2),
        p4 = page_protection(4);
  if (!VirtualProtect(memory, 8 * 4096, PAGE_READWRITE, &old))
    fail("output-unprotect");
  auto after = images();
  std::vector<uintptr_t> added;
  for (auto base : before)
    if (!after.contains(base))
      fail("existing-image-lost");
  for (auto base : after)
    if (!before.contains(base))
      added.push_back(base);
  printf(
      "case=%u family=%u section=%u process=%u base-mode=%u size-mode=%u "
      "offset-mode=%u base-in=%llX zero-bits=%llX commit=%llu size-in=%llu "
      "offset-in=%llX use-offset=%u inherit=%lu allocation=%08lX protect=%08lX "
      "status=%08lX exception=%08lX base-out=%llX size-out=%llu "
      "offset-out=%llX "
      "p0=%lX p2=%lX p4=%lX added=%zu\n",
      count, c.family, c.section, c.process, c.base_mode, c.size_mode,
      c.offset_mode, (unsigned long long)c.base,
      (unsigned long long)c.zero_bits, (unsigned long long)c.commit,
      (unsigned long long)c.size, (unsigned long long)c.offset, c.use_offset,
      c.inherit, c.allocation, c.protection, ULONG(status), exception,
      (unsigned long long)load(bp, sizeof(void *)),
      (unsigned long long)load(sp, sizeof(SIZE_T)),
      (unsigned long long)load(op, 8), p0, p2, p4, added.size());
  for (auto base : added) {
    uintptr_t returned = uintptr_t(load(bp, sizeof(void *)));
    MEMORY_BASIC_INFORMATION info{};
    if (!VirtualQuery(reinterpret_cast<void *>(returned), &info,
                      sizeof(info)) ||
        uintptr_t(info.AllocationBase) != base)
      returned = 0;
    view(base, count, returned);
    if (unmap_section(GetCurrentProcess(), reinterpret_cast<void *>(base)) < 0)
      fail("unmap-owned-view");
  }
  if (images() != before)
    fail("mapping-set-restore");
  if (!GetProcessHandleCount(GetCurrentProcess(), &handles_after) ||
      handles_before != handles_after)
    fail("handle-count-restore");
  count++;
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto ntdll = GetModuleHandleW(L"ntdll.dll");
  map_section =
      reinterpret_cast<Map>(GetProcAddress(ntdll, "NtMapViewOfSection"));
  unmap_section =
      reinterpret_cast<Unmap>(GetProcAddress(ntdll, "NtUnmapViewOfSection"));
  query_object =
      reinterpret_cast<Query>(GetProcAddress(ntdll, "NtQueryObject"));
  auto open_section =
      reinterpret_cast<Open>(GetProcAddress(ntdll, "NtOpenSection"));
  auto open_directory =
      reinterpret_cast<Open>(GetProcAddress(ntdll, "NtOpenDirectoryObject"));
  if (!map_section || !unmap_section || !query_object || !open_section ||
      !open_directory)
    fail("exports");
  query_memory = reinterpret_cast<QueryMemory>(
      GetProcAddress(ntdll, "NtQueryVirtualMemory"));
  using StatusError = ULONG(NTAPI *)(LONG);
  auto status_error = reinterpret_cast<StatusError>(
      GetProcAddress(ntdll, "RtlNtStatusToDosError"));
  if (!query_memory || !status_error)
    fail("ancillary-exports");
  for (ULONG status : {0x40000003u, 0x40000036u, 0xC000001Fu, 0xC0000220u})
    printf("status-error status=%08lX win32=%lu\n", status,
           status_error(LONG(status)));
  SYSTEM_INFO system{};
  GetSystemInfo(&system);
  high = uintptr_t(system.lpMaximumApplicationAddress) + 1;
  memory = static_cast<unsigned char *>(VirtualAlloc(
      nullptr, 8 * 4096, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE));
  if (!memory || !high)
    fail("probe-memory");
  for (unsigned i = 1; i <= 6; i++)
    section_handles[i] =
        open(open_section, L"\\KnownDlls\\kernel32.dll",
             std::initializer_list<ULONG>{0xD, 1, 4, 8, 2, 0x02000000}
                 .begin()[i - 1]);
  section_handles[7] = open(open_section, L"\\KnownDlls32\\kernel32.dll", 0xD);
  section_handles[8] =
      open(open_section, L"\\KnownDllsChpe32\\kernel32.dll", 0xD);
  section_handles[9] = open(open_section, L"\\KnownDlls\\ntdll.dll", 0xD);
  section_handles[10] = CreateEventW(nullptr, FALSE, FALSE, nullptr);
  section_handles[11] = open(open_directory, L"\\KnownDlls", 1);
  section_handles[12] = GetCurrentProcess();
  if (!section_handles[10])
    fail("event-input");
  for (unsigned i = 1; i <= 11; i++)
    printf("section-input index=%u handle=%llX grant=%08lX\n", i,
           (unsigned long long)uintptr_t(section_handles[i]),
           grant(section_handles[i]));
  process_handles[0] = GetCurrentProcess();
  process_handles[1] = nullptr;
  process_handles[2] = GetCurrentThread();
  process_handles[3] =
      OpenProcess(PROCESS_VM_OPERATION, FALSE, GetCurrentProcessId());
  process_handles[4] =
      OpenProcess(PROCESS_QUERY_INFORMATION, FALSE, GetCurrentProcessId());
  process_handles[5] = reinterpret_cast<HANDLE>(uintptr_t(1));
  process_handles[6] =
      OpenProcess(PROCESS_ALL_ACCESS, FALSE, GetCurrentProcessId());
  if (!process_handles[3] || !process_handles[4] || !process_handles[6])
    fail("process-input");
  for (unsigned i : {3u, 4u, 6u})
    printf("process-input index=%u handle=%llX grant=%08lX\n", i,
           (unsigned long long)uintptr_t(process_handles[i]),
           grant(process_handles[i]));
  Case c;
  one(c);
  for (ULONG allocation : {0u, 0x800000u, 0x20000000u}) {
    c = {};
    c.allocation = allocation;
    one(c);
  }
  auto before = images();
  HANDLE temporary = nullptr;
  if (!DuplicateHandle(GetCurrentProcess(), section_handles[1],
                       GetCurrentProcess(), &temporary, 0, FALSE,
                       DUPLICATE_SAME_ACCESS))
    fail("lifetime-section-duplicate");
  void *mapped = nullptr;
  SIZE_T size = 0;
  LONG status = map_section(temporary, GetCurrentProcess(), &mapped, 0, 0,
                            nullptr, &size, 1, 0x800000, 0x80);
  if (status < 0 || !mapped)
    fail("lifetime-map");
  if (!CloseHandle(temporary))
    fail("lifetime-close-section");
  printf("lifetime status=%08lX base=%llX size=%llu closed=1\n", ULONG(status),
         (unsigned long long)uintptr_t(mapped), (unsigned long long)size);
  view(uintptr_t(mapped), count);
  if (unmap_section(GetCurrentProcess(), mapped) < 0 || images() != before)
    fail("lifetime-view-restore");
  for (unsigned i = 1; i <= 11; i++)
    if (!CloseHandle(section_handles[i]))
      fail("close-input-section");
  for (unsigned i : {3u, 4u, 6u})
    if (!CloseHandle(process_handles[i]))
      fail("close-input-process");
  if (!VirtualFree(memory, 0, MEM_RELEASE))
    fail("free-input-memory");
  printf("complete cases=%u lifetime=1 pointer-bytes=%zu\n", count,
         sizeof(void *));
  return 0;
}
