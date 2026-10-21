#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <array>
#include <atomic>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <thread>
#include <windows.h>
#include <winternl.h>

// PHNT pinned PORT_MESSAGE64 shape: LPC uses this shape in WoW64 too.
struct alignas(8) Message {
  USHORT data, total, type, information;
  std::uint64_t pid, tid;
  ULONG id, padding;
  std::uint64_t callback;
  BYTE body[48];
};
static_assert(sizeof(Message) == 88 && offsetof(Message, body) == 40);
struct Client32 {
  USHORT data, total, type, information;
  ULONG pid, tid, id, callback;
  BYTE body[48];
};
static_assert(sizeof(Client32) == 72 && offsetof(Client32, body) == 24);
#if defined(_WIN64)
using ClientMessage = Message;
#else
using ClientMessage = Client32;
#endif
using Create = LONG(NTAPI *)(PHANDLE, POBJECT_ATTRIBUTES, ULONG, ULONG, ULONG);
using Connect = LONG(NTAPI *)(PHANDLE, PUNICODE_STRING,
                              PSECURITY_QUALITY_OF_SERVICE, PVOID, PVOID,
                              PULONG, PVOID, PULONG);
using Accept = LONG(NTAPI *)(PHANDLE, PVOID, PVOID, BOOLEAN, PVOID, PVOID);
using Complete = LONG(NTAPI *)(HANDLE);
using Receive = LONG(NTAPI *)(HANDLE, PVOID *, PVOID, PVOID, PLARGE_INTEGER);
using Reply = LONG(NTAPI *)(HANDLE, PVOID);
using Send = LONG(NTAPI *)(HANDLE, ULONG, PVOID, PVOID, PVOID, PSIZE_T, PVOID,
                           PLARGE_INTEGER);
