#include <windows.h>

#include <tlhelp32.h>
#include <winternl.h>

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>

using QueryProcess = NTSTATUS(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);
using QueryObject = NTSTATUS(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);
static unsigned calls;

static DWORD invoke(QueryProcess query, HANDLE handle, void *output,
                    ULONG length, ULONG *returned, ULONG *status) {
  __try {
    *status = static_cast<ULONG>(query(handle, 0, output, length, returned));
    return 0;
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    return GetExceptionCode();
  }
}

static void row(QueryProcess query, const char *kind, HANDLE handle,
                ULONG grant, ULONG length, uintptr_t input_size, bool seed_size,
                bool want_returned) {
  alignas(16) unsigned char output[128];
  std::memset(output, 0xA5, sizeof(output));
  if (seed_size)
    std::memcpy(output, &input_size, sizeof(input_size));
  ULONG returned = 0xA5A5A5A5, status = 0xCCCCCCCC;
  SetLastError(0x12345678);
  DWORD exception = invoke(query, handle, output, length,
                           want_returned ? &returned : nullptr, &status);
  DWORD error = GetLastError();
  std::printf("QUERY kind=%s grant=%08lX length=%lu seeded=%u input-size=%llX "
              "returned-pointer=%u status=%08lX exception=%08lX returned=%08lX "
              "last-error=%08lX bytes=",
              kind, grant, length, seed_size ? 1u : 0u,
              static_cast<unsigned long long>(input_size),
              want_returned ? 1u : 0u, status, exception, returned, error);
  for (unsigned char value : output)
    std::printf("%02X", value);
  std::printf("\n");
  ++calls;
}

static uintptr_t output_pointer(unsigned kind, unsigned char *middle,
                                ULONG basic, ULONG extended) {
  switch (kind) {
  case 0:
    return reinterpret_cast<uintptr_t>(middle + 16);
  case 1:
    return 0;
  case 2:
    return 1;
  case 3:
    return reinterpret_cast<uintptr_t>(middle + 17);
  case 4:
    return reinterpret_cast<uintptr_t>(middle + 20);
  case 5:
    return reinterpret_cast<uintptr_t>(middle + 4096 - basic);
  case 6:
    return reinterpret_cast<uintptr_t>(middle + 4096 - basic + 4);
  case 7:
    return reinterpret_cast<uintptr_t>(middle + 4096 - extended + 4);
  default:
    return sizeof(void *) == 8 ? uintptr_t(0x7FFFFFFFFFF0ULL)
                               : uintptr_t(0xFFFFFFF0);
  }
}

static uintptr_t returned_pointer(unsigned kind, uintptr_t output,
                                  unsigned char *middle, ULONG basic) {
  switch (kind) {
  case 0:
    return 0;
  case 1:
    return reinterpret_cast<uintptr_t>(middle + 16);
  case 2:
    return reinterpret_cast<uintptr_t>(middle + 17);
  case 3:
    return 1;
  case 4:
    return output;
  case 5:
    return output + 4;
  case 6:
    return output + 8;
  case 7:
    return output + basic - 4;
  case 8:
    return reinterpret_cast<uintptr_t>(middle + 4094);
  default:
    return sizeof(void *) == 8 ? uintptr_t(0x7FFFFFFFFFF0ULL)
                               : uintptr_t(0xFFFFFFF0);
  }
}

static void dump(const unsigned char *data, unsigned count) {
  for (unsigned i = 0; i < count; ++i)
    std::printf("%02X", data[i]);
}

