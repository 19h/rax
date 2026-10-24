#define WIN32_LEAN_AND_MEAN
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>
#if !defined(_M_ARM64)
#error This observer requires native ARM64.
#endif

struct Hook {
  unsigned char *address;
  std::uint32_t original[2];
};
static Hook init2, dispatcher, callback_return;
static unsigned char *peb;
static unsigned init_events, callback_events, return_events;
static bool dispatcher_step;
static unsigned phase;
static bool override_callback;
static std::uint32_t override_status;
static unsigned char peb_before[512], teb_before[6144];
static unsigned char callback_peb_before[512], callback_teb_before[6144];
static constexpr std::uint32_t breakpoint = 0xd43e0000;
static NTSTATUS(NTAPI *get_status)();
static void(NTAPI *set_status)(NTSTATUS);
struct PreserveErrors {
  DWORD error = GetLastError();
  NTSTATUS status = get_status();
  ~PreserveErrors() {
    set_status(status);
    SetLastError(error);
  }
};

[[noreturn]] static void alternate_callback() {
  auto callback = reinterpret_cast<NTSTATUS(NTAPI *)(void *, ULONG, NTSTATUS)>(
      callback_return.address);
  callback(nullptr, 0, static_cast<NTSTATUS>(override_status));
  ExitProcess(0xe0000001);
}

static bool patch(Hook &hook, std::uint32_t first, std::uint32_t second) {
  DWORD old = 0, ignored = 0;
  if (!VirtualProtect(hook.address, 8, PAGE_EXECUTE_READWRITE, &old))
    return false;
  const std::uint32_t words[2] = {first, second};
  std::memcpy(hook.address, words, sizeof(words));
  bool flushed =
      FlushInstructionCache(GetCurrentProcess(), hook.address, 8) != 0;
  bool restored = VirtualProtect(hook.address, 8, old, &ignored) != 0;
  return flushed && restored;
}

static std::uintptr_t pointer(const void *address) {
  std::uintptr_t result = 0;
  std::memcpy(&result, address, sizeof(result));
  return result;
}

static void region(const char *label, std::uintptr_t address) {
  PreserveErrors preserve;
  MEMORY_BASIC_INFORMATION info{};
  auto length = VirtualQuery(reinterpret_cast<const void *>(address), &info,
                             sizeof(info));
  std::printf("region=%s address=%016llx returned=%zu base=%p allocation=%p "
              "bytes=%zu state=%08lx protect=%08lx type=%08lx\n",
              label, static_cast<unsigned long long>(address), length,
              info.BaseAddress, info.AllocationBase, info.RegionSize,
              info.State, info.Protect, info.Type);
}

static void bytes(const char *label, std::uintptr_t address,
                  std::size_t length) {
  PreserveErrors preserve;
  unsigned char data[4096]{};
  if (length > sizeof(data)) {
    std::printf("buffer=%s address=%016llx requested=%zu bounded=4096\n", label,
                static_cast<unsigned long long>(address), length);
    length = sizeof(data);
  }
  SIZE_T copied = 0;
  BOOL ok = ReadProcessMemory(GetCurrentProcess(),
                              reinterpret_cast<const void *>(address), data,
                              length, &copied);
  std::printf(
      "buffer=%s address=%016llx requested=%zu ok=%d copied=%zu data=", label,
      static_cast<unsigned long long>(address), length, ok != 0, copied);
  for (std::size_t i = 0; i < copied; ++i)
    std::printf("%02x", data[i]);
  std::putchar('\n');
}

static void state(const char *label) {
  PreserveErrors preserve;
  auto callback = pointer(peb + 0x58), gdi = pointer(peb + 0xf8);
  std::printf("phase=%s experiment=%u init-events=%u callbacks=%u returns=%u "
              "callback-table=%016llx "
              "gdi-table=%016llx\n",
              label, phase, init_events, callback_events, return_events,
              static_cast<unsigned long long>(callback),
              static_cast<unsigned long long>(gdi));
  if (gdi)
    region("gdi", gdi);
}

