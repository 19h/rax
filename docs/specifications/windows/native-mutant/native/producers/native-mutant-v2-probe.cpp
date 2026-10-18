#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#include <windows.h>
#include <winternl.h>

using Create = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *, ULONG);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using Release = LONG(NTAPI *)(HANDLE, LONG *);
using Wait = LONG(NTAPI *)(HANDLE, BOOLEAN, LARGE_INTEGER *);
using Close = LONG(NTAPI *)(HANDLE);
using OpenDirectory = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);

static_assert(sizeof(PUBLIC_OBJECT_BASIC_INFORMATION) == 56);

static Create create_mutant;
static Query query_mutant, query_object;
static Release release_mutant;
static Wait wait_object;
static Close close_object;
static size_t created_count, closed_count, ordinal;

static LONG invoke(Create function, HANDLE *out, ULONG access,
                   OBJECT_ATTRIBUTES *attrs, ULONG owner, ULONG *escaped) {
  __try {
    return function(out, access, attrs, owner);
  } __except ((*escaped = GetExceptionCode()), EXCEPTION_EXECUTE_HANDLER) {
    return LONG(0xDEADBEEF);
  }
}

static void bytes(const void *source, size_t count) {
  const auto data = static_cast<const BYTE *>(source);
  for (size_t i = 0; i < count; ++i)
    printf("%02X", unsigned(data[i]));
}

static void state(HANDLE handle, const char *stage, size_t case_ordinal) {
  alignas(16) BYTE buffer[64];
  memset(buffer, 0xA5, sizeof buffer);
  ULONG returned = 0xA5A5A5A5;
  LONG status = query_mutant(handle, 0, buffer, 8, &returned);
  printf("state ordinal=%zu stage=%s class=0 status=%08lX returned=%08lX data=",
         case_ordinal, stage, ULONG(status), returned);
  bytes(buffer, 16);
  puts("");
  memset(buffer, 0xA5, sizeof buffer);
  returned = 0xA5A5A5A5;
  status =
      query_mutant(handle, 1, buffer, ULONG(2 * sizeof(void *)), &returned);
  printf("state ordinal=%zu stage=%s class=1 status=%08lX returned=%08lX data=",
         case_ordinal, stage, ULONG(status), returned);
  bytes(buffer, 2 * sizeof(void *));
  puts("");
}

