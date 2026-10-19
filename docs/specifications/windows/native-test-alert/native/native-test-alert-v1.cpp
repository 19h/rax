#include <cstdint>
#include <cstdio>
#include <vector>
#include <windows.h>

using TestAlert = LONG(NTAPI *)();
using AlertThread = LONG(NTAPI *)(HANDLE);
using NativeApc = VOID(NTAPI *)(PVOID, PVOID, PVOID);
using QueueApc = LONG(NTAPI *)(HANDLE, NativeApc, PVOID, PVOID, PVOID);
static TestAlert test_alert;
static AlertThread alert_thread;
static QueueApc queue_apc;

struct Event {
  unsigned phase;
  unsigned kind;
  uintptr_t a, b, c;
  DWORD tid, last_error;
  LONG nested_status;
};
struct State {
  unsigned number;
  unsigned phase;
  bool append;
  bool recurse;
  std::vector<Event> events;
};
static thread_local State *active;

static void CALLBACK win_callback(ULONG_PTR argument) {
  State &s = *active;
  const size_t index = s.events.size();
  s.events.push_back(
      {s.phase, 1, argument, 0, 0, GetCurrentThreadId(), GetLastError(), 0});
  if (argument == 0x1111 && s.append) {
    s.append = false;
    QueueUserAPC(win_callback, GetCurrentThread(), 0x3333);
  }
  if (argument == 0x1111 && s.recurse) {
    s.recurse = false;
    const unsigned saved = s.phase;
    s.phase = 6;
    s.events[index].nested_status = test_alert();
    s.phase = saved;
  }
  if (argument == 0x4444) {
    SetLastError(0xABCD);
  }
}
static void NTAPI native_callback(PVOID a, PVOID b, PVOID c) {
  State &s = *active;
  s.events.push_back({s.phase, 3, reinterpret_cast<uintptr_t>(a),
                      reinterpret_cast<uintptr_t>(b),
                      reinterpret_cast<uintptr_t>(c), GetCurrentThreadId(),
                      GetLastError(), 0});
}

static uintptr_t stack(const CONTEXT &c) {
#if defined(_M_ARM64)
  return static_cast<uintptr_t>(c.Sp);
#elif defined(_M_X64)
  return static_cast<uintptr_t>(c.Rsp);
#else
  return static_cast<uintptr_t>(c.Esp);
#endif
}
static bool preserved(const CONTEXT &a, const CONTEXT &b) {
#if defined(_M_ARM64)
  return a.X19 == b.X19 && a.X20 == b.X20 && a.X21 == b.X21 && a.X22 == b.X22 &&
         a.X23 == b.X23 && a.X24 == b.X24 && a.X25 == b.X25 && a.X26 == b.X26 &&
         a.X27 == b.X27 && a.X28 == b.X28 && a.Fp == b.Fp && a.Sp == b.Sp;
#elif defined(_M_X64)
  return a.Rbx == b.Rbx && a.Rbp == b.Rbp && a.Rsi == b.Rsi && a.Rdi == b.Rdi &&
         a.R12 == b.R12 && a.R13 == b.R13 && a.R14 == b.R14 && a.R15 == b.R15 &&
         a.Rsp == b.Rsp;
#else
  return a.Ebx == b.Ebx && a.Ebp == b.Ebp && a.Esi == b.Esi && a.Edi == b.Edi &&
         a.Esp == b.Esp;
#endif
}

