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
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using QuerySection = LONG(NTAPI *)(HANDLE, ULONG, void *, SIZE_T, SIZE_T *);
using Close = LONG(NTAPI *)(HANDLE);
static Open open_section, open_directory, open_link;
static Query query_process, query_object;
static QuerySection query_section;
static Close close_nt;
static unsigned char *memory;
static HANDLE roots[15];
static ULONG section_type;
static std::set<uintptr_t> inputs;
static unsigned count;
struct Entry {
  HANDLE h;
  uintptr_t handles, pointers;
  ULONG access, type, attributes, reserved;
};
struct SectionBasic {
  void *base;
  ULONG attributes;
  LARGE_INTEGER size;
};
static const wchar_t *names[] = {L"\\KnownDlls\\kernel32.dll",
                                 L"\\KNOWNDLLS\\KERNEL32.DLL",
                                 L"\\KnownDlls32\\kernel32.dll",
                                 L"\\KnownDlls\\ntdll.dll",
                                 L"\\KnownDlls32\\ntdll.dll",
                                 L"\\KnownDlls\\rax-absent-20261013",
                                 L"\\KnownDlls\\KnownDllPath",
                                 L"\\KnownDlls",
                                 L"kernel32.dll",
                                 L"KERNEL32.DLL",
                                 L"",
                                 L"\\",
                                 L"\\rax-missing\\kernel32.dll",
                                 L"\\KnownDlls\\kernel32.dll\\x",
                                 L"\\KnownDlls\\\\kernel32.dll",
                                 L"kernel32.dll\\x",
                                 L"\\KnownDlls\\kernel32.dll\\",
                                 L"\\KnownDllsChpe32\\kernel32.dll",
                                 L"\\KnownDllsChpe32\\ntdll.dll",
                                 L"KnownDlls32\\kernel32.dll",
                                 L"KnownDlls32\\ntdll.dll",
                                 L"KnownDllsChpe32\\kernel32.dll",
                                 L"KnownDlls\\kernel32.dll",
                                 L"Kernel32.Dll",
                                 L"rax-absent-20261013",
                                 L".\\kernel32.dll",
                                 L"knowndlls32\\kernel32.dll"};
static const ULONG masks[] = {
    0,          1,          2,          4,          8,          0x10,
    0x20,       0x40,       0x80,       0xD,        0x1F,       0xFFFF,
    0x10000,    0x20000,    0x40000,    0x80000,    0xF0000,    0xF001F,
    0x100000,   0x1F001F,   0x01000000, 0x02000000, 0x03000000, 0x04000000,
    0x08000000, 0x80000000, 0x40000000, 0x20000000, 0x10000000, 0xC0000000,
    0xA0000000, 0xE0000000, 0xFFFFFFFF, 0x0200000D, 0x0100000D, 0x0010000D,
    0x0002000D};
