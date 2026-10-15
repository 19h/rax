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
  for (long long size :
       {0LL, 1LL, 4095LL, 4096LL, 4097LL, 65535LL, 65536LL, 65537LL, -1LL}) {
    Case c{"maximum"};
    c.maximum = size;
    cases.push_back(c);
  }
  for (ULONG protection : {0u, 1u, 2u, 3u, 4u, 8u, 0x10u, 0x20u, 0x40u, 0x80u,
                           0x100u, 0x104u, 0x204u, 0x404u}) {
    Case c{"protection"};
    c.protection = protection;
    cases.push_back(c);
  }
  for (unsigned bit = 0; bit < 32; ++bit) {
    Case c{"attribute-bit"};
    c.attributes = ULONG(1) << bit;
    cases.push_back(c);
  }
  for (ULONG attributes : {0u, 0x0C000000u, 0x09000000u, 0x08800000u,
                           0x18000000u, 0x48000000u, 0x88000000u}) {
    Case c{"attribute-combination"};
    c.attributes = attributes;
    cases.push_back(c);
  }
  for (ULONG access :
       {0u, 1u, 2u, 4u, 8u, 16u, 0x20000u, 0x100000u, 0xF001Fu, 0x01000000u,
        0x02000000u, 0x80000000u, 0x40000000u, 0x20000000u, 0x10000000u}) {
    Case c{"access"};
    c.access = access;
    cases.push_back(c);
  }
  for (unsigned mode : {1u, 2u, 3u}) {
    Case c{"output"};
    c.output = mode;
    cases.push_back(c);
    c.protection = 0;
    cases.push_back(c);
    c.protection = PAGE_READWRITE;
    c.maximum = 0;
    cases.push_back(c);
  }
  for (unsigned mode : {1u, 2u, 3u}) {
    Case c{"maximum-pointer"};
    c.maximum_pointer = mode;
    cases.push_back(c);
    c.protection = 0;
    cases.push_back(c);
  }
  for (unsigned mode : {1u, 2u, 3u, 4u}) {
    Case c{"object"};
    c.object = mode;
    cases.push_back(c);
    c.protection = 0;
    cases.push_back(c);
  }
  for (intptr_t handle : {intptr_t(-1), intptr_t(-2), intptr_t(0x12345678)}) {
    Case c{"file"};
    c.file = handle;
    cases.push_back(c);
  }
  for (unsigned bit = 0; bit < 32; ++bit) {
    for (ULONG base : {ULONG(SEC_COMMIT), ULONG(SEC_RESERVE)}) {
      Case c{"allocation-combination-bit"};
      c.attributes = base | (ULONG(1) << bit);
      cases.push_back(c);
    }
    Case access_case{"access-bit"};
    access_case.access = ULONG(1) << bit;
    cases.push_back(access_case);
    Case object_case{"object-flag-bit"};
    object_case.object = 1;
    object_case.object_flags = ULONG(1) << bit;
    cases.push_back(object_case);
  }
  const auto base_cases = cases;
  for (unsigned invalid = 0; invalid < 5; ++invalid) {
    for (auto c : base_cases) {
      switch (invalid) {
      case 0:
        c.protection = 0;
        break;
      case 1:
        c.attributes = 0;
        break;
      case 2:
        c.maximum = 0;
        break;
      case 3:
        c.maximum_pointer = 2;
        break;
      case 4:
        c.output = 1;
        break;
      }
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