static DWORD WINAPI run_case(void *value) {
  State s{};
  s.number = static_cast<unsigned>(reinterpret_cast<uintptr_t>(value));
  s.events.reserve(128);
  s.append = s.number == 5;
  s.recurse = s.number == 6;
  active = &s;
  s.phase = 2;
  auto win = [&](uintptr_t a) {
    SetLastError(0xC0DE);
    const DWORD result = QueueUserAPC(win_callback, GetCurrentThread(), a);
    const DWORD error = GetLastError();
    printf("queue case=%u kind=1 arg=%llX result=%08lX last=%08lX seen=%zu\n",
           s.number, static_cast<unsigned long long>(a), result, error,
           s.events.size());
  };
  auto native = [&](uintptr_t a) {
    SetLastError(0xC0DE);
    const LONG result = queue_apc(
        GetCurrentThread(), native_callback, reinterpret_cast<void *>(a),
        reinterpret_cast<void *>(a + 1), reinterpret_cast<void *>(a + 2));
    const DWORD error = GetLastError();
    printf("queue case=%u kind=3 arg=%llX result=%08lX last=%08lX seen=%zu\n",
           s.number, static_cast<unsigned long long>(a),
           static_cast<unsigned long>(result), error, s.events.size());
  };
  if (s.number == 3 || s.number == 4 || s.number == 5 || s.number == 6 ||
      s.number == 11) {
    win(0x1111);
  }
  if (s.number == 4 || s.number == 5 || s.number == 6) {
    win(0x2222);
  }
  if (s.number == 7 || s.number == 8 || s.number == 9 || s.number == 10) {
    native(0x5555);
  }
  if (s.number == 8) {
    native(0x6666);
    native(0x7777);
  }
  if (s.number == 9) {
    win(0x1111);
    native(0x6666);
  }
  if (s.number == 12) {
    win(0x4444);
  }
  if (s.number == 1 || s.number == 2 || s.number == 10 || s.number == 11) {
    s.phase = 3;
    SetLastError(0xC0DE);
    const LONG status = alert_thread(GetCurrentThread());
    const DWORD error = GetLastError();
    printf("alert case=%u status=%08lX last=%08lX seen=%zu\n", s.number,
           static_cast<unsigned long>(status), error, s.events.size());
  }
  for (unsigned call = 0; call < (s.number == 2 ? 3u : 2u); ++call) {
    s.phase = 4 + call;
    SetLastError(0xC0DE);
    CONTEXT before{}, after{};
    RtlCaptureContext(&before);
    const size_t previous = s.events.size();
    const LONG status = test_alert();
    const DWORD error = GetLastError();
    RtlCaptureContext(&after);
    printf("check case=%u call=%u status=%08lX last=%08lX before=%zu after=%zu "
           "tid=%08lX "
           "sp-before=%llX sp-after=%llX nonvolatile=%u\n",
           s.number, call, static_cast<unsigned long>(status), error, previous,
           s.events.size(), GetCurrentThreadId(),
           static_cast<unsigned long long>(stack(before)),
           static_cast<unsigned long long>(stack(after)),
           preserved(before, after) ? 1u : 0u);
  }
  for (size_t index = 0; index < s.events.size(); ++index) {
    const Event &e = s.events[index];
    printf("event case=%u index=%zu phase=%u kind=%u a=%llX b=%llX c=%llX "
           "tid=%08lX "
           "last=%08lX nested=%08lX\n",
           s.number, index, e.phase, e.kind,
           static_cast<unsigned long long>(e.a),
           static_cast<unsigned long long>(e.b),
           static_cast<unsigned long long>(e.c), e.tid, e.last_error,
           static_cast<unsigned long>(e.nested_status));
  }
  active = nullptr;
  return 0;
}

int main(int argc, char **argv) {
  const HMODULE module = GetModuleHandleW(L"ntdll.dll");
  test_alert =
      reinterpret_cast<TestAlert>(GetProcAddress(module, "NtTestAlert"));
  alert_thread =
      reinterpret_cast<AlertThread>(GetProcAddress(module, "NtAlertThread"));
  queue_apc =
      reinterpret_cast<QueueApc>(GetProcAddress(module, "NtQueueApcThread"));
  if (!test_alert || !alert_thread || !queue_apc) {
    return 2;
  }
  printf("profile=%s pointer-bytes=%zu test-alert=%p alert-thread=%p "
         "queue-apc=%p\n",
         argc > 1 ? argv[1] : "unknown", sizeof(void *),
         reinterpret_cast<void *>(test_alert),
         reinterpret_cast<void *>(alert_thread),
         reinterpret_cast<void *>(queue_apc));
  for (unsigned number = 0; number <= 12; ++number) {
    const HANDLE thread = CreateThread(
        nullptr, 0, run_case,
        reinterpret_cast<void *>(static_cast<uintptr_t>(number)), 0, nullptr);
    if (!thread) {
      return 3;
    }
    if (WaitForSingleObject(thread, 30000) != WAIT_OBJECT_0) {
      return 4;
    }
    DWORD code = 0;
    if (!GetExitCodeThread(thread, &code) || !CloseHandle(thread) ||
        code != 0) {
      return 5;
    }
  }
  puts("complete cases=13 checks=27");
  return 0;
}
