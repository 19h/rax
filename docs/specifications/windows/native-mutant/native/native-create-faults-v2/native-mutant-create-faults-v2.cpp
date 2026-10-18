#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>

using Create = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *, ULONG);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using Release = LONG(NTAPI *)(HANDLE, LONG *);
using Close = LONG(NTAPI *)(HANDLE);
static Create create_mutant;
static Close close_object;
static size_t page_size;

struct Span {
  BYTE *base;
  explicit Span()
      : base(static_cast<BYTE *>(VirtualAlloc(nullptr, 2 * page_size,
                                              MEM_RESERVE | MEM_COMMIT,
                                              PAGE_READWRITE))) {
    if (!base)
      ExitProcess(31);
  }
  void *prepare(unsigned mode) {
    DWORD previous;
    if (!VirtualProtect(base, 2 * page_size, PAGE_READWRITE, &previous))
      ExitProcess(32);
    memset(base, 0xA5, 2 * page_size);
    const DWORD protections[] = {PAGE_READONLY, PAGE_NOACCESS,
                                 PAGE_READWRITE | PAGE_GUARD,
                                 PAGE_READONLY | PAGE_GUARD};
    unsigned protection_mode = mode;
    if (mode >= 10 && mode <= 14)
      protection_mode = mode == 10 ? 4 : mode == 11 || mode == 13 ? 5 : 6;
    if (protection_mode >= 4 && protection_mode <= 7 &&
        !VirtualProtect(base + page_size, page_size,
                        protections[protection_mode - 4], &previous))
      ExitProcess(33);
    switch (mode) {
    case 0:
      return base + 32;
    case 1:
      return base + 33;
    case 2:
      return base + page_size - 4;
    case 3:
      return base + page_size - 1;
    case 4:
    case 5:
    case 6:
    case 7:
      return base + page_size;
    case 10:
    case 11:
    case 12:
      return base + page_size - 4;
    case 13:
    case 14:
      return base + page_size - 1;
    case 15:
      return reinterpret_cast<void *>(uintptr_t(-2));
    case 8:
      return nullptr;
    case 9:
      return reinterpret_cast<void *>(uintptr_t(1));
    default:
      ExitProcess(34);
    }
  }
  void print() {
    MEMORY_BASIC_INFORMATION info{};
    if (!VirtualQuery(base + page_size, &info, sizeof info))
      ExitProcess(35);
    printf("%08lX:", info.Protect);
    DWORD previous;
    if (!VirtualProtect(base, 2 * page_size, PAGE_READWRITE, &previous))
      ExitProcess(36);
    for (size_t origin : {size_t(32), page_size - 64, page_size})
      for (size_t i = 0; i < 64; ++i)
        printf("%02X", unsigned(base[origin + i]));
  }
};

