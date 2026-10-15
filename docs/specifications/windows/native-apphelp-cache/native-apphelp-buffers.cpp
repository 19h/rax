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

static ULONG protection(void *address) {
  MEMORY_BASIC_INFORMATION state{};
  if (!VirtualQuery(address, &state, sizeof state))
    fail("query");
  return state.Protect;
}

static bool readable(void *address) {
  ULONG flags = protection(address);
  return flags && !(flags & (PAGE_NOACCESS | PAGE_GUARD));
}

static void protect(void *address, ULONG flags) {
  ULONG old = 0;
  if (!VirtualProtect(address, 4096, flags, &old))
    fail("protect");
}

int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto call = reinterpret_cast<Lookup>(
      GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "NtApphelpCacheControl"));
  if (!call)
    fail("lookup-export");
  auto arena = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 65536, MEM_RESERVE, PAGE_NOACCESS));
  if (!arena || !VirtualAlloc(arena, 32768, MEM_COMMIT, PAGE_READWRITE))
    fail("owned-arena");
  unsigned name_offset = sizeof(void *) == 8 ? 248 : 132;
  unsigned out_offset = sizeof(void *) == 8 ? 376 : 196;
  const unsigned extents[] = {0,   1,   4,   132, 140, 196, 200, 204, 208, 248,
                              264, 376, 384, 388, 392, 396, 400, 408, 512};
  const ULONG sizes[] = {0, 1, 2, 3, 4, 5, 8, 16, 4096, 0xffffffff};
  const USHORT lengths[] = {0, 1, 2, 3, 9, 10, 11, 12, 22, 23, 0xfffe, 0xffff};
  const USHORT maxima[] = {0, 1, 2, 8, 10, 21, 22, 23, 24, 0xffff};
  const unsigned total =
      50 + sizeof extents / sizeof *extents + sizeof sizes / sizeof *sizes +
      sizeof lengths / sizeof *lengths + sizeof maxima / sizeof *maxima;
  for (unsigned mode = 0; mode < total; ++mode) {
    for (unsigned p = 0; p < 8; ++p)
      protect(arena + p * 4096, PAGE_READWRITE);
    memset(arena, 0xA5, 32768);
    auto input = arena + 128;
    UNICODE_STRING name{};
    name.Length = 22;
    name.MaximumLength = 24;
    name.Buffer = reinterpret_cast<WCHAR *>(arena + 16384);
    memcpy(name.Buffer, L"apphelp.dll", 24);
    uintptr_t output = uintptr_t(arena + 24576);
    ULONG size = 4;
    bool cut_input = false;
    if (mode >= 50) {
      unsigned index = mode - 50;
      if (index < sizeof extents / sizeof *extents) {
        input = arena + 4096 - extents[index];
        cut_input = true;
      } else if ((index -= sizeof extents / sizeof *extents) <
                 sizeof sizes / sizeof *sizes) {
        size = sizes[index];
      } else if ((index -= sizeof sizes / sizeof *sizes) <
                 sizeof lengths / sizeof *lengths) {
        name.Length = lengths[index];
      } else {
        index -= sizeof lengths / sizeof *lengths;
        name.MaximumLength = maxima[index];
      }
    }
    switch (mode) {
    case 1:
      input += 1;
      break;
    case 2:
      input += 2;
      break;
    case 3:
      input += 4;
      break;
    case 4:
      name.Buffer = nullptr;
      break;
    case 5:
      name.Buffer = reinterpret_cast<WCHAR *>(uintptr_t(1));
      break;
    case 6:
      name.Length = 0;
      name.Buffer = nullptr;
      break;
    case 7:
      name.Length = 0;
      output = 0;
      break;
    case 8:
      output = 0;
      break;
    case 9:
      output = 1;
      break;
    case 10:
      output += 1;
      break;
    case 11:
      output += 2;
      break;
    case 12:
      name.Buffer = reinterpret_cast<WCHAR *>(arena + 16385);
      break;
    case 13:
      name.Buffer = reinterpret_cast<WCHAR *>(arena + 16386);
      break;
    case 14:
      output = uintptr_t(input);
      break;
    case 15:
      output = uintptr_t(input + name_offset);
      break;
    case 16:
      output = uintptr_t(input + name_offset + (sizeof(void *) == 8 ? 8 : 4));
      break;
    case 17:
      output = uintptr_t(input + out_offset);
      break;
    case 18:
      output = uintptr_t(input + out_offset + sizeof(void *));
      break;
    case 19:
      output = uintptr_t(name.Buffer);
      break;
    case 20:
      name.Buffer = reinterpret_cast<WCHAR *>(input + 16);
      break;
    case 21:
      name.Buffer = reinterpret_cast<WCHAR *>(input + name_offset);
      break;
    case 22:
      name.Buffer = reinterpret_cast<WCHAR *>(input + out_offset);
      break;
    case 23:
      name.Buffer = reinterpret_cast<WCHAR *>(arena + 20480 - 22);
      break;
    case 24:
      name.Buffer = reinterpret_cast<WCHAR *>(arena + 20480 - 20);
      break;
    case 25:
      output = uintptr_t(arena + 28672 - 4);
      break;
    case 26:
      output = uintptr_t(arena + 28672 - 3);
      break;
    case 35:
      name.Length = 0;
      break;
    case 36:
      output = 0;
      break;
    case 37:
      size = 0;
      break;
    case 38:
      name.Buffer = nullptr;
      break;
    case 39:
      name.Length = 0;
      output = 0;
      break;
    case 40:
      name.Buffer = reinterpret_cast<WCHAR *>(uintptr_t(1));
      size = 0;
      break;
    case 41:
      name.Length = 0;
      size = 0;
      break;
    case 42:
      name.MaximumLength = 0;
      output = 0;
      break;
    case 43:
      output = uintptr_t(input + out_offset + sizeof(void *) + 4);
      break;
    case 44:
      size = 8;
      output = uintptr_t(arena + 28672 - 4);
      break;
    case 45:
      name.Length = 23;
      output = 0;
      break;
    case 46:
      name.Length = 23;
      name.Buffer = nullptr;
      break;
    case 47:
      name.MaximumLength = 1;
      name.Buffer = nullptr;
      break;
    case 48:
      name.Length = 0xffff;
      output = 0;
      break;
    case 49:
      name.Length = 22;
      name.MaximumLength = 0;
      name.Buffer = reinterpret_cast<WCHAR *>(uintptr_t(1));
      break;
    }
    memset(input, 0, 512);
    memcpy(input + name_offset, &name, sizeof name);
    memcpy(input + out_offset, &output, sizeof output);
    memcpy(input + out_offset + sizeof(void *), &size, 4);
    if (mode == 20)
      memcpy(input + 16, L"apphelp.dll", 24);
    if (mode == 23 || mode == 24)
      memcpy(name.Buffer, L"apphelp.dll", 24);
    unsigned char before[512];
    memcpy(before, input, sizeof before);
    if (cut_input)
      protect(arena + 4096, PAGE_NOACCESS);
    if (mode == 23 || mode == 24)
      protect(arena + 20480, PAGE_NOACCESS);
    if (mode == 25 || mode == 26 || mode == 44)
      protect(arena + 28672, PAGE_NOACCESS);
    if (mode >= 27 && mode <= 30)
      protect(arena, mode == 27   ? PAGE_READONLY
                     : mode == 28 ? PAGE_EXECUTE_READ
                     : mode == 29 ? PAGE_READWRITE | PAGE_GUARD
                                  : PAGE_READONLY | PAGE_GUARD);
    if (mode == 31 || mode == 32)
      protect(arena + 16384,
              mode == 31 ? PAGE_READONLY : PAGE_READWRITE | PAGE_GUARD);
    if (mode == 33 || mode == 34)
      protect(arena + 24576,
              mode == 33 ? PAGE_READONLY : PAGE_READWRITE | PAGE_GUARD);
    if (mode >= 35 && mode <= 39)
      protect(arena + 24576, PAGE_NOACCESS);
    ULONG escaped = 0;
    LONG status = invoke(call, input, &escaped);
    printf("case=%u width=%zu input=%zu name=%zu output=%zu length=%u "
           "maximum=%u size=%lu status=%08lX escaped=%08lX protections=",
           mode, sizeof(void *), size_t(input - arena),
           size_t(uintptr_t(name.Buffer) - uintptr_t(arena)),
           size_t(output - uintptr_t(arena)), name.Length, name.MaximumLength,
           size, ULONG(status), escaped);
    for (unsigned p = 0; p < 8; ++p)
      printf("%08lX,", protection(arena + p * 4096));
    printf(" input-readable=");
    for (unsigned i = 0; i < 512; ++i) {
      if (readable(input + i))
        printf("%02X", input[i]);
      else
        printf("--");
    }
    printf(" output-bytes=");
    for (unsigned i = 0; i < 8; ++i) {
      auto at = reinterpret_cast<unsigned char *>(output + i);
      if (readable(at))
        printf("%02X", *at);
      else
        printf("--");
    }
    printf(" changes=");
    for (unsigned i = 0; i < 512; ++i)
      if (readable(input + i) && before[i] != input[i])
        printf("%u:%02X>%02X,", i, before[i], input[i]);
    puts("");
  }
  printf("complete cases=%u operation=6\n", total);
}
