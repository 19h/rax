#include <cstdint>
#include <cstdio>
#include <cstring>
#include <intrin.h>
#include <windows.h>

using Continue = LONG(NTAPI *)(PCONTEXT, BOOLEAN);
using Alert = LONG(NTAPI *)(HANDLE);
using Test = LONG(NTAPI *)();
using Capture = VOID(NTAPI *)(PCONTEXT);
static Continue nt_continue;
static Alert nt_alert;
static Test nt_test;
static Capture capture;
struct Event {
  ULONG_PTR value;
  unsigned phase;
  DWORD last;
  DWORD tid;
};
struct State {
  CONTEXT context;
  volatile LONG stage;
  unsigned number;
  unsigned phase;
  unsigned count;
  Event events[8];
};
static __declspec(thread) State state;
static void CALLBACK callback(ULONG_PTR value) {
  unsigned index = state.count++;
  if (index < 8)
    state.events[index] = {value, state.phase, GetLastError(),
                           GetCurrentThreadId()};
  if (state.number == 5 && value == 0x1111)
    QueueUserAPC(callback, GetCurrentThread(), 0x3333);
  if (value == 0x4444)
    SetLastError(0xABCD);
}
#if defined(_M_IX86)
// Native x86 RtlCaptureContext unwinds its caller frame. Keep the selected
// continuation in worker, rather than in the thread-start caller.
static __declspec(noinline) void capture_for_worker() {
  void *expected_pc = _ReturnAddress();
  uintptr_t expected_sp =
      reinterpret_cast<uintptr_t>(_AddressOfReturnAddress()) + 4;
  capture(&state.context);
  printf(
      "capture case=%u pc=%p expected-pc=%p sp=%p expected-sp=%p flags=%08lX\n",
      state.number, reinterpret_cast<void *>(uintptr_t(state.context.Eip)),
      expected_pc, reinterpret_cast<void *>(uintptr_t(state.context.Esp)),
      reinterpret_cast<void *>(expected_sp), state.context.ContextFlags);
  if (state.context.Eip != reinterpret_cast<uintptr_t>(expected_pc) ||
      state.context.Esp != expected_sp)
    ExitThread(14);
}
#endif
static DWORD WINAPI worker(void *argument) {
  state = {};
  state.number = static_cast<unsigned>(reinterpret_cast<uintptr_t>(argument));
  unsigned number = state.number;
  BOOLEAN test =
      number == 0 || number == 2 || number == 8 || number == 12 ? 0 : 1;
  if (number == 10)
    test = 2;
  if (number == 11)
    test = 255;
  bool queued = number == 2 || number == 3 || number == 4 || number == 5 ||
                number == 6 || number == 8 || number == 9 || number == 10 ||
                number == 11;
  if (queued && !QueueUserAPC(callback, GetCurrentThread(),
                              number == 9 ? 0x4444 : 0x1111))
    return 10;
  if ((number == 4 || number == 5) &&
      !QueueUserAPC(callback, GetCurrentThread(), 0x2222))
    return 11;
  if (number == 6 || number == 7 || number == 8 || number == 12) {
    LONG result = nt_alert(GetCurrentThread());
    printf("alert case=%u status=%08lX seen=%u\n", number, result, state.count);
    if (result)
      return 12;
  }
  state.phase = 1;
#if defined(_M_IX86)
  capture_for_worker();
#else
  capture(&state.context);
#endif
  if (state.stage == 0) {
    state.stage = 1;
    SetLastError(0xC0DE);
    LONG result = nt_continue(&state.context, test);
    printf("unexpected-return case=%u test=%u status=%08lX\n", number,
           unsigned(test), result);
    return 13;
  }
  state.phase = 2;
  printf("resume case=%u test=%u seen=%u last=%08lX tid=%08lX\n", state.number,
         unsigned(test), state.count, GetLastError(), GetCurrentThreadId());
  for (unsigned index = 0; index < 2; ++index) {
    state.phase = 3 + index;
    unsigned before = state.count;
    SetLastError(0xC0DE);
    LONG result = nt_test();
    printf(
        "check case=%u index=%u status=%08lX before=%u after=%u last=%08lX\n",
        state.number, index, result, before, state.count, GetLastError());
  }
  for (unsigned index = 0; index < state.count && index < 8; ++index) {
    const Event &event = state.events[index];
    printf("event case=%u index=%u value=%llX phase=%u last=%08lX tid=%08lX\n",
           state.number, index, static_cast<unsigned long long>(event.value),
           event.phase, event.last, event.tid);
  }
  return 0;
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX |
               SEM_NOOPENFILEERRORBOX);
  HMODULE module = GetModuleHandleW(L"ntdll.dll");
  nt_continue =
      reinterpret_cast<Continue>(GetProcAddress(module, "NtContinue"));
  nt_alert = reinterpret_cast<Alert>(GetProcAddress(module, "NtAlertThread"));
  nt_test = reinterpret_cast<Test>(GetProcAddress(module, "NtTestAlert"));
  capture =
      reinterpret_cast<Capture>(GetProcAddress(module, "RtlCaptureContext"));
  if (!nt_continue || !nt_alert || !nt_test || !capture)
    return 1;
  printf("profile=%s pointer=%zu context-bytes=%zu\n",
         argc > 1 ? argv[1] : "unknown", sizeof(void *), sizeof(CONTEXT));
  for (unsigned number = 0; number < 13; ++number) {
    HANDLE thread =
        CreateThread(nullptr, 0, worker,
                     reinterpret_cast<void *>(uintptr_t(number)), 0, nullptr);
    if (!thread)
      return 2;
    DWORD wait = WaitForSingleObject(thread, 30000);
    DWORD code = 0xFFFFFFFF;
    if (wait == WAIT_OBJECT_0)
      GetExitCodeThread(thread, &code);
    CloseHandle(thread);
    printf("worker case=%u wait=%08lX exit=%lu\n", number, wait, code);
    if (wait != WAIT_OBJECT_0 || code != 0)
      return 3;
  }
  printf("complete cases=13\n");
  return 0;
}