static LONG create(HANDLE *out, ULONG access, OBJECT_ATTRIBUTES *attrs,
                   ULONG owner, ULONG *escaped) {
  __try {
    return create_mutant(out, access, attrs, owner);
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 20;
  SYSTEM_INFO info{};
  GetSystemInfo(&info);
  page_size = info.dwPageSize;
  auto module = GetModuleHandleW(L"ntdll.dll");
  create_mutant =
      reinterpret_cast<Create>(GetProcAddress(module, "NtCreateMutant"));
  close_object = reinterpret_cast<Close>(GetProcAddress(module, "NtClose"));
  if (!create_mutant || !close_object)
    return 21;
  DWORD before;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 22;
  Span output, attrs_area;
  printf("profile=%s pointer-bytes=%zu page-bytes=%zu pid=%lu tid=%lu\n",
         argv[1], sizeof(void *), page_size, GetCurrentProcessId(),
         GetCurrentThreadId());
  size_t cases = 0, created = 0, closed = 0;
  for (unsigned attr_case = 0; attr_case < 20; ++attr_case)
    for (ULONG owner : {0ul, 1ul, 0x100ul, 0x101ul})
      for (ULONG access : {0ul, 0x1F0001ul})
        for (unsigned out_mode = 0; out_mode < 20; ++out_mode) {
          void *out = output.prepare(out_mode < 16 ? out_mode : 0);
          auto storage = static_cast<BYTE *>(attrs_area.prepare(0));
          OBJECT_ATTRIBUTES attrs{};
          attrs.Length = sizeof attrs;
          UNICODE_STRING empty{};
          wchar_t empty_buffer = 0;
          empty.Buffer = &empty_buffer;
          wchar_t unrooted[] = L"AssistMutantFaultUnrooted";
          UNICODE_STRING text_name{};
          text_name.Length = sizeof(unrooted) - 2;
          text_name.MaximumLength = sizeof unrooted;
          text_name.Buffer = unrooted;
          unsigned attr_protection = 0;
          switch (attr_case) {
          case 2:
            attrs.Length = 0;
            break;
          case 3:
            attrs.Length = sizeof attrs - 1;
            break;
          case 4:
            attrs.Length = sizeof attrs + 1;
            break;
          case 7:
            attr_protection = 5;
            break;
          case 8:
            attr_protection = 6;
            break;
          case 9:
            attr_protection = 7;
            break;
          case 10:
            attrs.SecurityDescriptor = reinterpret_cast<void *>(uintptr_t(1));
            break;
          case 11:
            attrs.SecurityQualityOfService =
                reinterpret_cast<void *>(uintptr_t(1));
            break;
          case 12:
            attrs.ObjectName = &empty;
            break;
          case 13:
            attrs.RootDirectory = reinterpret_cast<HANDLE>(uintptr_t(0x1234));
            break;
          case 14:
            attrs.ObjectName = &text_name;
            break;
          case 15:
            attrs.Attributes = 1;
            break;
          case 16:
            attrs.Attributes = 0x2000;
            break;
          case 17:
            attrs.Attributes = 0x80;
            break;
          case 18:
            attrs.Attributes = 0x40;
            break;
          case 19:
            attrs.Attributes = 2;
            break;
          }
          auto attr_pointer = reinterpret_cast<OBJECT_ATTRIBUTES *>(
              storage + (attr_case == 5 ? 1 : 0));
          if (attr_protection) {
            DWORD previous;
            VirtualProtect(attrs_area.base + page_size, page_size,
                           PAGE_READWRITE, &previous);
            attr_pointer = reinterpret_cast<OBJECT_ATTRIBUTES *>(
                attrs_area.base + page_size);
            memcpy(attr_pointer, &attrs, sizeof attrs);
            const DWORD protections[] = {PAGE_NOACCESS,
                                         PAGE_READWRITE | PAGE_GUARD,
                                         PAGE_READONLY | PAGE_GUARD};
            if (!VirtualProtect(attr_pointer, page_size,
                                protections[attr_protection - 5], &previous))
              return 23;
          } else
            memcpy(attr_pointer, &attrs, sizeof attrs);
          if (attr_case == 0)
            attr_pointer = nullptr;
          if (attr_case == 6)
            attr_pointer = reinterpret_cast<OBJECT_ATTRIBUTES *>(uintptr_t(1));
          if (out_mode >= 16)
            out = storage + (out_mode - 16) * sizeof(void *);
          ULONG escaped = 0;
          LONG status = create(static_cast<HANDLE *>(out), access, attr_pointer,
                               owner, &escaped);
          printf("create case=%zu attrs=%u output=%u owner=%08lX access=%08lX "
                 "status=%08lX escaped=%08lX out-bytes=",
                 cases++, attr_case, out_mode, owner, access, ULONG(status),
                 escaped);
          output.print();
          printf(" attr-bytes=");
          attrs_area.print();
          puts("");
          if (status >= 0) {
            HANDLE handle = nullptr;
            memcpy(&handle, out, sizeof handle);
            if (!handle || handle == reinterpret_cast<HANDLE>(uintptr_t(-1)))
              return 24;
            printf("published case=%zu handle=%llX\n", cases - 1,
                   static_cast<unsigned long long>(uintptr_t(handle)));
            ++created;
            if (close_object(handle) != 0)
              return 25;
            ++closed;
          }
        }
  if (!VirtualFree(output.base, 0, MEM_RELEASE) ||
      !VirtualFree(attrs_area.base, 0, MEM_RELEASE))
    return 26;
  DWORD after;
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 27;
  printf("complete cases=%zu created=%zu closed=%zu before=%lu after=%lu\n",
         cases, created, closed, before, after);
  return before == after && created == closed ? 0 : 28;
}
