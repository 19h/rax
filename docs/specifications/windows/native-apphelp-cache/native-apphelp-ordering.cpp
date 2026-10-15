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
  const unsigned total = 36;
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
    switch (mode) {
    case 1:
      name.Buffer = reinterpret_cast<WCHAR *>(uintptr_t(1));
      output = 1;
      break;
    case 2:
      name.Buffer = reinterpret_cast<WCHAR *>(uintptr_t(1));
      output = 0;
      break;
    case 3:
      output = 1;
      break;
    case 5:
      name.Length = 0xfffe;
      name.Buffer = nullptr;
      break;
    case 6:
      name.Length = 0xfffe;
      output = 1;
      break;
    case 7:
      name.Length = 0xfffe;
      output = 0;
      break;
    case 8:
      name.Length = 0xfffe;
      name.Buffer = nullptr;
      size = 0;
      break;
    case 9:
      name.Length = 24;
      break;
    case 10:
      name.Length = 26;
      break;
    case 11:
      name.Length = 21;
      break;
    case 17:
      name.Length = 0xffff;
      break;
    case 18:
      output = uintptr_t(input + (sizeof(void *) == 8 ? 368 : 192));
      break;
    case 19:
      output = uintptr_t(input + (sizeof(void *) == 8 ? 372 : 196));
      break;
    case 21:
      size = 0;
      break;
    case 22:
      name.Buffer = reinterpret_cast<WCHAR *>(uintptr_t(1));
      break;
    case 23:
      name.Length = 0;
      break;
    case 24:
      input = arena + 4096 - 192;
      break;
    case 25:
      input = arena + 4096 - 196;
      break;
    case 26:
      input = arena + 4096 - 200;
      break;
    case 27:
      input = arena + 4096 - 204;
      break;
    case 28:
      input = arena + 4096 - 192;
      break;
    case 29:
      input = arena + 4096 - 196;
      break;
    case 30:
      input = arena + 4096 - 200;
      break;
    case 31:
      input = arena + 4096 - 204;
      break;
    case 32:
      size = 0;
      break;
    case 33:
      name.Buffer = reinterpret_cast<WCHAR *>(uintptr_t(1));
      break;
    case 34:
      name.Length = 0;
      break;
    case 35:
      name.Length = 0xfffe;
      output = 1;
      break;
    }
    memset(input, mode == 20 ? 0xA5 : 0, 512);
    memcpy(input + name_offset, &name, sizeof name);
    memcpy(input + out_offset, &output, sizeof output);
    memcpy(input + out_offset + sizeof(void *), &size, 4);
    if (mode == 12)
      memcpy(arena + 16384, L"APPHELP.DLL", 24);
    if (mode == 13)
      memset(arena + 16388, 0, 2);
    if (mode == 14)
      memset(arena + 16384, 0, 2);
    if (mode == 15) {
      USHORT odd = 0xD800;
      memcpy(arena + 16384, &odd, 2);
    }
    if (mode == 16) {
      memset(arena + 16384, 0, 24);
    }
    unsigned char before[512];
    memcpy(before, input, sizeof before);
    if (mode == 3 || mode == 4 || mode == 32)
      protect(arena + 16384, PAGE_READWRITE | PAGE_GUARD);
    if (mode == 4 || mode == 17)
      protect(arena + 24576, PAGE_READWRITE | PAGE_GUARD);
    if ((mode >= 21 && mode <= 27))
      protect(arena, PAGE_READONLY);
    if (mode >= 28 && mode <= 31)
      protect(arena + 4096, PAGE_READONLY);
    if (mode >= 32 && mode <= 35)
      protect(arena, PAGE_READONLY);
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