static void fail(const char *what) {
  printf("failure=%s error=%lu\n", what, GetLastError());
  ExitProcess(21);
}
static std::vector<Entry> entries() {
  std::vector<uintptr_t> b(131072);
  ULONG returned = 0;
  LONG s = query_process(GetCurrentProcess(), 51, b.data(),
                         ULONG(b.size() * sizeof(uintptr_t)), &returned);
  if (s < 0 || b[0] > 8192 || returned > b.size() * sizeof(uintptr_t) ||
      returned <
          (2 + b[0] * (sizeof(Entry) / sizeof(uintptr_t))) * sizeof(uintptr_t))
    fail("handle-snapshot");
  auto first = reinterpret_cast<Entry *>(b.data() + 2);
  return {first, first + b[0]};
}
static std::set<uintptr_t> sections(const std::vector<Entry> &e) {
  std::set<uintptr_t> s;
  for (auto x : e)
    if (x.type == section_type)
      s.insert(uintptr_t(x.h));
  return s;
}
static bool inputs_intact(const std::vector<Entry> &e) {
  for (auto h : inputs) {
    bool found = false;
    for (auto x : e)
      found |= uintptr_t(x.h) == h;
    if (!found)
      return false;
  }
  return true;
}
static void close(HANDLE h) {
  if (!h || close_nt(h) < 0)
    fail("close");
}
static HANDLE base(Open fn, const wchar_t *path, ULONG access) {
  UNICODE_STRING n{};
  n.Length = USHORT(wcslen(path) * 2);
  n.MaximumLength = n.Length + 2;
  n.Buffer = const_cast<PWSTR>(path);
  OBJECT_ATTRIBUTES a{};
  a.Length = sizeof(a);
  a.ObjectName = &n;
  a.Attributes = 0x40;
  HANDLE h = nullptr;
  if (fn(&h, access, &a) < 0 || !h)
    fail("base");
  return h;
}
static ULONG grant(HANDLE h) {
  ULONG b[14]{}, n = 0;
  if (query_object(h, 0, b, sizeof(b), &n) < 0 || n != sizeof(b))
    fail("basic");
  return b[1];
}
static void type(HANDLE h) {
  uintptr_t data[1024]{};
  ULONG n = 0;
  if (query_object(h, 2, data, sizeof(data), &n) < 0 || n > sizeof(data))
    fail("type");
  auto u = reinterpret_cast<UNICODE_STRING *>(data);
  uintptr_t p = uintptr_t(u->Buffer), first = uintptr_t(data);
  if (u->Length != 14 || p < first || p - first > sizeof(data) - 14 ||
      memcmp(u->Buffer, L"Section", 14))
    fail("type-not-section");
}
static void object_name(HANDLE h) {
  uintptr_t data[2048]{};
  ULONG n = 0;
  if (query_object(h, 1, data, sizeof(data), &n) < 0 || n > sizeof(data))
    fail("object-name");
  auto u = reinterpret_cast<UNICODE_STRING *>(data);
  uintptr_t at = uintptr_t(u->Buffer), first = uintptr_t(data);
  if (u->Length % 2 || at < first || at - first > sizeof(data) ||
      u->Length > sizeof(data) - (at - first))
    fail("object-name-bounds");
  for (unsigned i = 0; i < u->Length / 2; i++)
    printf("%02X%02X", unsigned(u->Buffer[i] & 255),
           unsigned(u->Buffer[i] >> 8));
}
static void metadata(HANDLE h) {
  type(h);
  DWORD flags = 0;
  if (!GetHandleInformation(h, &flags))
    fail("handle-flags");
  SectionBasic b{};
  SIZE_T n = 0;
  LONG s = query_section(h, 0, &b, sizeof(b), &n);
  printf("%llX:Section:%08lX:%lu:%08lX:%zu:%llX:%08lX:%llX:",
         (unsigned long long)uintptr_t(h), grant(h), flags, ULONG(s), size_t(n),
         (unsigned long long)uintptr_t(b.base), b.attributes,
         (unsigned long long)b.size.QuadPart);
  object_name(h);
  printf(",");
}
static void protect(unsigned page, DWORD flags) {
  DWORD old;
  if (!VirtualProtect(memory + page * 4096, 4096, flags, &old))
    fail("protect");
}
static DWORD protection(unsigned page) {
  MEMORY_BASIC_INFORMATION m{};
  if (!VirtualQuery(memory + page * 4096, &m, sizeof(m)))
    fail("query-protection");
  return m.Protect;
}
static LONG invoke(HANDLE *out, ULONG access, OBJECT_ATTRIBUTES *attrs,
                   ULONG *exception) {
  __try {
    return open_section(out, access, attrs);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
    return LONG(*exception);
  }
}
static uintptr_t address(unsigned mode, unsigned page, unsigned local,
                         unsigned width) {
  switch (mode) {
  case 1:
    return 0;
  case 2:
    return 1;
  case 3:
    return uintptr_t(memory + local + 1);
  case 4:
    return uintptr_t(memory + local + 4);
  case 5:
  case 6:
  case 7:
  case 9:
    return uintptr_t(memory + page * 4096);
  case 8:
    return uintptr_t(memory + (page + 1) * 4096 - width / 2);
  case 10:
    return uintptr_t(memory + local + 2);
  case 11:
    return uintptr_t(memory + local + 3);
  default:
    return uintptr_t(memory + local);
  }
}
static void mode_protection(unsigned mode, unsigned page) {
  if (mode == 5)
    protect(page, PAGE_READWRITE | PAGE_GUARD);
  if (mode == 6)
    protect(page, PAGE_NOACCESS);
  if (mode == 7)
    protect(page, PAGE_READONLY);
  if (mode == 8)
    protect(page + 1, PAGE_NOACCESS);
  if (mode == 9)
    protect(page, PAGE_READONLY | PAGE_GUARD);
}
static void store(uintptr_t at, const void *p, size_t size) {
  if (at >= uintptr_t(memory) && at <= uintptr_t(memory + 40 * 4096) - size)
    memcpy(reinterpret_cast<void *>(at), p, size);
}
static void one(unsigned family, unsigned name, unsigned root, ULONG access,
                ULONG flags, unsigned ak = 0, unsigned ok = 0, unsigned nk = 0,
                unsigned bk = 0, ULONG alength = sizeof(OBJECT_ATTRIBUTES),
                int length = -1, int maximum = -1) {
  DWORD ignored;
  if (!VirtualProtect(memory, 40 * 4096, PAGE_READWRITE, &ignored))
    fail("unprotect");
  memset(memory, 0xA5, 40 * 4096);
  uintptr_t aa = address(ak, 0, 12 * 4096 + 128, sizeof(OBJECT_ATTRIBUTES));
  uintptr_t oa = address(ok, 2, 13 * 4096 + 128, sizeof(HANDLE));
  uintptr_t na = address(nk, 4, 14 * 4096 + 128, sizeof(UNICODE_STRING));
  uintptr_t ba = address(bk, 16, 20 * 4096 + 128, 32);
  UNICODE_STRING u{};
  u.Length = USHORT(length < 0 ? wcslen(names[name]) * 2 : length);
  u.MaximumLength = USHORT(maximum < 0 ? u.Length + 2 : maximum);
  u.Buffer = reinterpret_cast<PWSTR>(ba);
  OBJECT_ATTRIBUTES a{};
  a.Length = alength;
  a.RootDirectory = roots[root];
  a.ObjectName = reinterpret_cast<UNICODE_STRING *>(na);
  a.Attributes = flags;
  store(aa, &a, sizeof(a));
  store(na, &u, sizeof(u));
  store(ba, names[name], (wcslen(names[name]) + 1) * 2);
  mode_protection(ak, 0);
  mode_protection(ok, 2);
  mode_protection(nk, 4);
  mode_protection(bk, 16);
  auto before_entries = entries();
  auto before = sections(before_entries);
  if (!inputs_intact(before_entries))
    fail("input-before");
  ULONG exception = 0;
  LONG s = invoke(reinterpret_cast<HANDLE *>(oa), access,
                  reinterpret_cast<OBJECT_ATTRIBUTES *>(aa), &exception);
  DWORD p[8] = {protection(0), protection(1), protection(2),  protection(3),
                protection(4), protection(5), protection(16), protection(17)};
  auto after_entries = entries();
  if (!inputs_intact(after_entries))
    fail("input-after");
  if (!VirtualProtect(memory, 40 * 4096, PAGE_READWRITE, &ignored))
    fail("unprotect-after");
  uintptr_t result = 0;
  bool observed = oa >= uintptr_t(memory) &&
                  oa <= uintptr_t(memory + 40 * 4096) - sizeof(HANDLE);
  if (observed)
    memcpy(&result, reinterpret_cast<void *>(oa), sizeof(result));
  printf("case=%u family=%u width=%zu name=%u root=%u access=%08lX flags=%08lX "
         "attributes=%u output=%u unicode=%u buffer=%u attributes-length=%lu "
         "length=%u maximum=%u status=%08lX exception=%08lX observed=%u "
         "result=%llX "
         "protection=",
         count++, family, sizeof(HANDLE), name, root, access, flags, ak, ok, nk,
         bk, alength, unsigned(u.Length), unsigned(u.MaximumLength), ULONG(s),
         exception, unsigned(observed), (unsigned long long)result);
  for (auto f : p)
    printf("%lX,", f);
  printf(" output-bytes=");
  if (observed)
    for (unsigned i = 0; i < sizeof(HANDLE) + 8; i++)
      printf("%02X", unsigned(reinterpret_cast<unsigned char *>(oa)[i]));
  printf(" births=");
  unsigned births = 0;
  for (auto e : after_entries)
    if (e.type == section_type && !before.count(uintptr_t(e.h))) {
      metadata(e.h);
      births++;
    }
  for (auto e : after_entries)
    if (e.type == section_type && !before.count(uintptr_t(e.h)))
      close(e.h);
  auto restored = entries();
  bool exact = sections(restored) == before && inputs_intact(restored);
  printf(" birth-count=%u exact-section-and-input-handles=%u\n", births,
         unsigned(exact));
  if (!exact)
    fail("handle-restoration");
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto nt = GetModuleHandleW(L"ntdll.dll");
  open_section = Open(GetProcAddress(nt, "NtOpenSection"));
  open_directory = Open(GetProcAddress(nt, "NtOpenDirectoryObject"));
  open_link = Open(GetProcAddress(nt, "NtOpenSymbolicLinkObject"));
  query_process = Query(GetProcAddress(nt, "NtQueryInformationProcess"));
  query_object = Query(GetProcAddress(nt, "NtQueryObject"));
  query_section = QuerySection(GetProcAddress(nt, "NtQuerySection"));
  close_nt = Close(GetProcAddress(nt, "NtClose"));
  if (!open_section || !open_directory || !open_link || !query_process ||
      !query_object || !query_section || !close_nt)
    fail("exports");
  roots[1] = base(open_directory, L"\\", 0x02000000);
  roots[2] = base(open_directory, L"\\KnownDlls", 0x02000000);
  roots[3] = base(open_directory, L"\\KnownDlls32", 0x02000000);
  roots[4] = base(open_link, L"\\KnownDlls\\KnownDllPath", 1);
  roots[5] = base(open_section, L"\\KnownDlls\\ntdll.dll", 0x02000000);
  roots[6] = GetCurrentProcess();
  roots[7] = reinterpret_cast<HANDLE>(uintptr_t(0xABC));
  roots[8] = reinterpret_cast<HANDLE>(uintptr_t(roots[2]) | 3);
  roots[9] = base(open_directory, L"\\KnownDlls", 0x20000);
  roots[10] = base(open_directory, L"\\KnownDllsChpe32", 0x02000000);
  roots[11] = base(open_directory, L"\\KnownDlls32", 0x20000);
  roots[12] = base(open_directory, L"\\KnownDlls32", 1);
  roots[13] = reinterpret_cast<HANDLE>(uintptr_t(roots[3]) | 3);
  roots[14] = reinterpret_cast<HANDLE>(
      uintptr_t(roots[3]) |
      (sizeof(void *) == 8 ? 0x100000000ull : 0x40000000ull));
  for (unsigned i : {1u, 2u, 3u, 4u, 5u, 9u, 10u, 11u, 12u})
    inputs.insert(uintptr_t(roots[i]));
  for (auto e : entries())
    if (e.h == roots[5])
      section_type = e.type;
  if (!section_type)
    fail("section-type-index");
  printf("subject width=%zu section-type=%lu inputs=", sizeof(HANDLE),
         section_type);
  for (unsigned i : {1u, 2u, 3u, 4u, 5u, 9u, 10u, 11u, 12u})
    printf("%u:%llX:%08lX,", i, (unsigned long long)uintptr_t(roots[i]),
           grant(roots[i]));
  printf("\n");
  for (unsigned i : {1u, 2u, 3u, 4u, 5u, 9u, 10u, 11u, 12u}) {
    printf("root ordinal=%u name=", i);
    object_name(roots[i]);
    printf("\n");
  }
  memory = static_cast<unsigned char *>(VirtualAlloc(
      nullptr, 40 * 4096, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!memory)
    fail("allocate");
  for (ULONG access : masks)
    for (unsigned name : {0u, 2u, 3u, 4u, 5u, 6u, 7u, 17u, 18u})
      for (ULONG flags : {0u, 0x40u})
        one(0, name, 0, access, flags);
  for (unsigned root = 0; root < 15; root++)
    for (unsigned name = 0; name < sizeof(names) / sizeof(names[0]); name++)
      for (ULONG flags : {0u, 0x40u, 0x1000u, 0x1040u})
        one(1, name, root, 0xD, flags);
  std::vector<ULONG> flags = {0,     0x42,  0x60,   0x140,  0x240,
                              0x440, 0x840, 0x1040, 0x1FF2, 0xFFFFFFFF};
  for (unsigned bit = 0; bit < 32; bit++)
    flags.push_back(1u << bit);
  for (ULONG f : flags)
    for (unsigned name : {0u, 5u, 6u, 7u})
      for (ULONG access : {0ul, 0xDul, 0x02000000ul, 0xFFFFFFFFul})
        one(2, name, 0, access, f);
  for (unsigned ak = 0; ak < 10; ak++)
    for (unsigned ok = 0; ok < 12; ok++)
      for (unsigned nk : {0u, 1u, 2u, 5u, 6u, 8u})
        for (ULONG length : {0ul, ULONG(sizeof(OBJECT_ATTRIBUTES))})
          one(3, 0, 0, 0xD, 0x40, ak, ok, nk, 0, length);
  for (unsigned ok = 0; ok < 12; ok++)
    for (unsigned nk : {0u, 1u, 2u, 3u, 5u, 6u, 8u, 9u})
      for (unsigned bk : {0u, 1u, 2u, 3u, 5u, 6u, 8u, 9u})
        one(4, 0, 0, 0xD, 0x40, 0, ok, nk, bk);
  for (unsigned name : {0u, 5u, 8u, 10u})
    for (int length : {0, 1, 2, 46, 48, 50, 65532, 65534})
      for (int maximum : {0, 1, 2, 48, 65535})
        for (unsigned bk : {0u, 1u, 3u})
          one(5, name, 0, 0xD, 0x40, 0, 0, 0, bk, sizeof(OBJECT_ATTRIBUTES),
              length, maximum);
  for (unsigned i : {1u, 2u, 3u, 4u, 5u, 9u, 10u, 11u, 12u})
    close(roots[i]);
  if (!VirtualFree(memory, 0, MEM_RELEASE))
    fail("free");
  printf("complete cases=%u inputs-closed=%zu\n", count, inputs.size());
  return 0;
}