static void deltas(const char *label, const unsigned char *before,
                   const unsigned char *after, std::size_t length) {
  for (std::size_t at = 0; at < length; at += 8) {
    auto old = pointer(before + at), now = pointer(after + at);
    if (old != now)
      std::printf("delta=%s offset=%04zx before=%016llx after=%016llx\n", label,
                  at, static_cast<unsigned long long>(old),
                  static_cast<unsigned long long>(now));
  }
}

static LONG WINAPI observe(EXCEPTION_POINTERS *event) {
  PreserveErrors preserve;
  if (event->ExceptionRecord->ExceptionCode != EXCEPTION_BREAKPOINT)
    return EXCEPTION_CONTINUE_SEARCH;
  auto pc = reinterpret_cast<std::uintptr_t>(
      event->ExceptionRecord->ExceptionAddress);
  auto context = event->ContextRecord;
  if (pc == reinterpret_cast<std::uintptr_t>(init2.address) &&
      init_events == 0) {
    state("before-first-init2-svc");
    std::memcpy(peb_before, peb, sizeof(peb_before));
    std::memcpy(teb_before, NtCurrentTeb(), sizeof(teb_before));
    if (!patch(init2, init2.original[0], breakpoint))
      return EXCEPTION_CONTINUE_SEARCH;
    ++init_events;
    context->Pc = pc;
    return EXCEPTION_CONTINUE_EXECUTION;
  }
  if (pc == reinterpret_cast<std::uintptr_t>(init2.address + 4) &&
      init_events == 1) {
    state("after-first-init2-svc");
    deltas("peb", peb_before, peb, sizeof(peb_before));
    deltas("teb", teb_before, reinterpret_cast<unsigned char *>(NtCurrentTeb()),
           sizeof(teb_before));
    std::printf("init2-return=%016llx\n",
                static_cast<unsigned long long>(context->X0));
    if (!patch(init2, init2.original[0], init2.original[1]))
      return EXCEPTION_CONTINUE_SEARCH;
    ++init_events;
    context->Pc = pc;
    return EXCEPTION_CONTINUE_EXECUTION;
  }
  if (pc == reinterpret_cast<std::uintptr_t>(dispatcher.address) &&
      !dispatcher_step) {
    if (++callback_events > 64)
      return EXCEPTION_CONTINUE_SEARCH;
    std::uintptr_t buffer = pointer(reinterpret_cast<void *>(context->Sp));
    std::uint32_t length = 0, index = 0;
    std::memcpy(&length, reinterpret_cast<void *>(context->Sp + 8), 4);
    std::memcpy(&index, reinterpret_cast<void *>(context->Sp + 12), 4);
    auto table = pointer(peb + 0x58);
    std::uintptr_t target = 0;
    SIZE_T copied = 0;
    if (index >= 4096 ||
        !ReadProcessMemory(GetCurrentProcess(),
                           reinterpret_cast<const void *>(table + index * 8ull),
                           &target, sizeof(target), &copied) ||
        copied != sizeof(target))
      return EXCEPTION_CONTINUE_SEARCH;
    std::printf("callback=%u init-events=%u index=%u length=%u sp=%016llx "
                "target=%016llx teb=%016llx\n",
                callback_events, init_events, index, length,
                static_cast<unsigned long long>(context->Sp),
                static_cast<unsigned long long>(target),
                static_cast<unsigned long long>(
                    reinterpret_cast<std::uintptr_t>(NtCurrentTeb())));
    state("callback-entry");
    region("callback-target", target);
    region("callback-buffer", buffer);
    bytes("callback-frame", context->Sp, 128);
    bytes("callback-input", buffer, length);
    bytes("callback-target", target, 1024);
    bytes("gdi-table-first-page", pointer(peb + 0xf8), 4096);
    auto teb = reinterpret_cast<unsigned char *>(NtCurrentTeb());
    bytes("teb-client-info", reinterpret_cast<std::uintptr_t>(teb + 0x780),
          0x200);
    for (unsigned offset : {0x820u, 0x828u, 0x860u, 0x8d0u, 0x918u}) {
      auto address = pointer(teb + offset);
      std::printf("client-field-offset=%x\n", offset);
      region("client-field", address);
      bytes("client-field", address, 4096);
    }
    std::memcpy(callback_peb_before, peb, sizeof(callback_peb_before));
    std::memcpy(callback_teb_before, teb, sizeof(callback_teb_before));
    if (override_callback && index == 84) {
      std::printf("controlled-callback-status=%08x\n", override_status);
      if (!patch(callback_return, breakpoint, callback_return.original[1]))
        return EXCEPTION_CONTINUE_SEARCH;
      context->Pc = reinterpret_cast<std::uintptr_t>(alternate_callback);
      return EXCEPTION_CONTINUE_EXECUTION;
    }
    if (!patch(dispatcher, dispatcher.original[0], breakpoint) ||
        !patch(callback_return, breakpoint, callback_return.original[1]))
      return EXCEPTION_CONTINUE_SEARCH;
    dispatcher_step = true;
    context->Pc = pc;
    return EXCEPTION_CONTINUE_EXECUTION;
  }
  if (pc == reinterpret_cast<std::uintptr_t>(dispatcher.address + 4) &&
      dispatcher_step) {
    if (!patch(dispatcher, breakpoint, dispatcher.original[1]))
      return EXCEPTION_CONTINUE_SEARCH;
    dispatcher_step = false;
    context->Pc = pc;
    return EXCEPTION_CONTINUE_EXECUTION;
  }
  if (pc == reinterpret_cast<std::uintptr_t>(callback_return.address)) {
    ++return_events;
    std::printf("callback-return=%u init-events=%u buffer=%016llx length=%llu "
                "status=%08llx\n",
                return_events, init_events,
                static_cast<unsigned long long>(context->X0),
                static_cast<unsigned long long>(context->X1),
                static_cast<unsigned long long>(context->X2 & 0xffffffffull));
    state("callback-return");
    bytes("callback-output", context->X0,
          static_cast<std::uint32_t>(context->X1));
    deltas("callback-peb", callback_peb_before, peb,
           sizeof(callback_peb_before));
    deltas("callback-teb", callback_teb_before,
           reinterpret_cast<unsigned char *>(NtCurrentTeb()),
           sizeof(callback_teb_before));
    if (!patch(callback_return, callback_return.original[0],
               callback_return.original[1]))
      return EXCEPTION_CONTINUE_SEARCH;
    context->Pc = pc;
    return EXCEPTION_CONTINUE_EXECUTION;
  }
  return EXCEPTION_CONTINUE_SEARCH;
}

