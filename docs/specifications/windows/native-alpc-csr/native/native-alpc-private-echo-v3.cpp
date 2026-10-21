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
static Result invoke(Send send, HANDLE port, Message *message, SIZE_T *length) {
  __try {
    LARGE_INTEGER timeout{};
    timeout.QuadPart = -50000000; // 5 s, in 100 ns ticks.
    return {ULONG(send(port, 0x20000, message, nullptr, message, length,
                       nullptr, &timeout)),
            0};
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    return {0xDEADBEEF, GetExceptionCode()};
  }
}
int main(int argc, char **argv) {
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
  std::printf("PROFILE %s POINTER_BYTES %zu\n", argc > 1 ? argv[1] : "unknown",
              sizeof(void *));
  wchar_t path[160]{};
  swprintf_s(path, L"\\RPC Control\\assist-alpc-private-%lu-%llu",
             GetCurrentProcessId(), GetTickCount64());
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
  std::atomic<unsigned> received{0};
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
        if (message->type == 1 && message->total == sizeof(Message) &&
            message->data == 48) {
          ++received;
          message->body[0] ^= 0xFF;
          reply_status = NtReplyPort(peer, message);
          break;
        }
      }
    }
    CloseHandle(peer);
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
  Result result{0xDEADBEEF, 0};
  Message message{};
  message.data = 48;
  message.total = sizeof(message);
  std::memset(message.body, 0xC5, sizeof(message.body));
  SIZE_T capacity = 952;
  if (connected >= 0)
    result = invoke(NtAlpcSendWaitReceivePort, client, &message, &capacity);
  stop = true;
  if (client)
    CloseHandle(client);
  server.join();
  CloseHandle(listener);
  std::printf("CALL status=%08lX exception=%08lX capacity=%zu message=",
              result.status, result.exception, capacity);
  for (unsigned i = 0; i < sizeof(message); ++i)
    std::printf("%02X", unsigned(reinterpret_cast<BYTE *>(&message)[i]));
  std::printf("\nSERVER listen=%08lX accept=%08lX complete=%08lX receive=%08lX "
              "reply=%08lX received=%u\n",
              ULONG(listen_status), ULONG(accept_status),
              ULONG(complete_status), ULONG(receive_status),
              ULONG(reply_status), received.load());
  if (!GetProcessHandleCount(GetCurrentProcess(), &handles_after))
    return 5;
  std::printf("SUMMARY handles-before=%lu handles-after=%lu\n", handles_before,
              handles_after);
  return connected == 0 && result.status == 0 && result.exception == 0 &&
                 capacity == sizeof(message) && message.body[0] == 0x3A &&
                 received == 1 && reply_status == 0 &&
                 handles_before == handles_after
             ? 0
             : 6;
}
