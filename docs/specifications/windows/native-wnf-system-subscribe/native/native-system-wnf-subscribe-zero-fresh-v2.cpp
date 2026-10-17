#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <intrin.h>
#include <string>
#include <vector>
#include <windows.h>
#include <winternl.h>
extern "C" __declspec(dllimport) BOOL WINAPI
ConvertStringSecurityDescriptorToSecurityDescriptorW(LPCWSTR, DWORD,
                                                     PSECURITY_DESCRIPTOR *,
                                                     PULONG);
using CreateState = LONG(NTAPI *)(uint64_t *, ULONG, ULONG, BOOLEAN,
                                  const GUID *, ULONG, PSECURITY_DESCRIPTOR);
using DeleteState = LONG(NTAPI *)(const uint64_t *);
using UpdateState = LONG(NTAPI *)(const uint64_t *, const void *, ULONG,
                                  const GUID *, const void *, ULONG, ULONG);
using Query = LONG(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);
struct EventInfo {
  ULONG type;
  LONG state;
};
static uintptr_t pc(const CONTEXT &c) {
#if defined(_M_ARM64)
  return c.Pc;
#elif defined(_M_X64)
  return c.Rip;
#else
  return c.Eip;
#endif
}
static void set_pc(CONTEXT &c, uintptr_t v) {
#if defined(_M_ARM64)
  c.Pc = v;
#elif defined(_M_X64)
  c.Rip = v;
#else
  c.Eip = ULONG(v);
#endif
}
static uintptr_t stack(const CONTEXT &c) {
#if defined(_M_ARM64)
  return c.Sp;
#elif defined(_M_X64)
  return c.Rsp;
#else
  return c.Esp;
#endif
}
static uintptr_t result(const CONTEXT &c) {
#if defined(_M_ARM64)
  return c.X0;
#elif defined(_M_X64)
  return c.Rax;
#else
  return c.Eax;
#endif
}
static void fail(const char *what) {
  printf("failure=%s error=%lu\n", what, GetLastError());
  ExitProcess(21);
}
static void read(HANDLE process, uintptr_t at, void *out, SIZE_T size) {
  SIZE_T got = 0;
  if (!ReadProcessMemory(process, reinterpret_cast<void *>(at), out, size,
                         &got) ||
      got != size)
    fail("read-child");
}
static void write(HANDLE process, uintptr_t at, const void *input,
                  SIZE_T size) {
  SIZE_T put = 0;
  if (!WriteProcessMemory(process, reinterpret_cast<void *>(at), input, size,
                          &put) ||
      put != size)
    fail("write-child");
}
static void code(HANDLE process, uintptr_t at,
                 const std::vector<unsigned char> &bytes) {
  DWORD old = 0, ignored = 0;
  if (!VirtualProtectEx(process, reinterpret_cast<void *>(at), bytes.size(),
                        PAGE_EXECUTE_READWRITE, &old))
    fail("protect-child-code");
  write(process, at, bytes.data(), bytes.size());
  if (!VirtualProtectEx(process, reinterpret_cast<void *>(at), bytes.size(),
                        old, &ignored) ||
      !FlushInstructionCache(process, reinterpret_cast<void *>(at),
                             bytes.size()))
    fail("restore-child-protection");
}
static uintptr_t argument(HANDLE process, const CONTEXT &c) {
#if defined(_M_ARM64)
  return c.X0;
#elif defined(_M_X64)
  return c.Rcx;
#else
  ULONG v = 0;
  read(process, c.Esp + 4, &v, 4);
  return v;
#endif
}
static void set_argument(HANDLE process, CONTEXT &c, uintptr_t v) {
#if defined(_M_ARM64)
  c.X0 = v;
#elif defined(_M_X64)
  c.Rcx = v;
#else
  ULONG value = ULONG(v);
  write(process, c.Esp + 4, &value, 4);
#endif
}
static uintptr_t caller(HANDLE process, const CONTEXT &c) {
#if defined(_M_ARM64)
  return c.Lr;
#else
  uintptr_t v = 0;
  read(process, stack(c), &v, sizeof v);
  return v;
#endif
}
static void repeat(HANDLE process, CONTEXT &c, uintptr_t entry,
                   uintptr_t resume, uintptr_t arg) {
#if defined(_M_ARM64)
  c.Lr = resume;
  c.X0 = arg;
#elif defined(_M_X64)
  c.Rsp -= 8;
  write(process, c.Rsp, &resume, 8);
  c.Rcx = arg;
#else
  c.Esp -= 8;
  ULONG ret = ULONG(resume), value = ULONG(arg);
  write(process, c.Esp, &ret, 4);
  write(process, c.Esp + 4, &value, 4);
#endif
  set_pc(c, entry);
}