static bool select(Hook &hook, HMODULE module, const char *name,
                   std::uint32_t first, std::uint32_t second) {
  hook.address =
      reinterpret_cast<unsigned char *>(GetProcAddress(module, name));
  if (!hook.address)
    return false;
  std::memcpy(hook.original, hook.address, 8);
  std::printf("hook=%s address=%p rva=%llx first=%08x second=%08x\n", name,
              hook.address,
              static_cast<unsigned long long>(
                  hook.address - reinterpret_cast<unsigned char *>(module)),
              hook.original[0], hook.original[1]);
  return hook.original[0] == first && hook.original[1] == second;
}

static DWORD WINAPI worker(void *) {
  state("worker-start");
  auto call = reinterpret_cast<std::uintptr_t(WINAPI *)(void)>(init2.address);
  for (unsigned i = 0; i < 4; ++i)
    std::printf("worker-repeat=%u result=%016llx\n", i,
                static_cast<unsigned long long>(call()));
  state("worker-finished");
  return 0;
}

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 9;
  if (std::strcmp(argv[1], "normal") &&
      std::strcmp(argv[1], "callback-success") &&
      std::strcmp(argv[1], "callback-failure"))
    return 10;
  override_callback = std::strcmp(argv[1], "normal") != 0;
  override_status = !std::strcmp(argv[1], "callback-failure") ? 0xc0000001 : 0;
  phase = 1;
  std::printf("mode=%s\n", argv[1]);
  std::printf("pid=%lu tid=%lu\n", GetCurrentProcessId(), GetCurrentThreadId());
  auto nt = GetModuleHandleW(L"ntdll.dll"),
       win32u = LoadLibraryW(L"win32u.dll");
  get_status = reinterpret_cast<NTSTATUS(NTAPI *)()>(
      GetProcAddress(nt, "RtlGetLastNtStatus"));
  set_status = reinterpret_cast<void(NTAPI *)(NTSTATUS)>(
      GetProcAddress(nt, "RtlSetLastWin32ErrorAndNtStatusFromNtStatus"));
  if (!get_status || !set_status)
    return 8;
  auto query = reinterpret_cast<NTSTATUS(NTAPI *)(HANDLE, PROCESSINFOCLASS,
                                                  PVOID, ULONG, PULONG)>(
      GetProcAddress(nt, "NtQueryInformationProcess"));
  PROCESS_BASIC_INFORMATION info{};
  ULONG length = 0;
  if (!win32u || !query ||
      query(GetCurrentProcess(), ProcessBasicInformation, &info, sizeof(info),
            &length) < 0)
    return 2;
  peb = reinterpret_cast<unsigned char *>(info.PebBaseAddress);
  if (!select(init2, win32u, "NtGdiInit2", 0xd4025fa1, 0xd65f03c0) ||
      !select(dispatcher, nt, "KiUserCallbackDispatcher", 0xf94003e0,
              0xb9400be1) ||
      !select(callback_return, nt, "NtCallbackReturn", 0xd40000a1, 0xd65f03c0))
    return 3;
  auto handler = AddVectoredExceptionHandler(1, observe);
  if (!handler || !patch(dispatcher, breakpoint, dispatcher.original[1]) ||
      !patch(callback_return, breakpoint, callback_return.original[1]) ||
      !patch(init2, breakpoint, init2.original[1]))
    return 4;
  state("before-load-user32");
  auto user = LoadLibraryW(L"user32.dll");
  std::printf("user32-loaded=%d\n", user != nullptr);
  if (!user)
    return 5;
  state("after-load-user32");
  auto call = reinterpret_cast<std::uintptr_t(WINAPI *)(void)>(init2.address);
  for (unsigned i = 0; i < 4; ++i)
    std::printf("repeat=%u result=%016llx\n", i,
                static_cast<unsigned long long>(call()));
  if (!override_callback) {
    if (init_events != 2)
      return 11;
    phase = 2;
    init_events = 0;
    if (!patch(init2, breakpoint, init2.original[1]))
      return 12;
    auto thread = CreateThread(nullptr, 0, worker, nullptr, 0, nullptr);
    if (!thread || WaitForSingleObject(thread, 30000) != WAIT_OBJECT_0 ||
        !CloseHandle(thread))
      return 13;
    state("main-after-worker");
  }
  if (init_events != 2 || dispatcher_step)
    return 6;
  bool restored =
      patch(init2, init2.original[0], init2.original[1]) &&
      patch(dispatcher, dispatcher.original[0], dispatcher.original[1]) &&
      patch(callback_return, callback_return.original[0],
            callback_return.original[1]);
  restored = restored && !std::memcmp(init2.address, init2.original, 8) &&
             !std::memcmp(dispatcher.address, dispatcher.original, 8) &&
             !std::memcmp(callback_return.address, callback_return.original, 8);
  if (!restored || !RemoveVectoredExceptionHandler(handler) ||
      !FreeLibrary(user) || !FreeLibrary(win32u))
    return 7;
  std::printf("finished=1 restored-instructions=3 closed-module-references=2 "
              "removed-observers=1 callbacks=%u returns=%u\n",
              callback_events, return_events);
  return 0;
}
