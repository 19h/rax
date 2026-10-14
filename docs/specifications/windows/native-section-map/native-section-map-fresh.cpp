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

static void inspect_alias(HANDLE child, HANDLE original, unsigned ordinal,
                          const char *phase, bool section) {
  HANDLE alias = nullptr;
  if (!DuplicateHandle(child, original, GetCurrentProcess(), &alias, 0, FALSE,
                       DUPLICATE_SAME_ACCESS))
    fail("inspect-duplicate");
  using QueryObject = LONG(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);
  auto query = reinterpret_cast<QueryObject>(
      GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "NtQueryObject"));
  if (!query)
    fail("query-object-export");
  for (ULONG cls = 0; cls < 3; cls++) {
    alignas(16) unsigned char buffer[16384]{};
    ULONG need = 0;
    ULONG supplied =
        cls == 0 ? sizeof(PUBLIC_OBJECT_BASIC_INFORMATION) : sizeof(buffer);
    LONG status = query(alias, cls, buffer, supplied, &need);
    printf("object case=%u phase=%s child-handle=%llX class=%lu status=%08lX "
           "need=%lu supplied=%lu data=",
           ordinal, phase, (unsigned long long)original, cls, ULONG(status),
           need, supplied);
    for (unsigned i = 0; i < 64; i++)
      printf("%02X", unsigned(buffer[i]));
    if (status < 0)
      fail("query-object-status");
    if (cls != 0) {
      auto u = reinterpret_cast<UNICODE_STRING *>(buffer);
      uintptr_t begin = reinterpret_cast<uintptr_t>(u->Buffer);
      uintptr_t end = reinterpret_cast<uintptr_t>(buffer) + sizeof(buffer);
      if (u->Length > u->MaximumLength || (u->Length & 1) ||
          begin < reinterpret_cast<uintptr_t>(buffer) || begin > end ||
          u->Length > end - begin)
        fail("query-object-name-bounds");
      printf(" name-units=%u name=", unsigned(u->Length / 2));
      for (unsigned i = 0; i < u->Length / 2; i++)
        printf("%04X", unsigned(u->Buffer[i]));
    }
    puts("");
  }
  if (section) {
    using QuerySection = LONG(NTAPI *)(HANDLE, ULONG, PVOID, SIZE_T, PSIZE_T);
    auto q = reinterpret_cast<QuerySection>(
        GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "NtQuerySection"));
    if (!q)
      fail("query-section-export");
    alignas(16) unsigned char b[256]{};
    SIZE_T need = 0;
    LONG status = q(alias, 0, b, 24, &need);
    printf("section case=%u status=%08lX need=%llu data=", ordinal,
           ULONG(status), (unsigned long long)need);
    for (unsigned i = 0; i < 64; i++)
      printf("%02X", unsigned(b[i]));
    puts("");
    if (status < 0)
      fail("query-section-basic");
  }
  if (!CloseHandle(alias))
    fail("close-inspection-alias");
}