static HANDLE run(const char *label, ULONG access, ULONG owner,
                  OBJECT_ATTRIBUTES *attrs, bool keep = false,
                  size_t output_offset = 0) {
  alignas(16) BYTE storage[32];
  memset(storage, 0xA5, sizeof storage);
  ULONG escaped = 0;
  auto out = reinterpret_cast<HANDLE *>(storage + output_offset);
  const LONG status =
      invoke(create_mutant, out, access, attrs, owner, &escaped);
  HANDLE handle = nullptr;
  memcpy(&handle, storage + output_offset, sizeof handle);
  printf("create ordinal=%zu label=%s access=%08lX owner=%08lX attrs=%u "
         "flags=%08lX offset=%zu status=%08lX escaped=%08lX output=",
         ordinal, label, access, owner, unsigned(attrs != nullptr),
         attrs ? attrs->Attributes : 0, output_offset, ULONG(status), escaped);
  bytes(storage, sizeof storage);
  puts("");
  if (status >= 0) {
    if (!handle || escaped)
      ExitProcess(30);
    ++created_count;
    alignas(16) BYTE basic[128];
    memset(basic, 0xA5, sizeof basic);
    ULONG returned = 0xA5A5A5A5;
    const auto query_status = query_object(
        handle, 0, basic, sizeof(PUBLIC_OBJECT_BASIC_INFORMATION), &returned);
    ULONG attributes = 0, granted = 0;
    memcpy(&attributes, basic, 4);
    memcpy(&granted, basic + 4, 4);
    printf("object ordinal=%zu status=%08lX returned=%08lX attributes=%08lX "
           "granted=%08lX\n",
           ordinal, ULONG(query_status), returned, attributes, granted);
    state(handle, "created", ordinal);
    if (!keep) {
      if (close_object(handle) != 0)
        ExitProcess(31);
      ++closed_count;
    }
  }
  ++ordinal;
  return status >= 0 && keep ? handle : nullptr;
}

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 21;
  const auto module = GetModuleHandleW(L"ntdll.dll");
  create_mutant =
      reinterpret_cast<Create>(GetProcAddress(module, "NtCreateMutant"));
  query_mutant =
      reinterpret_cast<Query>(GetProcAddress(module, "NtQueryMutant"));
  query_object =
      reinterpret_cast<Query>(GetProcAddress(module, "NtQueryObject"));
  release_mutant =
      reinterpret_cast<Release>(GetProcAddress(module, "NtReleaseMutant"));
  wait_object =
      reinterpret_cast<Wait>(GetProcAddress(module, "NtWaitForSingleObject"));
  close_object = reinterpret_cast<Close>(GetProcAddress(module, "NtClose"));
  const auto open_directory = reinterpret_cast<OpenDirectory>(
      GetProcAddress(module, "NtOpenDirectoryObject"));
  if (!create_mutant || !query_mutant || !query_object || !release_mutant ||
      !wait_object || !close_object || !open_directory)
    return 22;
  printf("profile=%s pointer-bytes=%zu pid=%lu tid=%lu\n", argv[1],
         sizeof(void *), GetCurrentProcessId(), GetCurrentThreadId());
  const ULONG accesses[] = {0,          1,          2,          3,
                            0x1F0001,   0x1F0003,   0x02000000, 0x80000000,
                            0x40000000, 0x20000000, 0x10000000, 0x01000000,
                            0x00200000, 0xFFFFFFFF};
  const ULONG owners[] = {0, 1, 2, 0xFF, 0x100, 0x101, 0xFFFFFFFF};
  OBJECT_ATTRIBUTES unnamed{};
  unnamed.Length = sizeof unnamed;
  for (ULONG access : accesses)
    for (ULONG owner : owners) {
      run("null", access, owner, nullptr);
      run("unnamed", access, owner, &unnamed);
    }
  // OBJ_PERMANENT is excluded: no probe may leave a persistent kernel object.
  const ULONG flags[] = {0,     1,     2,     0x20,  0x40,   0x80,
                         0x100, 0x200, 0x400, 0x800, 0x1000, 0x2000};
  for (ULONG flag : flags) {
    unnamed.Attributes = flag;
    run("flags", 0x1F0001, 0, &unnamed);
  }
  unnamed.Attributes = 0;
  for (size_t offset = 0; offset < 8; ++offset)
    run("output-alignment", 0x1F0001, 0, &unnamed, false, offset);
  for (ULONG owner : {0ul, 1ul, 2ul, 0x100ul}) {
    HANDLE handle = run("lifecycle", 0x1F0001, owner, nullptr, true);
    if (!handle)
      return 23;
    for (unsigned iteration = 0; iteration < 2; ++iteration) {
      LARGE_INTEGER timeout{};
      const auto wait_status = wait_object(handle, FALSE, &timeout);
      printf("wait ordinal=%zu iteration=%u status=%08lX\n", ordinal - 1,
             iteration, ULONG(wait_status));
      state(handle, "waited", ordinal - 1);
    }
    for (unsigned iteration = 0; iteration < 4; ++iteration) {
      LONG previous = LONG(0xA5A5A5A5);
      const auto release_status = release_mutant(handle, &previous);
      printf("release ordinal=%zu iteration=%u status=%08lX previous=%08lX\n",
             ordinal - 1, iteration, ULONG(release_status), ULONG(previous));
      state(handle, "released", ordinal - 1);
    }
    if (close_object(handle) != 0)
      return 24;
    ++closed_count;
  }
  std::wstring root_name = L"\\BaseNamedObjects";
  UNICODE_STRING root_string{};
  root_string.Length = USHORT(root_name.size() * 2);
  root_string.MaximumLength = root_string.Length;
  root_string.Buffer = root_name.data();
  OBJECT_ATTRIBUTES attrs{};
  attrs.Length = sizeof attrs;
  attrs.Attributes = 0x40;
  attrs.ObjectName = &root_string;
  HANDLE root = nullptr;
  if (open_directory(&root, 3, &attrs) != 0)
    return 25;
  wchar_t unique[160];
  swprintf_s(unique, L"Local\\AssistMutantProof_%lu_%llu",
             GetCurrentProcessId(),
             static_cast<unsigned long long>(GetTickCount64()));
  UNICODE_STRING name{};
  name.Length = USHORT(wcslen(unique) * 2);
  name.MaximumLength = name.Length;
  name.Buffer = unique;
  attrs.RootDirectory = root;
  attrs.ObjectName = &name;
  attrs.Attributes = 0;
  HANDLE first = run("named-first", 0x1F0001, 0, &attrs, true);
  if (!first)
    return 26;
  run("named-collision", 0x1F0001, 1, &attrs);
  attrs.Attributes = 0x80;
  HANDLE second = run("named-openif", 0x1F0001, 1, &attrs, true);
  if (!second)
    return 27;
  state(first, "first-after-openif", ordinal - 3);
  if (close_object(second) != 0 || close_object(first) != 0 ||
      close_object(root) != 0)
    return 28;
  closed_count += 2;
  printf("complete cases=%zu created=%zu closed=%zu directory-closed=1\n",
         ordinal, created_count, closed_count);
  return created_count == closed_count ? 0 : 29;
}
