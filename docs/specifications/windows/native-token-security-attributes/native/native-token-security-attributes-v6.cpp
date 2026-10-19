#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cwchar>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>

using Query = LONG(NTAPI *)(HANDLE, const UNICODE_STRING *, ULONG, PVOID, ULONG,
                            PULONG);
static Query query_attributes;
static HANDLE token, denied, event;
static DWORD page;
static unsigned ordinal;

static LONG invoke(HANDLE handle, const UNICODE_STRING *names, ULONG count,
                   void *output, ULONG length, ULONG *required,
                   ULONG *exception) {
  LONG status = LONG(0xDEADBEEF);
  *exception = 0;
  __try {
    status = query_attributes(handle, names, count, output, length, required);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
  }
  return status;
}

static DWORD protection(void *address) {
  MEMORY_BASIC_INFORMATION info{};
  return VirtualQuery(address, &info, sizeof info) ? info.Protect : 0;
}

static bool peek(void *address, ULONG *value) {
  __try {
    memcpy(value, address, 4);
    return true;
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    return false;
  }
}

static void protect(void *address, SIZE_T bytes, DWORD value) {
  DWORD old = 0;
  if (!VirtualProtect(address, bytes, value, &old))
    ExitProcess(20);
}

static unsigned char *allocate(unsigned pages) {
  auto result = static_cast<unsigned char *>(VirtualAlloc(
      nullptr, SIZE_T(pages) * page, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!result)
    ExitProcess(21);
  memset(result, 0xA5, SIZE_T(pages) * page);
  return result;
}

static void run(const char *label, unsigned names_mode, unsigned handle_mode,
                unsigned output_mode, unsigned returned_mode, ULONG length,
                unsigned remaining = 4) {
  DWORD before = 0, after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    ExitProcess(22);
  auto input = allocate(4);
  auto output_storage = allocate(2);
  auto return_storage = allocate(2);
  auto text_a = reinterpret_cast<wchar_t *>(input + 2 * page + 64);
  auto text_b = reinterpret_cast<wchar_t *>(input + 2 * page + 128);
  const wchar_t *first = (names_mode == 2 || names_mode == 20)
                             ? L"TSA://ProcUnique"
                             : L"WIN://SYSAPPID";
  wcscpy_s(text_a, 64, first);
  wcscpy_s(text_b, 64, names_mode == 20 ? L"TSA://ProcUnique" : L"WIN://PKG");
  UNICODE_STRING descriptors[2]{};
  descriptors[0].Buffer = text_a;
  descriptors[1].Buffer = text_b;
  for (auto &descriptor : descriptors) {
    descriptor.Length = USHORT(wcslen(descriptor.Buffer) * 2);
    descriptor.MaximumLength = descriptor.Length + 2;
  }
  ULONG count = names_mode == 1 || names_mode == 19 ? 0
                : names_mode == 2                   ? 1
                                                    : 2;
  void *names = input;
  if (names_mode == 1 || names_mode == 3)
    names = nullptr;
  if (names_mode == 4 || names_mode == 19)
    names = reinterpret_cast<void *>(uintptr_t(1));
  if (names_mode == 5)
    names = input + 1;
  if (names_mode == 8)
    descriptors[0].Buffer = nullptr;
  if (names_mode == 9)
    descriptors[0].Buffer = reinterpret_cast<wchar_t *>(uintptr_t(1));
  if (names_mode == 10) {
    descriptors[0].Buffer = reinterpret_cast<wchar_t *>(input + 2 * page + 65);
    memcpy(descriptors[0].Buffer, first, descriptors[0].Length);
  }
  if (names_mode == 11)
    --descriptors[0].Length;
  if (names_mode == 12)
    descriptors[0].MaximumLength = 0;
  if (names_mode == 13)
    descriptors[0].Length = 0;
  if (names_mode == 16)
    names = input + page - 4;
  if (names_mode == 17) {
    descriptors[0].Buffer = reinterpret_cast<wchar_t *>(input + 3 * page - 2);
    memcpy(descriptors[0].Buffer, first, descriptors[0].Length);
  }
  if (names != nullptr && names != reinterpret_cast<void *>(uintptr_t(1)))
    memcpy(names, descriptors, sizeof descriptors);
  if (names_mode == 6)
    protect(input, page, PAGE_NOACCESS);
  if (names_mode == 7)
    protect(input, page, PAGE_READWRITE | PAGE_GUARD);
  if (names_mode == 14)
    protect(input + 2 * page, page, PAGE_READWRITE | PAGE_GUARD);
  if (names_mode == 15)
    protect(input + 2 * page, page, PAGE_NOACCESS);
  if (names_mode == 16)
    protect(input + page, page, PAGE_NOACCESS);
  if (names_mode == 17)
    protect(input + 3 * page, page, PAGE_NOACCESS);

  void *output = output_storage;
  if (output_mode == 1)
    output = nullptr;
  if (output_mode == 2)
    output = reinterpret_cast<void *>(uintptr_t(1));
  if (output_mode == 6)
    output = output_storage + 1;
  if (output_mode >= 7 && output_mode <= 11)
    output = output_storage + page - remaining;
  if (output_mode == 12)
    output = output_storage + 4;
  if (output_mode == 13)
    output = output_storage + 2;
  if (output_mode == 14)
    output = output_storage + 8;
  ULONG initial = 0x11223344;
  ULONG local_return = initial;
  void *returned = &local_return;
  if (returned_mode == 1)
    returned = nullptr;
  if (returned_mode == 2)
    returned = reinterpret_cast<void *>(uintptr_t(1));
  if (returned_mode >= 3 && returned_mode <= 6)
    returned = return_storage + (returned_mode == 6 ? 1 : 0);
  if (returned_mode == 7)
    returned = output;
  if (returned_mode == 8)
    returned = static_cast<unsigned char *>(output) + 4;
  if (returned_mode == 9)
    returned = return_storage + page - 2;
  if (returned_mode >= 3 && returned_mode <= 6 || returned_mode == 9)
    memcpy(returned, &initial, 4);
  if ((returned_mode == 7 || returned_mode == 8) && output == output_storage)
    memcpy(returned, &initial, 4);
  if (output_mode == 3)
    protect(output_storage, page, PAGE_READONLY);
  if (output_mode == 4)
    protect(output_storage, page, PAGE_NOACCESS);
  if (output_mode == 5 || output_mode == 11)
    protect(output_storage, page, PAGE_READWRITE | PAGE_GUARD);
  if (output_mode == 8 || output_mode == 11)
    protect(output_storage + page, page, PAGE_NOACCESS);
  if (output_mode == 9)
    protect(output_storage, page, PAGE_READONLY | PAGE_GUARD);
  if (output_mode == 10)
    protect(output_storage + page, page, PAGE_READWRITE | PAGE_GUARD);
  if (returned_mode == 3)
    protect(return_storage, page, PAGE_READONLY);
  if (returned_mode == 4)
    protect(return_storage, page, PAGE_NOACCESS);
  if (returned_mode == 5)
    protect(return_storage, page, PAGE_READWRITE | PAGE_GUARD);
  if (returned_mode == 9)
    protect(return_storage + page, page, PAGE_NOACCESS);

  HANDLE handle = reinterpret_cast<HANDLE>(intptr_t(-4));
  switch (handle_mode) {
  case 1:
    handle = token;
    break;
  case 2:
    handle = nullptr;
    break;
  case 3:
    handle = reinterpret_cast<HANDLE>(uintptr_t(0x13572468));
    break;
  case 4:
    handle = event;
    break;
  case 5:
    handle = denied;
    break;
  case 6:
    handle = GetCurrentProcess();
    break;
  case 7:
    handle = GetCurrentThread();
    break;
  case 8:
    handle = reinterpret_cast<HANDLE>(intptr_t(-5));
    break;
  case 9:
    handle = reinterpret_cast<HANDLE>(intptr_t(-6));
    break;
  }
  ULONG exception = 0;
  SetLastError(0xC0DE);
  LONG status =
      invoke(handle, static_cast<UNICODE_STRING *>(names), count, output,
             length, static_cast<PULONG>(returned), &exception);
  DWORD last_error = GetLastError();
  DWORD input_protect[4], output_protect[2], return_protect[2];
  for (unsigned i = 0; i < 4; ++i)
    input_protect[i] = protection(input + i * page);
  for (unsigned i = 0; i < 2; ++i) {
    output_protect[i] = protection(output_storage + i * page);
    return_protect[i] = protection(return_storage + i * page);
  }
  protect(input, 4 * page, PAGE_READWRITE);
  protect(output_storage, 2 * page, PAGE_READWRITE);
  protect(return_storage, 2 * page, PAGE_READWRITE);
  ULONG returned_value = initial;
  bool readable = peek(returned, &returned_value);
  bool valid_output = uintptr_t(output) >= uintptr_t(output_storage) &&
                      uintptr_t(output) < uintptr_t(output_storage + 2 * page);
  const unsigned char *dump =
      valid_output ? static_cast<unsigned char *>(output) : output_storage;
  printf(
      "case=%u label=%s width=%zu names-mode=%u handle-mode=%u output-mode=%u "
      "returned-mode=%u remaining=%u count=%lu length=%08lX status=%08lX "
      "exception=%08lX "
      "returned=%08lX readable=%u last-error=%08lX output=%p input=%p "
      "out-base=%p ret-base=%p out-protect=%08lX,%08lX ret-protect=%08lX,%08lX "
      "input-protect=%08lX,%08lX,%08lX,%08lX bytes=",
      ordinal++, label, sizeof(void *), names_mode, handle_mode, output_mode,
      returned_mode, remaining, count, length, ULONG(status), exception,
      returned_value, unsigned(readable), last_error, output, input,
      output_storage, return_storage, output_protect[0], output_protect[1],
      return_protect[0], return_protect[1], input_protect[0], input_protect[1],
      input_protect[2], input_protect[3]);
  for (unsigned i = 0; i < 128; ++i)
    printf("%02X", dump[i]);
  puts("");
  if (!VirtualFree(input, 0, MEM_RELEASE) ||
      !VirtualFree(output_storage, 0, MEM_RELEASE) ||
      !VirtualFree(return_storage, 0, MEM_RELEASE) ||
      !GetProcessHandleCount(GetCurrentProcess(), &after) || before != after)
    ExitProcess(23);
}

int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  SYSTEM_INFO info{};
  GetSystemInfo(&info);
  page = info.dwPageSize;
  query_attributes = reinterpret_cast<Query>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtQuerySecurityAttributesToken"));
  if (!query_attributes ||
      !OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token) ||
      !DuplicateHandle(GetCurrentProcess(), token, GetCurrentProcess(), &denied,
                       0, FALSE, 0) ||
      !(event = CreateEventW(nullptr, TRUE, FALSE, nullptr)))
    return 2;
  for (unsigned remaining :
       {1u,   2u,    3u,    4u,    8u,    12u,   15u,   16u,   17u,
        24u,  31u,   32u,   33u,   63u,   64u,   65u,   95u,   96u,
        97u,  127u,  128u,  129u,  255u,  256u,  257u,  511u,  512u,
        513u, 1023u, 1024u, 1025u, 2047u, 2048u, 2049u, 4095u, 4096u})
    for (unsigned nm : {2u, 20u})
      for (unsigned om : {8u, 9u, 10u, 11u})
        for (ULONG length : {104ul, 192ul, 816ul, 4096ul})
          run("copy-boundary", nm, 0, om, 0, length, remaining);
  if (!CloseHandle(event) || !CloseHandle(denied) || !CloseHandle(token))
    return 3;
  printf("complete cases=%u\n", ordinal);
  return 0;
}