static std::vector<uintptr_t> handles() {
  std::vector<uintptr_t> result;
  for (uintptr_t value = 4; value < 65536; value += 4) {
    DWORD flags = 0;
    if (GetHandleInformation(reinterpret_cast<HANDLE>(value), &flags))
      result.push_back(value);
  }
  return result;
}
static void object_info(Query query, HANDLE event, unsigned char *buffer,
                        const char *phase, unsigned ordinal) {
  ULONG returned = 0;
  LONG status = query(event, 0, buffer, 56, &returned);
  ULONG hc = 0, pc = 0;
  memcpy(&hc, buffer + 8, 4);
  memcpy(&pc, buffer + 12, 4);
  printf("object case=%u phase=%s status=%08lX returned=%lu handles=%lu "
         "pointers=%lu\n",
         ordinal, phase, ULONG(status), returned, hc, pc);
  if (status < 0 || returned != 56)
    fail("object-query");
}
#if !defined(_M_ARM64)
#error This producer measures native ARM64 process startup only.
#endif
static void invoke(CONTEXT &c, uintptr_t entry, uintptr_t resume, uint64_t a0,
                   uint64_t a1 = 0, uint64_t a2 = 0, uint64_t a3 = 0) {
  c.Pc = entry;
  c.Lr = resume;
  c.X0 = a0;
  c.X1 = a1;
  c.X2 = a2;
  c.X3 = a3;
}
int main(int argc, char **argv) {
  auto m = GetModuleHandleW(L"ntdll.dll");
  auto setter =
      uintptr_t(GetProcAddress(m, "NtSetWnfProcessNotificationEvent"));
  auto subscribe = uintptr_t(GetProcAddress(m, "NtSubscribeWnfStateChange"));
  auto unsubscribe =
      uintptr_t(GetProcAddress(m, "NtUnsubscribeWnfStateChange"));
  if (argc > 1 && strcmp(argv[1], "--child") == 0) {
    HANDLE event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (!event)
      return 42;
    reinterpret_cast<LONG(NTAPI *)(HANDLE)>(setter)(event);
    CloseHandle(event);
    return 41;
  }
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto query = reinterpret_cast<Query>(GetProcAddress(m, "NtQueryEvent"));
  auto dbg = GetProcAddress(m, "DbgBreakPoint");
  if (!setter || !subscribe || !unsubscribe || !query || !dbg)
    fail("exports");
  std::vector<unsigned char> trap(4), original(4);
  memcpy(trap.data(), dbg, 4);
  memcpy(original.data(), reinterpret_cast<void *>(setter), 4);
  ULONG encoded = 0;
  memcpy(&encoded, trap.data(), 4);
  if ((encoded & 0xFFE0001F) != 0xD4200000)
    fail("native-break");
  wchar_t exe[32768];
  if (!GetModuleFileNameW(nullptr, exe, 32768))
    fail("path");
  using QueryState = LONG(NTAPI *)(const uint64_t *, const GUID *, const void *,
                                   ULONG *, void *, ULONG *);
  auto query_state =
      reinterpret_cast<QueryState>(GetProcAddress(m, "NtQueryWnfStateData"));
  uint64_t selected_name = 0x0280032EA3BC0875ull;
  auto remove_state =
      reinterpret_cast<DeleteState>(GetProcAddress(m, "NtDeleteWnfStateName"));
  bool owned = argc == 2 && (strcmp(argv[1], "--owned-empty") == 0 ||
                             strcmp(argv[1], "--owned-value") == 0);
  if (!owned)
    fail("explicit-owned-mode-required");
  auto create_state =
      reinterpret_cast<CreateState>(GetProcAddress(m, "NtCreateWnfStateName"));
  auto update_state =
      reinterpret_cast<UpdateState>(GetProcAddress(m, "NtUpdateWnfStateData"));
  if (!create_state || !remove_state || !update_state)
    fail("owned-exports");
  PSECURITY_DESCRIPTOR sd = nullptr;
  ULONG sd_size = 0;
  if (!ConvertStringSecurityDescriptorToSecurityDescriptorW(
          L"O:SYG:SYD:(A;;GA;;;WD)", 1, &sd, &sd_size))
    fail("owned-descriptor");
  LONG created = create_state(&selected_name, 3, 0, FALSE, nullptr, 64, sd);
  if (LocalFree(sd) != nullptr)
    fail("owned-descriptor-release");
  printf("owned-create status=%08lX name=%016llX mode=%s descriptor-freed=1\n",
         ULONG(created), (unsigned long long)selected_name, argv[1]);
  if (created < 0)
    fail("owned-create");
  if (strcmp(argv[1], "--owned-value") == 0) {
    const uint64_t payload = 0x1122334455667788ull;
    LONG updated =
        update_state(&selected_name, &payload, 0, nullptr, nullptr, 0, 0);
    printf("owned-update status=%08lX\n", ULONG(updated));
    if (updated < 0)
      fail("owned-update");
  }
  const GUID selected_type{0x48f96463,
                           0x52d6,
                           0x4e85,
                           {0x9c, 0x02, 0xf2, 0xc1, 0x4e, 0x17, 0x26, 0x4e}};
  BYTE before_data[8]{}, after_data[8]{};
  ULONG before_stamp = 0, before_size = 8, after_stamp = 0, after_size = 8;
  if (!query_state ||
      query_state(&selected_name, &selected_type, nullptr, &before_stamp,
                  before_data, &before_size) < 0 ||
      before_size > 8)
    fail("selected-state-before");
  const ULONG stamps[] = {0, before_stamp, 0xFFFFFFFF};
  if (before_size != 0)
    fail("zero-length-before");
  printf("selected-size-before=%lu\n", before_size);
  printf("selected-before stamp=%08lX bytes=", before_stamp);
  for (BYTE value : before_data)
    printf("%02X", value);
  puts("");
  for (unsigned ordinal = 0; ordinal < 193; ordinal++) {
    bool warmup = ordinal == 0;
    unsigned index = warmup ? 0 : ordinal - 1, stamp_index = index / 64,
             after = (index / 32) % 2, mask = index % 32;
    ULONG selected_stamp = stamps[stamp_index];
    DWORD hb = 0, ha = 0;
    GetProcessHandleCount(GetCurrentProcess(), &hb);
    auto before_handles = handles();
    std::wstring command = L"\"" + std::wstring(exe) + L"\" --child";
    STARTUPINFOW start{};
    start.cb = sizeof start;
    PROCESS_INFORMATION p{};
    if (!CreateProcessW(exe, command.data(), nullptr, nullptr, FALSE,
                        DEBUG_ONLY_THIS_PROCESS, nullptr, nullptr, &start, &p))
      fail("create-child");
    bool installed = false, entered = false, exited = false;
    unsigned phase = 0, calls = 0;
    uintptr_t resume = 0, event_arg = 0, remote = 0;
    std::vector<unsigned char> saved(4);
    HANDLE event = nullptr;
    ULONG set_status = 0xDEADBEEF, sub_status = set_status,
          repeat_status = set_status, unsub_status = set_status, exit = 0;
    uint64_t id = 0, id2 = 0;
    LONG states[5] = {-1, -1, -1, -1, -1};
    ULONG types[5] = {~0u, ~0u, ~0u, ~0u, ~0u};
    ULONGLONG deadline = GetTickCount64() + 30000;
    auto observe = [&](unsigned n) {
      EventInfo i{};
      ULONG size = 0;
      LONG status = query(event, 0, &i, 8, &size);
      if (status < 0 || size != 8)
        fail("event-query");
      states[n] = i.state;
      types[n] = i.type;
    };
    while (!exited && GetTickCount64() < deadline) {
      DEBUG_EVENT e{};
      if (!WaitForDebugEvent(&e, 1000)) {
        if (GetLastError() == ERROR_SEM_TIMEOUT)
          continue;
        fail("debug-event");
      }
      DWORD disposition = DBG_CONTINUE;
      if ((e.dwDebugEventCode == CREATE_PROCESS_DEBUG_EVENT ||
           e.dwDebugEventCode == LOAD_DLL_DEBUG_EVENT) &&
          !installed) {
        unsigned char bytes[32];
        SIZE_T got = 0;
        MEMORY_BASIC_INFORMATION info{};
        if (VirtualQueryEx(p.hProcess, reinterpret_cast<void *>(setter), &info,
                           sizeof info) &&
            info.Type == MEM_IMAGE &&
            ReadProcessMemory(p.hProcess, reinterpret_cast<void *>(setter),
                              bytes, 32, &got) &&
            got == 32 &&
            memcmp(bytes, reinterpret_cast<void *>(setter), 32) == 0) {
          code(p.hProcess, setter, trap);
          installed = true;
        }
      }
      if (e.dwDebugEventCode == CREATE_PROCESS_DEBUG_EVENT) {
        if (e.u.CreateProcessInfo.hFile)
          CloseHandle(e.u.CreateProcessInfo.hFile);
      } else if (e.dwDebugEventCode == LOAD_DLL_DEBUG_EVENT) {
        if (e.u.LoadDll.hFile)
          CloseHandle(e.u.LoadDll.hFile);
      } else if (e.dwDebugEventCode == EXCEPTION_DEBUG_EVENT) {
        auto address =
            uintptr_t(e.u.Exception.ExceptionRecord.ExceptionAddress);
        ULONG exception = e.u.Exception.ExceptionRecord.ExceptionCode;
        if ((exception == EXCEPTION_BREAKPOINT || exception == 0x4000001F) &&
            installed && address == setter && !entered) {
          if (e.dwThreadId != p.dwThreadId)
            fail("startup-thread");
          CONTEXT ctx{};
          ctx.ContextFlags = CONTEXT_CONTROL | CONTEXT_INTEGER;
          if (!GetThreadContext(p.hThread, &ctx))
            fail("context");
          event_arg = ctx.X0;
          resume = ctx.Lr;
          read(p.hProcess, resume, saved.data(), 4);
          if (!DuplicateHandle(p.hProcess, reinterpret_cast<HANDLE>(event_arg),
                               GetCurrentProcess(), &event, 0, FALSE,
                               DUPLICATE_SAME_ACCESS))
            fail("event-alias");
          observe(0);
          remote = uintptr_t(VirtualAllocEx(p.hProcess, nullptr, 4096,
                                            MEM_RESERVE | MEM_COMMIT,
                                            PAGE_READWRITE));
          if (!remote)
            fail("remote");
          uint64_t values[] = {selected_name, 0xA5A5A5A5A5A5A5A5};
          write(p.hProcess, remote, values, 16);
          code(p.hProcess, setter, original);
          code(p.hProcess, resume, trap);
          if (after)
            invoke(ctx, setter, resume, event_arg);
          else
            invoke(ctx, subscribe, resume, remote, selected_stamp, mask,
                   remote + 8);
          if (!SetThreadContext(p.hThread, &ctx))
            fail("initial-call");
          entered = true;
          printf("trap case=%u phase=entry address=%llX\n", ordinal,
                 (unsigned long long)address);
        } else if ((exception == EXCEPTION_BREAKPOINT ||
                    exception == 0x4000001F) &&
                   entered && address == resume) {
          printf("trap case=%u phase=%u address=%llX\n", ordinal, phase,
                 (unsigned long long)address);
          CONTEXT ctx{};
          ctx.ContextFlags = CONTEXT_CONTROL | CONTEXT_INTEGER;
          if (!GetThreadContext(p.hThread, &ctx))
            fail("return-context");
          ULONG status = ULONG(ctx.X0);
          calls++;
          observe(phase + 1);
          if (phase == 0) {
            if (after)
              set_status = status;
            else {
              sub_status = status;
              read(p.hProcess, remote + 8, &id, 8);
            }
            if (after)
              invoke(ctx, subscribe, resume, remote, selected_stamp, mask,
                     remote + 8);
            else
              invoke(ctx, setter, resume, event_arg);
          } else if (phase == 1) {
            if (after) {
              sub_status = status;
              read(p.hProcess, remote + 8, &id, 8);
            } else
              set_status = status;
            invoke(ctx, subscribe, resume, remote, ~ULONG(0), mask, remote + 8);
          } else if (phase == 2) {
            repeat_status = status;
            read(p.hProcess, remote + 8, &id2, 8);
            invoke(ctx, unsubscribe, resume, remote);
          } else {
            unsub_status = status;
            code(p.hProcess, resume, saved);
            if (!TerminateProcess(p.hProcess, 77))
              fail("terminate-child");
          }
          if (phase < 3 && !SetThreadContext(p.hThread, &ctx))
            fail("next-call");
          phase++;
        } else if (exception != EXCEPTION_BREAKPOINT &&
                   exception != 0x4000001F) {
          printf("unexpected=%08lX address=%llX\n", exception,
                 (unsigned long long)address);
          disposition = DBG_EXCEPTION_NOT_HANDLED;
        }
      } else if (e.dwDebugEventCode == EXIT_PROCESS_DEBUG_EVENT) {
        exit = e.u.ExitProcess.dwExitCode;
        exited = true;
      }
      if (!ContinueDebugEvent(e.dwProcessId, e.dwThreadId, disposition))
        fail("continue");
    }
    if (!exited) {
      TerminateProcess(p.hProcess, 78);
      fail("deadline");
    }
    if (event)
      CloseHandle(event);
    CloseHandle(p.hThread);
    CloseHandle(p.hProcess);
    GetProcessHandleCount(GetCurrentProcess(), &ha);
    printf("case=%u warmup=%u width=%zu after=%u stamp=%08X mask=%08X calls=%u "
           "registered=%08X status=%08X id=%016llX repeat=%08X "
           "repeat-id=%016llX end=%08X state0=%ld state1=%ld state2=%ld "
           "state3=%ld state4=%ld type0=%lu type1=%lu type2=%lu type3=%lu "
           "type4=%lu exit=%lu handles-before=%lu handles-after=%lu\n",
           ordinal, warmup, sizeof(void *), after, selected_stamp, mask, calls,
           set_status, sub_status, (unsigned long long)id, repeat_status,
           (unsigned long long)id2, unsub_status, states[0], states[1],
           states[2], states[3], states[4], types[0], types[1], types[2],
           types[3], types[4], exit, hb, ha);
    if (calls != 4 || exit != 77 ||
        (!warmup && (hb != ha || before_handles != handles())))
      return 22;
  }
  if (query_state(&selected_name, &selected_type, nullptr, &after_stamp,
                  after_data, &after_size) < 0 ||
      after_size != before_size || after_stamp != before_stamp ||
      memcmp(before_data, after_data, 8) != 0)
    fail("selected-state-changed");
  printf("selected-size-after=%lu\n", after_size);
  printf("selected-after stamp=%08lX bytes=", after_stamp);
  for (BYTE value : after_data)
    printf("%02X", value);
  puts("");
  LONG removed = remove_state(&selected_name);
  printf("owned-delete status=%08lX\n", ULONG(removed));
  if (removed < 0)
    fail("owned-delete");
  puts("complete cases=192 warmup=1 deleted=1 descriptor-freed=1");
  return 0;
}