static void capture_view(HANDLE child, uintptr_t base, SIZE_T size,
                         unsigned ordinal) {
  if (!base || !size || size > 64 * 1024 * 1024 || base + size < base)
    fail("mapped-view-bounds");
  uintptr_t cursor = base, end = base + size;
  unsigned regions = 0;
  while (cursor < end) {
    MEMORY_BASIC_INFORMATION info{};
    if (VirtualQueryEx(child, reinterpret_cast<void *>(cursor), &info,
                       sizeof(info)) != sizeof(info))
      fail("mapped-region-query");
    uintptr_t lo = uintptr_t(info.BaseAddress), hi = lo + info.RegionSize;
    if (hi <= cursor || hi < lo || ++regions > 256)
      fail("mapped-region-bounds");
    printf("region case=%u index=%u base=%llX allocation=%llX size=%llu "
           "state=%lX protect=%lX allocation-protect=%lX type=%lX\n",
           ordinal, regions - 1, (unsigned long long)lo,
           (unsigned long long)info.AllocationBase,
           (unsigned long long)info.RegionSize, info.State, info.Protect,
           info.AllocationProtect, info.Type);
    if (info.State == MEM_COMMIT &&
        !(info.Protect & (PAGE_NOACCESS | PAGE_GUARD)))
      dump(child, cursor,
           ULONG((hi < end ? hi : end) - cursor < 64
                     ? (hi < end ? hi : end) - cursor
                     : 64),
           ordinal, "region-prefix");
    cursor = hi;
  }
  IMAGE_DOS_HEADER dos{};
  read(child, base, &dos, sizeof(dos));
  if (dos.e_magic != IMAGE_DOS_SIGNATURE || dos.e_lfanew < 64 ||
      dos.e_lfanew > 4096)
    fail("mapped-dos-header");
  IMAGE_NT_HEADERS64 nt{};
  read(child, base + dos.e_lfanew, &nt, sizeof(nt));
  if (nt.Signature != IMAGE_NT_SIGNATURE ||
      nt.OptionalHeader.Magic != IMAGE_NT_OPTIONAL_HDR64_MAGIC)
    fail("mapped-nt-header");
  dump(child, base, 512, ordinal, "mapped-header");
  printf("pe case=%u machine=%04X sections=%u preferred=%llX image-size=%lu "
         "headers=%lu entry=%lX dll-flags=%04X\n",
         ordinal, nt.FileHeader.Machine, nt.FileHeader.NumberOfSections,
         (unsigned long long)nt.OptionalHeader.ImageBase,
         nt.OptionalHeader.SizeOfImage, nt.OptionalHeader.SizeOfHeaders,
         nt.OptionalHeader.AddressOfEntryPoint,
         nt.OptionalHeader.DllCharacteristics);
  if (nt.FileHeader.NumberOfSections > 96 ||
      nt.FileHeader.SizeOfOptionalHeader != sizeof(IMAGE_OPTIONAL_HEADER64))
    fail("mapped-pe-section-bounds");
  uintptr_t sections =
      base + dos.e_lfanew + 24 + nt.FileHeader.SizeOfOptionalHeader;
  for (unsigned i = 0; i < nt.FileHeader.NumberOfSections; i++) {
    IMAGE_SECTION_HEADER section{};
    read(child, sections + i * sizeof(section), &section, sizeof(section));
    printf("pe-section case=%u index=%u name=", ordinal, i);
    for (auto byte : section.Name)
      printf("%02X", unsigned(byte));
    printf(
        " rva=%lX virtual=%lu raw=%lu raw-offset=%lX characteristics=%08lX\n",
        section.VirtualAddress, section.Misc.VirtualSize, section.SizeOfRawData,
        section.PointerToRawData, section.Characteristics);
  }
  auto reloc = nt.OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_BASERELOC];
  printf("relocation-directory case=%u rva=%lX size=%lu\n", ordinal,
         reloc.VirtualAddress, reloc.Size);
  if (reloc.Size > 1024 * 1024 || reloc.VirtualAddress > size ||
      reloc.Size > size - reloc.VirtualAddress)
    fail("mapped-relocation-bounds");
  uintptr_t at = base + reloc.VirtualAddress, stop = at + reloc.Size;
  unsigned blocks = 0, observed = 0;
  while (at < stop) {
    IMAGE_BASE_RELOCATION block{};
    if (stop - at < sizeof(block))
      fail("mapped-relocation-tail");
    read(child, at, &block, sizeof(block));
    if (block.SizeOfBlock < sizeof(block) || (block.SizeOfBlock & 1) ||
        block.SizeOfBlock > stop - at)
      fail("mapped-relocation-block");
    for (ULONG i = 0;
         i < (block.SizeOfBlock - sizeof(block)) / 2 && observed < 64; i++) {
      WORD item = 0;
      read(child, at + sizeof(block) + 2 * i, &item, 2);
      unsigned type = item >> 12;
      if (!type)
        continue;
      uintptr_t target = block.VirtualAddress + (item & 0xFFF);
      if (target > size || 8 > size - target)
        fail("mapped-relocation-target");
      printf("relocation case=%u index=%u block=%u rva=%llX type=%u\n", ordinal,
             observed++, blocks, (unsigned long long)target, type);
      dump(child, base + target, 8, ordinal, "relocation-target");
    }
    at += block.SizeOfBlock;
    blocks++;
  }
  printf("view-complete case=%u base=%llX size=%llu regions=%u "
         "relocation-blocks=%u targets=%u\n",
         ordinal, (unsigned long long)base, (unsigned long long)size, regions,
         blocks, observed);
}

int main(int argc, char **) {
  if (argc > 1)
    return 41;
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto module = GetModuleHandleW(L"ntdll.dll");
  uintptr_t entry = uintptr_t(GetProcAddress(module, "NtMapViewOfSection"));
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
    ULONGLONG args[10]{};
    unsigned calls = 0;
    ULONGLONG deadline = GetTickCount64() + 120000;
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
      } else if (e.dwDebugEventCode == CREATE_THREAD_DEBUG_EVENT) {
        if (e.u.CreateThread.hThread && !CloseHandle(e.u.CreateThread.hThread))
          fail("close-debug-thread-handle");
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
          for (unsigned i = 0; i < 8; i++)
            args[i] = c.X[i];
          read(p.hProcess, c.Sp, args + 8, 16);
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
          printf("stack case=%u sp=%llX allocation=%08lX protect=%08lX\n",
                 ordinal, (unsigned long long)c.Sp, ULONG(args[8]),
                 ULONG(args[9]));
          if (!args[0] || !args[2] || !args[6])
            fail("initial-map-shape");
          dump(p.hProcess, args[2], 8, ordinal, "base-before");
          dump(p.hProcess, args[6], 8, ordinal, "size-before");
          if (args[5])
            dump(p.hProcess, args[5], 8, ordinal, "offset-before");
          inspect_alias(p.hProcess, reinterpret_cast<HANDLE>(args[0]), ordinal,
                        "section-before", true);
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
          dump(p.hProcess, args[2], 8, ordinal, "base-after");
          dump(p.hProcess, args[6], 8, ordinal, "size-after");
          if (args[5])
            dump(p.hProcess, args[5], 8, ordinal, "offset-after");
          if (LONG(c.X0) >= 0) {
            uintptr_t base = 0;
            SIZE_T size = 0;
            read(p.hProcess, args[2], &base, sizeof(base));
            read(p.hProcess, args[6], &size, sizeof(size));
            capture_view(p.hProcess, base, size, ordinal);
          }
          inspect_alias(p.hProcess, reinterpret_cast<HANDLE>(args[0]), ordinal,
                        "section-after", true);
          done = true;
          if (!TerminateProcess(p.hProcess, 77))
            fail("terminate-owned-child");
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