struct Result {
  ULONG status, exception;
};
static Result invoke(Send send, HANDLE port, ULONG flags, void *message,
                     void *send_attrs, void *receive, SIZE_T *length,
                     void *receive_attrs, LARGE_INTEGER *timeout) {
  __try {
    return {ULONG(send(port, flags, message, send_attrs, receive, length,
                       receive_attrs, timeout)),
            0};
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    return {0xDEADBEEF, GetExceptionCode()};
  }
}
static ULONG protection(void *address) {
  MEMORY_BASIC_INFORMATION region{};
  return VirtualQuery(address, &region, sizeof(region)) == sizeof(region)
             ? region.Protect
             : 0xFFFFFFFF;
}
static bool dump_page(const char *name, void *address, SIZE_T length) {
  std::array<BYTE, 1024> bytes{};
  SIZE_T got = 0;
  BOOL ok = ReadProcessMemory(GetCurrentProcess(), address, bytes.data(),
                              length, &got);
  std::printf(" %s-read=%u %s-got=%zu %s-hex=", name, unsigned(ok), name, got,
              name);
  for (SIZE_T i = 0; i < got; ++i)
    std::printf("%02X", unsigned(bytes[i]));
  return ok && got == length;
}
static int run_case(unsigned test_case, const char *profile) {

  setvbuf(stdout, nullptr, _IONBF, 0);
  HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
#define LOAD(type, name)                                                       \
  auto name = reinterpret_cast<type>(GetProcAddress(ntdll, #name))
  LOAD(Create, NtCreatePort);
  LOAD(Connect, NtConnectPort);
  LOAD(Accept, NtAcceptConnectPort);
  LOAD(Complete, NtCompleteConnectPort);
  LOAD(Receive, NtReplyWaitReceivePortEx);
  LOAD(Reply, NtReplyPort);
  LOAD(Send, NtAlpcSendWaitReceivePort);
#undef LOAD
  if (!NtCreatePort || !NtConnectPort || !NtAcceptConnectPort ||
      !NtCompleteConnectPort || !NtReplyWaitReceivePortEx || !NtReplyPort ||
      !NtAlpcSendWaitReceivePort)
    return 2;
  // Initialize the installed thread/runtime once before resource accounting.
  std::thread([] {}).join();
  DWORD handles_before = 0, handles_after = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &handles_before))
    return 3;
  std::printf("PROFILE %s POINTER_BYTES %zu\n", profile, sizeof(void *));
  std::printf("BEGIN case=%u\n", test_case);
  wchar_t path[160]{};
  swprintf_s(path, L"\\RPC Control\\assist-alpc-private-%lu-%llu-%u",
             GetCurrentProcessId(), GetTickCount64(), test_case);
  UNICODE_STRING name{};
  name.Buffer = path;
  name.Length = USHORT(wcslen(path) * sizeof(wchar_t));
  name.MaximumLength = name.Length + sizeof(wchar_t);
  OBJECT_ATTRIBUTES attributes{};
  attributes.Length = sizeof(attributes);
  attributes.ObjectName = &name;
  attributes.Attributes = 0x40;
  HANDLE listener = nullptr;
  LONG created = NtCreatePort(&listener, &attributes, 48, 512, 0);
  std::printf("CREATE status=%08lX owned=%u\n", ULONG(created),
              unsigned(listener != nullptr));
  if (created < 0)
    return 4;
  std::atomic<bool> stop{false};
  std::atomic<unsigned> received{0}, owned_opened{1}, owned_closed{0};
  LONG listen_status = 0, accept_status = 0, complete_status = 0,
       receive_status = 0, reply_status = 0;
  std::thread server([&] {
    alignas(16) std::array<BYTE, 1024> buffer{};
    LARGE_INTEGER timeout{};
    timeout.QuadPart = -50000000;
    listen_status = NtReplyWaitReceivePortEx(listener, nullptr, nullptr,
                                             buffer.data(), &timeout);
    if (listen_status != 0)
      return;
    HANDLE peer = nullptr;
    accept_status = NtAcceptConnectPort(&peer, nullptr, buffer.data(), TRUE,
                                        nullptr, nullptr);
    if (accept_status < 0)
      return;
    ++owned_opened;
    complete_status = NtCompleteConnectPort(peer);
    if (complete_status >= 0) {
      while (!stop.load()) {
        timeout.QuadPart = -1000000; // 0.1 s polling bound.
        receive_status = NtReplyWaitReceivePortEx(peer, nullptr, nullptr,
                                                  buffer.data(), &timeout);
        if (receive_status == 0x102)
          continue;
        if (receive_status != 0)
          break;
        auto *message = reinterpret_cast<Message *>(buffer.data());
        if (message->type == 1 && message->total >= 40 &&
            message->total <= buffer.size() &&
            message->data == message->total - 40) {
          ++received;
          if (message->data)
            message->body[0] ^= 0xFF;
          reply_status = NtReplyPort(peer, message);
          break;
        }
      }
    }
    if (CloseHandle(peer))
      ++owned_closed;
  });
  SECURITY_QUALITY_OF_SERVICE qos{};
  qos.Length = sizeof(qos);
  qos.ImpersonationLevel = SecurityImpersonation;
  qos.ContextTrackingMode = SECURITY_DYNAMIC_TRACKING;
  qos.EffectiveOnly = TRUE;
  HANDLE client = nullptr;
  ULONG maximum = 0;
  LONG connected = NtConnectPort(&client, &name, &qos, nullptr, nullptr,
                                 &maximum, nullptr, nullptr);
  std::printf("CONNECT status=%08lX owned=%u maximum=%lu\n", ULONG(connected),
              unsigned(client != nullptr), maximum);
  if (connected >= 0)
    ++owned_opened;
  void *send_page =
      VirtualAlloc(nullptr, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
  void *receive_page =
      VirtualAlloc(nullptr, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
  void *length_page =
      VirtualAlloc(nullptr, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
  if (!send_page || !receive_page || !length_page)
    ExitProcess(7);
  std::memset(send_page, 0xA5, 4096);
  std::memset(receive_page, 0xA5, 4096);
  std::memset(length_page, 0xA5, 4096);
  ClientMessage initial{};
  initial.data = 48;
  initial.total = sizeof(initial);
  std::memset(initial.body, 0xC5, sizeof(initial.body));
  std::memcpy(send_page, &initial, sizeof(initial));
  void *send_message = send_page, *receive_message = send_page,
       *send_attributes = nullptr, *receive_attributes = nullptr;
  SIZE_T *capacity = static_cast<SIZE_T *>(length_page);
  *capacity = 952;
  LARGE_INTEGER timer{};
  timer.QuadPart = -5000000; // 0.5 s, 100 ns ticks.
  LARGE_INTEGER *timeout = &timer;
  ULONG flags = 0x20000;
  HANDLE argument_port = client, extra_event = nullptr;
  DWORD old_protection = 0;
  constexpr SIZE_T header = offsetof(ClientMessage, body);
  switch (test_case) {
  case 0:
    break;
  case 1:
    send_message = nullptr;
    break;
  case 2:
    send_message = reinterpret_cast<void *>(1);
    break;
  case 3:
    send_message = static_cast<BYTE *>(send_page) + 1;
    std::memcpy(send_message, &initial, sizeof(initial));
    break;
  case 4:
    receive_message = nullptr;
    break;
  case 5:
    receive_message = reinterpret_cast<void *>(1);
    break;
  case 6:
    receive_message = static_cast<BYTE *>(receive_page) + 1;
    break;
  case 7:
    capacity = nullptr;
    break;
  case 8:
    capacity = reinterpret_cast<SIZE_T *>(1);
    break;
  case 9:
    capacity = reinterpret_cast<SIZE_T *>(static_cast<BYTE *>(length_page) + 1);
    {
      SIZE_T value = 952;
      std::memcpy(capacity, &value, sizeof(value));
    }
    break;
  case 10:
    *capacity = 0;
    break;
  case 11:
    *capacity = header - 1;
    break;
  case 12:
    *capacity = header;
    break;
  case 13:
    *capacity = sizeof(initial) - 1;
    break;
  case 14:
    *capacity = sizeof(initial);
    break;
  case 15:
    *capacity = 1024;
    break;
  case 16:
    reinterpret_cast<ClientMessage *>(send_page)->total = USHORT(header - 1);
    break;
  case 17:
    reinterpret_cast<ClientMessage *>(send_page)->total = USHORT(header);
    reinterpret_cast<ClientMessage *>(send_page)->data = 0;
    break;
  case 18:
    reinterpret_cast<ClientMessage *>(send_page)->data = 49;
    break;
  case 19:
    reinterpret_cast<ClientMessage *>(send_page)->total = 1025;
    break;
  case 20:
    argument_port = nullptr;
    break;
  case 21:
    argument_port = reinterpret_cast<HANDLE>(static_cast<intptr_t>(-1));
    break;
  case 22:
    extra_event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (!extra_event)
      ExitProcess(8);
    argument_port = extra_event;
    ++owned_opened;
    break;
  case 23:
    flags = 0;
    break;
  case 24:
    flags = 1;
    break;
  case 25:
    flags = 0xFFFFFFFF;
    break;
  case 26:
    send_attributes = reinterpret_cast<void *>(1);
    break;
  case 27:
    receive_attributes = reinterpret_cast<void *>(1);
    break;
  case 28:
    timeout = reinterpret_cast<LARGE_INTEGER *>(1);
    break;
  case 29:
    timeout = reinterpret_cast<LARGE_INTEGER *>(
        static_cast<BYTE *>(length_page) + 33);
    std::memcpy(timeout, &timer, sizeof(timer));
    break;
  case 30:
    receive_message = receive_page;
    if (!VirtualProtect(send_page, 4096, PAGE_READONLY, &old_protection))
      ExitProcess(9);
    break;
  case 31:
    receive_message = receive_page;
    if (!VirtualProtect(receive_page, 4096, PAGE_READONLY, &old_protection))
      ExitProcess(9);
    break;
  case 32:
    if (!VirtualProtect(send_page, 4096, PAGE_READWRITE | PAGE_GUARD,
                        &old_protection))
      ExitProcess(9);
    break;
  case 33:
    receive_message = receive_page;
    if (!VirtualProtect(receive_page, 4096, PAGE_READWRITE | PAGE_GUARD,
                        &old_protection))
      ExitProcess(9);
    break;
  case 34:
    if (!VirtualProtect(length_page, 4096, PAGE_READWRITE | PAGE_GUARD,
                        &old_protection))
      ExitProcess(9);
    break;
  case 35:
    *capacity = SIZE_MAX;
    break;
  case 36:
    send_message = nullptr;
    receive_message = nullptr;
    capacity = nullptr;
    break;
  case 37:
    receive_message = receive_page;
    if (!VirtualProtect(receive_page, 4096, PAGE_READONLY | PAGE_GUARD,
                        &old_protection))
      ExitProcess(9);
    break;
  case 38:
    if (!VirtualProtect(length_page, 4096, PAGE_READONLY, &old_protection))
      ExitProcess(9);
    break;
  case 39:
    timeout = nullptr;
    break;
  default:
    ExitProcess(10);
  }
  ULONG send_before = protection(send_page),
        receive_before = protection(receive_page),
        length_before = protection(length_page);
  SetLastError(0x12345678);
  Result result{0xDEADBEEF, 0};
  if (connected >= 0)
    result = invoke(NtAlpcSendWaitReceivePort, argument_port, flags,
                    send_message, send_attributes, receive_message, capacity,
                    receive_attributes, timeout);
  DWORD last_error = GetLastError();
  ULONG send_after = protection(send_page),
        receive_after = protection(receive_page),
        length_after = protection(length_page);
  // Snapshot only after recording guard state; do not consume an unvisited
  // guard.
  std::printf(
      "CASE id=%u flags=%08lX status=%08lX exception=%08lX last-error=%08lX "
      "send-before=%08lX send-after=%08lX receive-before=%08lX "
      "receive-after=%08lX length-before=%08lX length-after=%08lX",
      test_case, flags, result.status, result.exception, last_error,
      send_before, send_after, receive_before, receive_after, length_before,
      length_after);
  if (!(send_after & PAGE_GUARD))
    dump_page("send", send_page, 128);
  if (!(receive_after & PAGE_GUARD))
    dump_page("receive", receive_page, 128);
  if (!(length_after & PAGE_GUARD))
    dump_page("length", length_page, 64);
  std::puts("");
  if (extra_event && CloseHandle(extra_event))
    ++owned_closed;
  stop = true;
  if (client && CloseHandle(client))
    ++owned_closed;
  server.join();
  if (CloseHandle(listener))
    ++owned_closed;
  std::printf("SERVER case=%u listen=%08lX accept=%08lX complete=%08lX "
              "receive=%08lX reply=%08lX received=%u\n",
              test_case, ULONG(listen_status), ULONG(accept_status),
              ULONG(complete_status), ULONG(receive_status),
              ULONG(reply_status), received.load());
  unsigned pages_closed = 0;
  for (void *page : {send_page, receive_page, length_page})
    if (VirtualFree(page, 0, MEM_RELEASE))
      ++pages_closed;
  if (!GetProcessHandleCount(GetCurrentProcess(), &handles_after))
    return 5;
  std::printf("SUMMARY handles-before=%lu handles-after=%lu\n", handles_before,
              handles_after);
  std::printf("OWNED port-handles-opened=%u port-handles-closed=%u "
              "server-thread-joined=1\n",
              owned_opened.load(), owned_closed.load());
  unsigned expected = test_case == 22 ? 4 : 3;
  std::printf("END case=%u pages-opened=3 pages-released=%u\n", test_case,
              pages_closed);
  return connected == 0 && owned_opened == expected &&
                 owned_closed == expected && pages_closed == 3 &&
                 (test_case != 0 ||
                  (result.status == 0 && result.exception == 0 &&
                   received == 1 && reply_status == 0))
             ? 0
             : 6;
}
int main(int argc, char **argv) {
  const char *profile = argc > 1 ? argv[1] : "unknown";
  for (unsigned test_case = 0; test_case < 40; ++test_case) {
    int result = run_case(test_case, profile);
    if (result)
      return result;
  }
  std::puts("MATRIX cases=40 complete=1");
  return 0;
}
