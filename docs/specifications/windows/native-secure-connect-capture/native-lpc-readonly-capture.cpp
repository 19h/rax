#include <cstdint>
#include <cstdio>
#include <cstring>
#include <thread>
#include <windows.h>
#include <winternl.h>

using Connect = LONG(NTAPI *)(uintptr_t, uintptr_t, uintptr_t, uintptr_t,
                              uintptr_t, uintptr_t, uintptr_t, uintptr_t,
                              uintptr_t);
using Create = LONG(NTAPI *)(HANDLE *, ULONG, void *, LARGE_INTEGER *, ULONG,
                             ULONG, HANDLE);
struct alignas(8) View {
  ULONG length, padding;
  uint64_t section;
  ULONG offset, padding2;
  uint64_t size, base, remote;
};
struct alignas(8) Remote {
  ULONG length, padding;
  uint64_t size, base;
};
static_assert(sizeof(View) == 48);
static_assert(sizeof(Remote) == 24);

static LONG invoke(Connect connect, const uintptr_t *a, ULONG *exception) {
  __try {
    return connect(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], a[8]);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
    return LONG(0xDEADBEEF);
  }
}
static void dump(const char *name, const void *address, size_t bytes) {
  std::printf("bytes name=%s size=%zu hex=", name, bytes);
  const auto *data = static_cast<const unsigned char *>(address);
  for (size_t i = 0; i != bytes; ++i)
    std::printf("%02X", data[i]);
  std::puts("");
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  std::thread([] {
    Sleep(60000);
    ExitProcess(78);
  }).detach();
  auto module = GetModuleHandleW(L"ntdll.dll");
  auto connect =
      reinterpret_cast<Connect>(GetProcAddress(module, "NtSecureConnectPort"));
  auto create =
      reinterpret_cast<Create>(GetProcAddress(module, "NtCreateSection"));
  if (!connect || !create)
    return 21;
  HANDLE section = nullptr;
  LARGE_INTEGER maximum{};
  maximum.QuadPart = 65536;
  if (create(&section, 0xF001F, nullptr, &maximum, PAGE_READWRITE, SEC_COMMIT,
             nullptr) < 0)
    return 22;
  auto *arena = static_cast<unsigned char *>(VirtualAlloc(
      nullptr, 16 * 4096, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!arena)
    return 23;
  LARGE_INTEGER counter{};
  QueryPerformanceCounter(&counter);
  wchar_t missing[128]{};
  int characters = swprintf_s(
      missing, L"\\Windows\\RaxLpcReadonly-%lu-%llX", GetCurrentProcessId(),
      static_cast<unsigned long long>(counter.QuadPart));
  if (characters <= 0)
    return 24;
  const char *names[] = {"baseline",    "output",      "name-header",
                         "name-text",   "qos",         "client-view",
                         "sid",         "server-view", "message",
                         "information", "length"};
  std::printf("profile pid=%lu width=%zu cases=11\n", GetCurrentProcessId(),
              sizeof(void *));
  for (unsigned index = 0; index != 11; ++index) {
    DWORD old = 0;
    if (!VirtualProtect(arena, 16 * 4096, PAGE_READWRITE, &old))
      return 25;
    std::memset(arena, 0xA5, 16 * 4096);
    auto at = [arena](unsigned page) { return arena + page * 4096 + 64; };
    auto *name = reinterpret_cast<UNICODE_STRING *>(at(1));
    name->Length = USHORT(characters * 2);
    name->MaximumLength = USHORT((characters + 1) * 2);
    name->Buffer = reinterpret_cast<wchar_t *>(at(2));
    std::memcpy(at(2), missing, size_t(characters + 1) * 2);
    SECURITY_QUALITY_OF_SERVICE qos{};
    qos.ImpersonationLevel = SecurityImpersonation;
    qos.ContextTrackingMode = 1;
    qos.EffectiveOnly = 1;
    std::memcpy(at(3), &qos, sizeof qos);
    View view{};
    view.length = 48;
    view.section = reinterpret_cast<uintptr_t>(section);
    view.size = 65536;
    std::memcpy(at(4), &view, sizeof view);
    const unsigned char system_sid[12] = {1, 1, 0, 0, 0, 0, 0, 5, 18, 0, 0, 0};
    std::memcpy(at(5), system_sid, sizeof system_sid);
    Remote remote{};
    remote.length = 24;
    std::memcpy(at(6), &remote, sizeof remote);
    ULONG length = 32;
    std::memcpy(at(9), &length, sizeof length);
    if (index != 0 &&
        !VirtualProtect(arena + (index - 1) * 4096, 4096, PAGE_READONLY, &old))
      return 26;
    uintptr_t args[] = {
        reinterpret_cast<uintptr_t>(at(0)), reinterpret_cast<uintptr_t>(at(1)),
        reinterpret_cast<uintptr_t>(at(3)), reinterpret_cast<uintptr_t>(at(4)),
        reinterpret_cast<uintptr_t>(at(5)), reinterpret_cast<uintptr_t>(at(6)),
        reinterpret_cast<uintptr_t>(at(7)), reinterpret_cast<uintptr_t>(at(8)),
        reinterpret_cast<uintptr_t>(at(9))};
    DWORD before = 0, after = 0;
    if (!GetProcessHandleCount(GetCurrentProcess(), &before))
      return 27;
    ULONG exception = 0;
    LONG status = invoke(connect, args, &exception);
    if (!GetProcessHandleCount(GetCurrentProcess(), &after))
      return 28;
    std::printf("case=%u name=%s readonly-page=%d status=%08lX exception=%08lX "
                "handles-before=%lu handles-after=%lu\n",
                index, names[index], index ? int(index - 1) : -1, ULONG(status),
                exception, before, after);
    dump("output", at(0), 32);
    dump("client-view", at(4), 48);
    dump("server-view", at(6), 24);
    dump("message", at(7), 8);
    dump("information", at(8), 64);
    dump("length", at(9), 8);
    if ((status >= 0 && exception == 0) || before != after)
      return 29;
  }
  if (!CloseHandle(section) || !VirtualFree(arena, 0, MEM_RELEASE))
    return 30;
  std::puts("complete cases=11 closed-handles=1 no-native-success=1");
  return 0;
}
