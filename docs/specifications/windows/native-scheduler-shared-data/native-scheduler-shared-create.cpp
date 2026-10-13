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
using Set = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG);
using Close = LONG(NTAPI *)(HANDLE);
using QueryProcess = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using QueryObject = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
static Set setter;
static Close closer;
static QueryProcess query_process;
static QueryObject query_object;
static unsigned char *memory;
static HANDLE processes[13];
struct HandleEntry {
  HANDLE handle;
  uintptr_t handle_count, pointer_count;
  ULONG access, type, attributes, reserved;
};
static void fail(const char *what) {
  printf("failure=%s error=%lu\n", what, GetLastError());
  ExitProcess(21);
}
static std::set<uintptr_t> handles() {
  std::vector<uintptr_t> data(131072);
  ULONG required = 0;
  auto status =
      query_process(GetCurrentProcess(), 51, data.data(),
                    ULONG(data.size() * sizeof(uintptr_t)), &required);
  if (status < 0 || data[0] > 8192 ||
      required > data.size() * sizeof(uintptr_t))
    fail("handle-snapshot");
  auto rows = reinterpret_cast<HandleEntry *>(data.data() + 2);
  std::set<uintptr_t> result;
  for (size_t i = 0; i < data[0]; ++i)
    result.insert(uintptr_t(rows[i].handle));
  return result;
}
static void protect(unsigned page, DWORD flags) {
  DWORD old = 0;
  if (!VirtualProtect(memory + page * 4096, 4096, flags, &old))
    fail("protect");
}
static LONG invoke(HANDLE process, void *output, ULONG length,
                   ULONG *exception) {
  __try {
    return setter(process, 112, output, length);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
    return LONG(*exception);
  }
}
static void dump(unsigned offset, unsigned length) {
  for (unsigned i = 0; i < length; ++i)
    printf("%02X", unsigned(memory[offset + i]));
}
static void one(unsigned ordinal, unsigned process, ULONG length,
                unsigned output_mode, unsigned content) {
  for (unsigned page = 0; page < 2; ++page)
    protect(page, PAGE_READWRITE);
  memset(memory, 0xA5, 8192);
  void *output = memory + 128;
  switch (output_mode) {
  case 1:
    output = nullptr;
    break;
  case 2:
    output = reinterpret_cast<void *>(uintptr_t(1));
    break;
  case 3:
    output = memory + 129;
    break;
  case 4:
    output = memory + 132;
    break;
  case 5:
  case 6:
  case 7:
  case 8:
    output = memory + 4096;
    break;
  case 9:
  case 10:
  case 11:
    output = memory + 4092;
    break;
  case 12:
    output = memory + 4088;
    break;
  case 13:
    output = reinterpret_cast<void *>(~uintptr_t(0) - 7);
    break;
  }
  if (output_mode != 1 && output_mode != 2 && output_mode != 13) {
    uint64_t value = content ? UINT64_MAX : 0;
    memcpy(output, &value, 8);
  }
  if (output_mode == 5 || output_mode == 11)
    protect(1, PAGE_READONLY);
  if (output_mode == 6 || output_mode == 9 || output_mode == 12)
    protect(1, PAGE_NOACCESS);
  if (output_mode == 7 || output_mode == 10)
    protect(1, PAGE_READWRITE | PAGE_GUARD);
  if (output_mode == 8)
    protect(1, PAGE_READONLY | PAGE_GUARD);
  auto before = handles();
  ULONG exception = 0;
  LONG status = invoke(processes[process], output, length, &exception);
  auto after = handles();
  DWORD flags[2]{};
  for (unsigned page = 0; page < 2; ++page) {
    MEMORY_BASIC_INFORMATION info{};
    if (!VirtualQuery(memory + page * 4096, &info, sizeof(info)))
      fail("virtual-query");
    flags[page] = info.Protect;
    protect(page, PAGE_READWRITE);
  }
  printf("case=%u width=%zu process=%u length=%08lX output=%u content=%u "
         "status=%08lX exception=%08lX protection=%08lX,%08lX, bytes0=",
         ordinal, sizeof(void *), process, length, output_mode, content, status,
         exception, flags[0], flags[1]);
  dump(120, 32);
  printf(" bytes1=");
  dump(4080, 48);
  printf(" born=");
  unsigned born = 0;
  for (uintptr_t handle : after) {
    if (before.contains(handle))
      continue;
    unsigned char basic[64]{};
    unsigned char type[512]{};
    ULONG returned = 0;
    ULONG basic_length = 56;
    LONG bs = query_object(reinterpret_cast<HANDLE>(handle), 0, basic,
                           basic_length, &returned);
    ULONG access = 0;
    memcpy(&access, basic + 4, 4);
    LONG ts = query_object(reinterpret_cast<HANDLE>(handle), 2, type,
                           sizeof(type), &returned);
    auto name = reinterpret_cast<UNICODE_STRING *>(type);
    bool correct = ts >= 0 && name->Length == 38 &&
                   memcmp(name->Buffer, L"SchedulerSharedData", 38) == 0;
    DWORD attributes = 0;
    BOOL valid =
        GetHandleInformation(reinterpret_cast<HANDLE>(handle), &attributes);
    printf("%llX:%08lX:%08lX:%u:%lu,", (unsigned long long)handle, bs, access,
           unsigned(correct && valid), attributes);
    if (bs < 0 || !correct || !valid ||
        closer(reinterpret_cast<HANDLE>(handle)) < 0)
      fail("inspect-close-owned");
    ++born;
  }
  auto final = handles();
  printf(" born-count=%u restored=%u handles-before=%zu handles-after=%zu\n",
         born, unsigned(final == before), before.size(), final.size());
  if (final != before)
    fail("exact-lifetime");
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto n = GetModuleHandleW(L"ntdll.dll");
  setter = reinterpret_cast<Set>(GetProcAddress(n, "NtSetInformationProcess"));
  closer = reinterpret_cast<Close>(GetProcAddress(n, "NtClose"));
  query_process = reinterpret_cast<QueryProcess>(
      GetProcAddress(n, "NtQueryInformationProcess"));
  query_object =
      reinterpret_cast<QueryObject>(GetProcAddress(n, "NtQueryObject"));
  memory = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 8192, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!setter || !closer || !query_process || !query_object || !memory)
    return 22;
  processes[0] = GetCurrentProcess();
  processes[1] = nullptr;
  processes[2] = reinterpret_cast<HANDLE>(intptr_t(-2));
  processes[3] = reinterpret_cast<HANDLE>(uintptr_t(0x1234));
  unsigned index = 4;
  for (DWORD access :
       {0u, 0x200u, 0x2000u, 0x400u, 0x1000u, 0x20u, 0x10u, 0x1FFFFFu}) {
    if (!DuplicateHandle(GetCurrentProcess(), GetCurrentProcess(),
                         GetCurrentProcess(), &processes[index], access, FALSE,
                         0))
      fail("duplicate-process");
    ++index;
  }
  processes[12] = CreateEventW(nullptr, FALSE, FALSE, nullptr);
  if (!processes[12])
    fail("event");
  auto before = handles();
  printf("host pointer=%zu memory=%p handles=%zu\n", sizeof(void *), memory,
         before.size());
  unsigned ordinal = 0;
  for (unsigned process = 0; process < 13; ++process)
    for (ULONG length :
         {0u, 1u, 3u, 4u, 7u, 8u, 9u, 12u, 16u, 32u, 0xFFFFFFFFu})
      for (unsigned output = 0; output < 14; ++output)
        for (unsigned content = 0; content < 2; ++content)
          one(ordinal++, process, length, output, content);
  if (handles() != before)
    fail("complete-lifetime");
  for (unsigned i = 4; i < 13; ++i)
    if (!CloseHandle(processes[i]))
      fail("close-process-input");
  if (!VirtualFree(memory, 0, MEM_RELEASE))
    fail("release");
  printf("complete cases=%u exact-handle-set=1 owned-input-handles-closed=9\n",
         ordinal);
  return 0;
}
