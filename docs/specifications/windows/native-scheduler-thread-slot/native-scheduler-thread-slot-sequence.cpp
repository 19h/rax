#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#include <windows.h>
#include <winternl.h>
#if !defined(_M_ARM64)
#error This producer verifies native ARM64 entry and true LR return.
#endif
static void fail(const char *what) {
  printf("failure=%s error=%lu\n", what, GetLastError());
  ExitProcess(21);
}
static void read(HANDLE p, uintptr_t at, void *out, SIZE_T n) {
  SIZE_T got = 0;
  if (!ReadProcessMemory(p, reinterpret_cast<void *>(at), out, n, &got) ||
      got != n)
    fail("read-child");
}
static void write(HANDLE p, uintptr_t at, const void *in, SIZE_T n) {
  SIZE_T got = 0;
  if (!WriteProcessMemory(p, reinterpret_cast<void *>(at), in, n, &got) ||
      got != n)
    fail("write-child");
}
static void code(HANDLE p, uintptr_t at, const unsigned char *bytes) {
  DWORD old = 0, ignored = 0;
  if (!VirtualProtectEx(p, reinterpret_cast<void *>(at), 4,
                        PAGE_EXECUTE_READWRITE, &old))
    fail("protect-child-code");
  write(p, at, bytes, 4);
  if (!VirtualProtectEx(p, reinterpret_cast<void *>(at), 4, old, &ignored) ||
      !FlushInstructionCache(p, reinterpret_cast<void *>(at), 4))
    fail("restore-child-code");
}
static std::vector<uintptr_t> handles() {
  std::vector<uintptr_t> v;
  for (uintptr_t h = 4; h < 65536; h += 4) {
    DWORD f = 0;
    if (GetHandleInformation(reinterpret_cast<HANDLE>(h), &f))
      v.push_back(h);
  }
  return v;
}
static void dump(HANDLE p, uintptr_t at, ULONG n, unsigned ordinal,
                 const char *phase) {
  if (n > 4096)
    fail("dump-bound");
  std::vector<unsigned char> b(n);
  if (n)
    read(p, at, b.data(), n);
  printf("bytes case=%u phase=%s address=%llX size=%lu data=", ordinal, phase,
         (unsigned long long)at, n);
  for (auto x : b)
    printf("%02X", unsigned(x));
  puts("");
}
int main(int argc, char **) {
  if (argc > 1)
    return 41;
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto module = GetModuleHandleW(L"ntdll.dll");
  uintptr_t entry = uintptr_t(GetProcAddress(module, "NtSetInformationThread"));
  auto dbg = GetProcAddress(module, "DbgBreakPoint");
  if (!entry || !dbg)
    fail("exports");
  unsigned char trap[4], original[4];
  memcpy(trap, dbg, 4);
  memcpy(original, reinterpret_cast<void *>(entry), 4);
  ULONG service_instruction = 0;
  memcpy(&service_instruction, original, 4);
  if ((service_instruction & 0xFFE0001F) != 0xD4000001)
    fail("installed-native-svc-shape");
  const ULONG captured_function = (service_instruction >> 5) & 0xFFFF;
  ULONG encoded = 0;
  memcpy(&encoded, trap, 4);
  if ((encoded & 0xFFE0001F) != 0xD4200000)
    fail("native-break-instruction");
  wchar_t executable[32768];
  if (!GetModuleFileNameW(nullptr, executable, 32768))
    fail("executable-path");
  for (unsigned ordinal = 0; ordinal < 3; ordinal++) {
    uintptr_t original_owner = 0, original_slot = 0;
    auto hb = handles();
    DWORD count_before = 0, count_after = 0;
    if (!GetProcessHandleCount(GetCurrentProcess(), &count_before))
      fail("handle-count-before");
    std::wstring command = L"\"" + std::wstring(executable) + L"\" --child";
    STARTUPINFOW start{};
    start.cb = sizeof(start);
    PROCESS_INFORMATION p{};
    if (!CreateProcessW(executable, command.data(), nullptr, nullptr, FALSE,
                        DEBUG_ONLY_THIS_PROCESS, nullptr, nullptr, &start, &p))
      fail("debug-child");
    bool installed = false, waiting = false, done = false, exited = false;
    uintptr_t resume = 0;
    unsigned char caller_bytes[4];
    ULONG function = 0, exit_code = 0;
    ULONGLONG args[4]{};
    unsigned calls = 0;
    ULONGLONG deadline = GetTickCount64() + 30000;
    while (!exited && GetTickCount64() < deadline) {
      DEBUG_EVENT e{};
      if (!WaitForDebugEvent(&e, 1000)) {
        if (GetLastError() == ERROR_SEM_TIMEOUT)
          continue;
        fail("wait-debug-event");
      }
      DWORD disposition = DBG_CONTINUE;
      if ((e.dwDebugEventCode == CREATE_PROCESS_DEBUG_EVENT ||
           e.dwDebugEventCode == LOAD_DLL_DEBUG_EVENT) &&
          !installed) {
        MEMORY_BASIC_INFORMATION info{};
        unsigned char bytes[32];
        SIZE_T got = 0;
        if (VirtualQueryEx(p.hProcess, reinterpret_cast<void *>(entry), &info,
                           sizeof(info)) &&
            info.Type == MEM_IMAGE &&
            ReadProcessMemory(p.hProcess, reinterpret_cast<void *>(entry),
                              bytes, 32, &got) &&
            got == 32 &&
            memcmp(bytes, reinterpret_cast<void *>(entry), 32) == 0) {
          code(p.hProcess, entry, trap);
          installed = true;
          printf("image case=%u entry=%llX allocation=%llX type=%lX bytes=",
                 ordinal, (unsigned long long)entry,
                 (unsigned long long)info.AllocationBase, info.Type);
          for (auto b : bytes)
            printf("%02X", unsigned(b));
          puts("");
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
        bool breakpoint = exception == EXCEPTION_BREAKPOINT ||
                          exception == 0x4000001F ||
                          exception == EXCEPTION_ILLEGAL_INSTRUCTION;
        if (breakpoint && installed && !waiting && !done && address == entry) {
          if (e.dwThreadId != p.dwThreadId)
            fail("query-thread");
          unsigned char actual[4];
          read(p.hProcess, entry, actual, 4);
          if (memcmp(actual, trap, 4))
            fail("entry-trap-bytes");
          CONTEXT c{};
          c.ContextFlags = CONTEXT_CONTROL | CONTEXT_INTEGER;
          if (!GetThreadContext(p.hThread, &c))
            fail("entry-context");
          for (unsigned i = 0; i < 4; i++)
            args[i] = c.X[i];
          function = captured_function;
          resume = c.Lr;
          MEMORY_BASIC_INFORMATION info{};
          if (!VirtualQueryEx(p.hProcess, reinterpret_cast<void *>(resume),
                              &info, sizeof(info)) ||
              info.Type != MEM_IMAGE)
            fail("true-return-image");
          read(p.hProcess, resume, caller_bytes, 4);
          printf("entry case=%u call=%u function=%lu thread=%lu pc=%llX "
                 "lr=%llX args=",
                 ordinal, calls, function, e.dwThreadId,
                 (unsigned long long)c.Pc, (unsigned long long)resume);
          for (auto a : args)
            printf("%llX,", a);
          puts("");
          if (args[1] == 57) {
            if (args[0] != ~ULONGLONG(1) || !args[2] || args[3] != 24)
              fail("initial-scheduler-thread-slot-shape");
            dump(p.hProcess, args[2], 24, ordinal, "output-before");
            if (calls == 0)
              read(p.hProcess, args[2] + 8, &original_owner, 8);
          }
          code(p.hProcess, entry, original);
          code(p.hProcess, resume, trap);
          c.Pc = entry;
          if (!SetThreadContext(p.hThread, &c))
            fail("entry-resume");
          waiting = true;
        } else if (breakpoint && waiting && address == resume) {
          unsigned char actual[4];
          read(p.hProcess, resume, actual, 4);
          if (memcmp(actual, trap, 4))
            fail("return-trap-bytes");
          CONTEXT c{};
          c.ContextFlags = CONTEXT_CONTROL | CONTEXT_INTEGER;
          if (!GetThreadContext(p.hThread, &c))
            fail("return-context");
          printf("return case=%u call=%u function=%lu thread=%lu pc=%llX "
                 "status=%08lX\n",
                 ordinal, calls, function, e.dwThreadId,
                 (unsigned long long)c.Pc, ULONG(c.X0));
          code(p.hProcess, resume, caller_bytes);
          calls++;
          waiting = false;
          if (args[1] == 57) {
            dump(p.hProcess, args[2], 24, ordinal, "output-after");
            uintptr_t child_handle = 0;
            child_handle = original_owner;
            HANDLE local = nullptr;
            if ((calls == 1 && ULONG(c.X0) != 0) || !child_handle ||
                !DuplicateHandle(p.hProcess,
                                 reinterpret_cast<HANDLE>(child_handle),
                                 GetCurrentProcess(), &local, 0, FALSE,
                                 DUPLICATE_SAME_ACCESS))
              fail("fresh-owned-handle");
            using QueryObject =
                LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
            auto query = reinterpret_cast<QueryObject>(
                GetProcAddress(module, "NtQueryObject"));
            unsigned char basic[56]{}, type[512]{};
            ULONG returned = 0;
            if (!query ||
                query(local, 0, basic, sizeof(basic), &returned) < 0 ||
                query(local, 2, type, sizeof(type), &returned) < 0)
              fail("fresh-owner-query");
            auto name = reinterpret_cast<UNICODE_STRING *>(type);
            ULONG grant = 0;
            memcpy(&grant, basic + 4, 4);
            bool correct =
                name->Length == 38 &&
                memcmp(name->Buffer, L"SchedulerSharedData", 38) == 0;
            BOOL closed = CloseHandle(local);
            printf("owner case=%u handle=%llX grant=%08lX type-match=%u "
                   "duplicate-closed=%u\n",
                   ordinal, (unsigned long long)child_handle, grant,
                   unsigned(correct), unsigned(closed));
            if (!correct || grant != 1 || !closed)
              fail("fresh-owner-type-grant");
            uintptr_t slot = 0;
            read(p.hProcess, args[2] + 16, &slot, 8);
            if (calls == 1)
              original_slot = slot;
            MEMORY_BASIC_INFORMATION slot_info{};
            if (!original_slot ||
                !VirtualQueryEx(p.hProcess,
                                reinterpret_cast<void *>(original_slot),
                                &slot_info, sizeof(slot_info)))
              fail("original-slot-map");
            printf(
                "slot case=%u call=%u address=%llX current-slot=%llX base=%llX "
                "allocation=%llX size=%llX state=%lX type=%lX protect=%lX "
                "allocation-protect=%lX\n",
                ordinal, calls - 1, (unsigned long long)original_slot,
                (unsigned long long)slot,
                (unsigned long long)slot_info.BaseAddress,
                (unsigned long long)slot_info.AllocationBase,
                (unsigned long long)slot_info.RegionSize, slot_info.State,
                slot_info.Type, slot_info.Protect, slot_info.AllocationProtect);
            if (slot_info.State == MEM_COMMIT &&
                slot_info.Protect == PAGE_READWRITE)
              dump(p.hProcess, original_slot, 4096, ordinal,
                   "original-slot-page");
            // The first service call is unchanged native startup. Remaining
            // calls are declared controlled kernel probes on the owned child;
            // no ordinary guest execution claim is made from this sequence.
            if (calls < 8) {
              const ULONG actions[] = {0, 2, 0, 2, 1, 2, 0, 1};
              const uintptr_t owners[] = {
                  original_owner, original_owner, original_owner, 0,
                  original_owner, original_owner, original_owner, 0};
              unsigned char input[24]{};
              uintptr_t next_slot =
                  calls == 4 || calls == 7 ? original_slot : 0;
              memcpy(input, &actions[calls], 4);
              memcpy(input + 8, &owners[calls], 8);
              memcpy(input + 16, &next_slot, 8);
              write(p.hProcess, args[2], input, sizeof(input));
              printf("controlled case=%u next-call=%u action=%lu handle=%llX "
                     "slot=%llX\n",
                     ordinal, calls, actions[calls],
                     (unsigned long long)owners[calls],
                     (unsigned long long)next_slot);
              code(p.hProcess, entry, trap);
              c.X0 = ~ULONGLONG(1);
              c.X1 = 57;
              c.X2 = args[2];
              c.X3 = 24;
              c.Lr = resume;
              c.Pc = entry;
              if (!SetThreadContext(p.hThread, &c))
                fail("controlled-reentry");
            } else {
              done = true;
              if (!TerminateProcess(p.hProcess, 77))
                fail("terminate-owned-child");
            }
          } else {
            if (calls > 256)
              fail("call-bound");
            code(p.hProcess, entry, trap);
            c.Pc = resume;
            if (!SetThreadContext(p.hThread, &c))
              fail("skip-return-resume");
          }
        } else if (exception != EXCEPTION_BREAKPOINT &&
                   exception != 0x4000001F) {
          printf("unexpected-exception=%08lX address=%llX\n", exception,
                 (unsigned long long)address);
          disposition = DBG_EXCEPTION_NOT_HANDLED;
        }
      } else if (e.dwDebugEventCode == EXIT_PROCESS_DEBUG_EVENT) {
        exit_code = e.u.ExitProcess.dwExitCode;
        exited = true;
      }
      if (!ContinueDebugEvent(e.dwProcessId, e.dwThreadId, disposition))
        fail("continue-debug-event");
    }
    if (!exited) {
      TerminateProcess(p.hProcess, 78);
      fail("capture-deadline");
    }
    CloseHandle(p.hThread);
    CloseHandle(p.hProcess);
    if (!GetProcessHandleCount(GetCurrentProcess(), &count_after))
      fail("handle-count-after");
    printf("case=%u warmup=%u installed=%u done=%u calls=%u exit=%lu "
           "handles-before=%lu handles-after=%lu exact-handle-set=%u\n",
           ordinal, ordinal == 0, installed, done, calls, exit_code,
           count_before, count_after, hb == handles());
    if (!installed || !done || exit_code != 77 || (ordinal && hb != handles()))
      return 22;
  }
  puts("complete cases=2 warmup=1");
  return 0;
}