static bool fault_row(QueryProcess query, unsigned id, unsigned pattern,
                      unsigned handle_kind, HANDLE event, ULONG length,
                      unsigned out_kind, unsigned ret_kind, DWORD out_protect,
                      DWORD ret_protect) {
  constexpr ULONG basic = sizeof(PROCESS_BASIC_INFORMATION),
                  extended = 8 * sizeof(void *);
  auto out_all = static_cast<unsigned char *>(VirtualAlloc(
      nullptr, 3 * 4096, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  auto ret_all = static_cast<unsigned char *>(VirtualAlloc(
      nullptr, 3 * 4096, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!out_all || !ret_all) {
    if (out_all)
      VirtualFree(out_all, 0, MEM_RELEASE);
    if (ret_all)
      VirtualFree(ret_all, 0, MEM_RELEASE);
    return false;
  }
  auto out = out_all + 4096, ret = ret_all + 4096;
  std::memset(out, pattern, 4096);
  std::memset(ret, pattern, 4096);
  DWORD old;
  bool protected_ok =
      VirtualProtect(out_all, 4096, PAGE_NOACCESS, &old) &&
      VirtualProtect(out_all + 8192, 4096, PAGE_NOACCESS, &old) &&
      VirtualProtect(ret_all, 4096, PAGE_NOACCESS, &old) &&
      VirtualProtect(ret_all + 8192, 4096, PAGE_NOACCESS, &old) &&
      VirtualProtect(out, 4096, out_protect, &old) &&
      VirtualProtect(ret, 4096, ret_protect, &old);
  if (!protected_ok) {
    VirtualFree(out_all, 0, MEM_RELEASE);
    VirtualFree(ret_all, 0, MEM_RELEASE);
    return false;
  }
  uintptr_t output = output_pointer(out_kind, out, basic, extended);
  uintptr_t returned = returned_pointer(ret_kind, output, ret, basic);
  HANDLE handle = handle_kind == 0   ? GetCurrentProcess()
                  : handle_kind == 1 ? nullptr
                  : handle_kind == 2 ? GetCurrentThread()
                                     : event;
  ULONG status = 0xCCCCCCCC;
  SetLastError(0x12345678);
  DWORD exception =
      invoke(query, handle, reinterpret_cast<void *>(output), length,
             reinterpret_cast<ULONG *>(returned), &status);
  DWORD last_error = GetLastError();
  MEMORY_BASIC_INFORMATION out_info{}, ret_info{};
  if (!VirtualQuery(out, &out_info, sizeof(out_info)) ||
      !VirtualQuery(ret, &ret_info, sizeof(ret_info)))
    return false;
  if (!VirtualProtect(out, 4096, PAGE_READWRITE, &old) ||
      !VirtualProtect(ret, 4096, PAGE_READWRITE, &old))
    return false;
  std::printf(
      "FAULT id=%u pattern=%02X handle-kind=%u length=%lu out-kind=%u "
      "ret-kind=%u out-protect=%08lX ret-protect=%08lX out-after=%08lX "
      "ret-after=%08lX status=%08lX exception=%08lX last-error=%08lX out-head=",
      id, pattern, handle_kind, length, out_kind, ret_kind, out_protect,
      ret_protect, out_info.Protect, ret_info.Protect, status, exception,
      last_error);
  dump(out, 128);
  std::printf(" out-tail=");
  dump(out + 3968, 128);
  std::printf(" ret-head=");
  dump(ret, 32);
  std::printf(" ret-tail=");
  dump(ret + 4080, 16);
  std::printf("\n");
  ++calls;
  bool a = VirtualFree(out_all, 0, MEM_RELEASE) != FALSE;
  bool b = VirtualFree(ret_all, 0, MEM_RELEASE) != FALSE;
  return a && b;
}

static bool faults(QueryProcess query) {
  constexpr ULONG basic = sizeof(PROCESS_BASIC_INFORMATION),
                  extended = 8 * sizeof(void *);
  HANDLE event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
  if (!event)
    return false;
  unsigned id = 0;
  for (unsigned h = 0; h < 4; ++h)
    for (ULONG length : {ULONG(0), basic - 1, basic, extended, extended + 1})
      for (unsigned out = 0; out < 9; ++out)
        for (unsigned ret = 0; ret < 10; ++ret) {
          ++id;
          for (unsigned pattern : {0xA5u, 0x5Au})
            if (!fault_row(query, id, pattern, h, event, length, out, ret,
                           PAGE_READWRITE, PAGE_READWRITE)) {
              CloseHandle(event);
              return false;
            }
        }
  const DWORD protections[] = {PAGE_READWRITE, PAGE_READONLY, PAGE_NOACCESS,
                               PAGE_READWRITE | PAGE_GUARD,
                               PAGE_READONLY | PAGE_GUARD};
  for (DWORD op : protections)
    for (DWORD rp : protections)
      for (unsigned out : {0u, 6u})
        for (unsigned ret : {1u, 4u, 8u})
          for (ULONG length : {basic, extended})
            for (unsigned h : {0u, 1u}) {
              ++id;
              for (unsigned pattern : {0xA5u, 0x5Au})
                if (!fault_row(query, id, pattern, h, event, length, out, ret,
                               op, rp)) {
                  CloseHandle(event);
                  return false;
                }
            }
  if (!CloseHandle(event))
    return false;
  std::printf(
      "FAULT-SUMMARY cases=%u observations=%u event-opened=1 event-closed=1\n",
      id, id * 2);
  return true;
}

int main(int argc, char **argv) {
  const char *profile = argc > 1 ? argv[1] : "unknown";
  SYSTEM_INFO system_info{};
  GetSystemInfo(&system_info);
  if (system_info.dwPageSize != 4096)
    return 13;
  auto ntdll = GetModuleHandleW(L"ntdll.dll");
  auto query = reinterpret_cast<QueryProcess>(
      GetProcAddress(ntdll, "NtQueryInformationProcess"));
  auto query_object =
      reinterpret_cast<QueryObject>(GetProcAddress(ntdll, "NtQueryObject"));
  if (!query || !query_object)
    return 2;
  DWORD pid = GetCurrentProcessId(), parent = 0, priority = 0, exit_status = 0;
  HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
  if (snapshot == INVALID_HANDLE_VALUE)
    return 3;
  PROCESSENTRY32W entry{};
  entry.dwSize = sizeof(entry);
  if (Process32FirstW(snapshot, &entry)) {
    unsigned visited = 0;
    do {
      if (++visited > 65536) {
        CloseHandle(snapshot);
        return 4;
      }
      if (entry.th32ProcessID == pid) {
        parent = entry.th32ParentProcessID;
        priority = static_cast<DWORD>(entry.pcPriClassBase);
        break;
      }
    } while (Process32NextW(snapshot, &entry));
  }
  CloseHandle(snapshot);
  DWORD_PTR affinity = 0, system_affinity = 0;
  if (!GetProcessAffinityMask(GetCurrentProcess(), &affinity,
                              &system_affinity) ||
      !GetExitCodeProcess(GetCurrentProcess(), &exit_status) || priority == 0)
    return 5;
  constexpr ULONG basic = sizeof(PROCESS_BASIC_INFORMATION);
  constexpr ULONG extended = 8 * sizeof(void *);
  static_assert(basic == 6 * sizeof(void *));
  std::printf("META profile=%s pointer-bytes=%zu basic-bytes=%lu "
              "extended-candidate-bytes=%lu pid=%08lX parent=%08lX peb=%llX "
              "affinity=%llX base-priority=%lu exit-status=%08lX\n",
              profile, sizeof(void *), basic, extended, pid, parent,
              static_cast<unsigned long long>(reinterpret_cast<uintptr_t>(
                  NtCurrentTeb()->ProcessEnvironmentBlock)),
              static_cast<unsigned long long>(affinity), priority, exit_status);
  DWORD before = 0, after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 6;
  const ULONG lengths[] = {0,  1,  4,  23, 24, 25, 31, 32,
                           33, 47, 48, 49, 63, 64, 65, 128};
  for (ULONG length : lengths) {
    row(query, "self-pseudo", GetCurrentProcess(), 0, length, 0, false, true);
    for (uintptr_t seed :
         {uintptr_t(0), uintptr_t(basic), uintptr_t(extended), ~uintptr_t(0)})
      row(query, "self-pseudo", GetCurrentProcess(), 0, length, seed, true,
          true);
  }
  for (bool want_returned : {false, true})
    row(query, "self-pseudo", GetCurrentProcess(), 0, basic, 0, false,
        want_returned);
  for (HANDLE handle :
       {HANDLE(nullptr), HANDLE(uintptr_t(1)), GetCurrentThread()})
    for (ULONG length : {ULONG(0), basic, extended})
      row(query,
          handle == nullptr ? "null"
                            : (handle == GetCurrentThread() ? "thread-pseudo"
                                                            : "invalid-one"),
          handle, 0, length, extended, true, true);
  const ULONG grants[] = {0,      0x10,    0x20,     0x400,   0x1000,
                          0x1400, 0x20000, 0x100000, 0x1FFFFF};
  unsigned opened = 0, closed = 0;
  for (ULONG requested : grants) {
    HANDLE handle = nullptr;
    if (!DuplicateHandle(GetCurrentProcess(), GetCurrentProcess(),
                         GetCurrentProcess(), &handle, requested, FALSE, 0)) {
      std::printf("DUPLICATE-FAILED requested=%08lX error=%lu\n", requested,
                  GetLastError());
      return 7;
    }
    ++opened;
    PUBLIC_OBJECT_BASIC_INFORMATION info{};
    ULONG returned = 0;
    NTSTATUS status = query_object(handle, 0, &info, sizeof(info), &returned);
    std::printf("GRANT requested=%08lX status=%08lX input-bytes=%zu "
                "returned=%lu actual=%08lX\n",
                requested, static_cast<ULONG>(status), sizeof(info), returned,
                info.GrantedAccess);
    if (status < 0) {
      CloseHandle(handle);
      return 8;
    }
    for (uintptr_t tag = 0; tag < 4; ++tag) {
      char kind[48];
      std::snprintf(kind, sizeof(kind), "self-real-%08lX-tag%u", requested,
                    unsigned(tag));
      HANDLE tagged =
          reinterpret_cast<HANDLE>(reinterpret_cast<uintptr_t>(handle) | tag);
      row(query, kind, tagged, info.GrantedAccess, basic, 0, false, true);
      row(query, kind, tagged, info.GrantedAccess, extended, extended, true,
          true);
    }
    if (!CloseHandle(handle))
      return 9;
    ++closed;
  }
  if (!faults(query))
    return 12;
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 10;
  std::printf("SUMMARY calls=%u opened=%u closed=%u handles-before=%lu "
              "handles-after=%lu\n",
              calls, opened, closed, before, after);
  return before == after && opened == closed ? 0 : 11;
}
