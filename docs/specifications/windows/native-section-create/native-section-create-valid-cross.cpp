#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <vector>
#include <windows.h>
#include <winternl.h>

using CreateSection = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *,
                                    LARGE_INTEGER *, ULONG, ULONG, HANDLE);
using QuerySection = LONG(NTAPI *)(HANDLE, ULONG, void *, SIZE_T, SIZE_T *);
using QueryObject = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using Close = LONG(NTAPI *)(HANDLE);
struct Case {
  const char *group;
  long long maximum = 65536;
  ULONG access = 0xF001F;
  ULONG protection = PAGE_READWRITE;
  ULONG attributes = SEC_COMMIT;
  unsigned output = 0;
  unsigned maximum_pointer = 0;
  unsigned object = 0;
  intptr_t file = 0;
  ULONG object_flags = 0;
};
static void fail(const char *operation) {
  printf("failure=%s error=%lu\n", operation, GetLastError());
  ExitProcess(21);
}
static void dump(const char *name, const void *pointer, size_t bytes) {
  printf("bytes name=%s size=%zu hex=", name, bytes);
  auto data = static_cast<const unsigned char *>(pointer);
  for (size_t i = 0; i < bytes; ++i)
    printf("%02X", unsigned(data[i]));
  puts("");
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  HMODULE dll = GetModuleHandleW(L"ntdll.dll");
  auto create =
      reinterpret_cast<CreateSection>(GetProcAddress(dll, "NtCreateSection"));
  auto query =
      reinterpret_cast<QuerySection>(GetProcAddress(dll, "NtQuerySection"));
  auto query_object =
      reinterpret_cast<QueryObject>(GetProcAddress(dll, "NtQueryObject"));
  auto close = reinterpret_cast<Close>(GetProcAddress(dll, "NtClose"));
  if (!create || !query || !query_object || !close)
    fail("installed-exports");
  std::vector<Case> cases;
  for (long long size : {1LL, 4097LL, 65537LL}) {
    for (ULONG protection : {2u, 4u, 8u, 0x10u, 0x20u, 0x40u, 0x80u}) {
      for (ULONG base : {ULONG(SEC_COMMIT), ULONG(SEC_RESERVE)}) {
        for (unsigned flags = 0; flags < 8; ++flags) {
          Case c{"valid-cross"};
          c.maximum = size;
          c.protection = protection;
          c.attributes = base | ((flags & 1) ? 0x10000000u : 0) |
                         ((flags & 2) ? 0x40000000u : 0) |
                         ((flags & 4) ? 1u : 0);
          cases.push_back(c);
        }
      }
    }
  }
  const auto base_cases = cases;
  for (unsigned invalid = 0; invalid < 3; ++invalid) {
    for (auto c : base_cases) {
      if (invalid == 0)
        c.output = 1;
      if (invalid == 1)
        c.maximum_pointer = 2;
      if (invalid == 2)
        c.protection = 0;
      cases.push_back(c);
    }
  }
  unsigned ordinal = 0, closed = 0;
  for (const auto &c : cases) {
    alignas(16) unsigned char output[32];
    memset(output, 0xA5, sizeof(output));
    HANDLE *out = reinterpret_cast<HANDLE *>(output + (c.output == 3 ? 1 : 0));
    if (c.output == 1)
      out = nullptr;
    if (c.output == 2)
      out = reinterpret_cast<HANDLE *>(uintptr_t(1));
    alignas(16) unsigned char size_bytes[16];
    memset(size_bytes, 0xA5, sizeof(size_bytes));
    auto maximum = reinterpret_cast<LARGE_INTEGER *>(
        size_bytes + (c.maximum_pointer == 3 ? 1 : 0));
    memcpy(maximum, &c.maximum, sizeof(c.maximum));
    if (c.maximum_pointer == 1)
      maximum = nullptr;
    if (c.maximum_pointer == 2)
      maximum = reinterpret_cast<LARGE_INTEGER *>(uintptr_t(1));
    OBJECT_ATTRIBUTES object{};
    object.Length = sizeof(object);
    object.Attributes = c.object_flags;
    if (c.object == 2)
      object.Length = 0;
    if (c.object == 3)
      object.Attributes = 0xFFFFFFFF;
    OBJECT_ATTRIBUTES *op = c.object ? &object : nullptr;
    if (c.object == 4)
      op = reinterpret_cast<OBJECT_ATTRIBUTES *>(uintptr_t(1));
    LONG status = create(out, c.access, op, maximum, c.protection, c.attributes,
                         reinterpret_cast<HANDLE>(c.file));
    printf("case=%u group=%s width=%zu maximum=%lld access=%08lX "
           "protection=%08lX attributes=%08lX output=%u maximum-pointer=%u "
           "object=%u file=%llX object-flags=%08lX status=%08lX\n",
           ordinal++, c.group, sizeof(void *), c.maximum, c.access,
           c.protection, c.attributes, c.output, c.maximum_pointer, c.object,
           static_cast<unsigned long long>(uintptr_t(c.file)), c.object_flags,
           ULONG(status));
    dump("output", output, sizeof(output));
    dump("maximum", size_bytes, sizeof(size_bytes));
    if (status >= 0) {
      if (c.output == 1 || c.output == 2)
        fail("successful-invalid-output");
      HANDLE section = nullptr;
      memcpy(&section, out, sizeof(section));
      alignas(16) unsigned char basic[64];
      memset(basic, 0xA5, sizeof(basic));
      SIZE_T returned = SIZE_T(-1);
      LONG qs = query(section, 0, basic, sizeof(basic), &returned);
      printf("query status=%08lX returned=%zu\n", ULONG(qs), returned);
      dump("section-basic", basic, sizeof(basic));
      alignas(16) unsigned char rights[56]{};
      ULONG object_returned = 0;
      LONG os =
          query_object(section, 0, rights, sizeof(rights), &object_returned);
      ULONG granted = 0;
      memcpy(&granted, rights + 4, sizeof(granted));
      printf("grant status=%08lX returned=%lu rights=%08lX\n", ULONG(os),
             object_returned, granted);
      LONG cs = close(section);
      printf("close status=%08lX\n", ULONG(cs));
      if (cs < 0)
        fail("owned-handle-close");
      ++closed;
    }
  }
  printf("complete cases=%u closed-handles=%u\n", ordinal, closed);
  return 0;
}
