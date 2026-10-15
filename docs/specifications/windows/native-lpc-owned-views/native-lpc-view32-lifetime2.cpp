#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <thread>
#include <windows.h>
#include <winternl.h>

// Low-level LPC WoW64 uses the 64-bit message and view layouts (PHNT).
struct alignas(8) Message {
  USHORT data_length, total_length, type, data_offset;
  uint64_t process, thread;
  ULONG id, pad;
  uint64_t client_view_size;
};
#if defined(_M_IX86)
struct View {
  ULONG length;
  uint32_t section;
  ULONG offset;
  uint32_t size, base, remote_base;
};
#else
struct alignas(8) View {
  ULONG length, pad0;
  uint64_t section;
  ULONG offset, pad1;
  uint64_t size, base, remote_base;
};
#endif
struct alignas(8) RemoteView {
  ULONG length, pad;
  uint64_t size, base;
};
static_assert(sizeof(Message) == 40);
static_assert(sizeof(View) == (sizeof(void *) == 4 ? 24 : 48));
static_assert(sizeof(RemoteView) == 24);

using CreatePort = LONG(NTAPI *)(HANDLE *, OBJECT_ATTRIBUTES *, ULONG, ULONG,
                                 ULONG);
using ListenPort = LONG(NTAPI *)(HANDLE, void *);
using AcceptPort = LONG(NTAPI *)(HANDLE *, void *, void *, BOOLEAN, void *,
                                 void *);
using CompletePort = LONG(NTAPI *)(HANDLE);
using SecureConnect = LONG(NTAPI *)(HANDLE *, UNICODE_STRING *,
                                    SECURITY_QUALITY_OF_SERVICE *, void *,
                                    void *, void *, ULONG *, void *, ULONG *);
using CreateSection = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *,
                                    LARGE_INTEGER *, ULONG, ULONG, HANDLE);
using QueryObject = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using Unmap = LONG(NTAPI *)(HANDLE, void *);

static void fail(const char *what, ULONG status = 0) {
  printf("failure=%s status=%08lX error=%lu\n", what, status, GetLastError());
  ExitProcess(21);
}
template <class T> static T symbol(HMODULE module, const char *name) {
  auto p = GetProcAddress(module, name);
  if (!p)
    fail(name);
  return reinterpret_cast<T>(p);
}
static void bytes(const char *name, const void *data, size_t size) {
  printf("bytes name=%s size=%zu hex=", name, size);
  auto p = static_cast<const unsigned char *>(data);
  for (size_t i = 0; i < size; ++i)
    printf("%02X", p[i]);
  puts("");
}
static void region(const char *phase, const char *name, uint64_t address) {
  MEMORY_BASIC_INFORMATION info{};
  SIZE_T got =
      VirtualQuery(reinterpret_cast<void *>(static_cast<uintptr_t>(address)),
                   &info, sizeof info);
  unsigned char sample[2] = {0xA5, 0xA5};
  SIZE_T count = 0;
  BOOL readable = ReadProcessMemory(
      GetCurrentProcess(),
      reinterpret_cast<void *>(static_cast<uintptr_t>(address)), sample,
      sizeof sample, &count);
  printf("region phase=%s name=%s address=%llX returned=%zu base=%llX "
         "allocation=%llX size=%zu state=%lX protect=%lX type=%lX read=%u "
         "got=%zu data=%02X%02X\n",
         phase, name, static_cast<unsigned long long>(address), got,
         static_cast<unsigned long long>(
             reinterpret_cast<uintptr_t>(info.BaseAddress)),
         static_cast<unsigned long long>(
             reinterpret_cast<uintptr_t>(info.AllocationBase)),
         info.RegionSize, info.State, info.Protect, info.Type,
         unsigned(readable), count, unsigned(sample[0]), unsigned(sample[1]));
}
static void object(QueryObject query, HANDLE handle, const char *name) {
  alignas(16) unsigned char data[2048]{};
  ULONG returned = 0;
  LONG status = query(handle, 0, data, 56, &returned);
  printf("object name=%s basic-status=%08lX returned=%lu", name, ULONG(status),
         returned);
  if (status >= 0 && returned >= 8) {
    ULONG grant = 0;
    memcpy(&grant, data + 4, 4);
    ULONG handles = 0, pointers = 0;
    memcpy(&handles, data + 8, 4);
    memcpy(&pointers, data + 12, 4);
    printf(" grant=%08lX handle-count=%lu pointer-count=%lu", grant, handles,
           pointers);
  }
  puts("");
  memset(data, 0, sizeof data);
  returned = 0;
  status = query(handle, 2, data, sizeof data, &returned);
  printf("object name=%s type-status=%08lX returned=%lu", name, ULONG(status),
         returned);
  if (status >= 0) {
    auto u = reinterpret_cast<UNICODE_STRING *>(data);
    auto start = reinterpret_cast<uintptr_t>(data), end = start + sizeof data,
         ptr = reinterpret_cast<uintptr_t>(u->Buffer);
    if ((u->Length & 1) || u->Length > 128 || ptr < start || ptr > end ||
        u->Length > end - ptr)
      fail("bounded-object-type");
    printf(" type-hex=");
    for (unsigned i = 0; i < u->Length / 2; ++i)
      printf("%04X", unsigned(u->Buffer[i]));
  }
  puts("");
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  const char *mode = argc > 1 ? argv[1] : "client-first";
  if (strcmp(mode, "client-first") && strcmp(mode, "server-first") &&
      strcmp(mode, "duplicate-client") && strcmp(mode, "unmap-client") &&
      strcmp(mode, "unmap-private-client"))
    fail("case-name");
  printf("case=%s\n", mode);
  printf("profile pid=%lu width=%zu\n", GetCurrentProcessId(), sizeof(void *));
  unsigned closed = 0;

  // The watchdog only terminates this task-owned control if the handshake
  // stalls.
  std::thread([] {
    Sleep(60000);
    ExitProcess(78);
  }).detach();
  HMODULE n = GetModuleHandleW(L"ntdll.dll");
  auto create = symbol<CreatePort>(n, "NtCreatePort");
  auto listen = symbol<ListenPort>(n, "NtListenPort");
  auto accept = symbol<AcceptPort>(n, "NtAcceptConnectPort");
  auto complete = symbol<CompletePort>(n, "NtCompleteConnectPort");
  auto connect = symbol<SecureConnect>(n, "NtSecureConnectPort");
  auto section_create = symbol<CreateSection>(n, "NtCreateSection");
  auto query = symbol<QueryObject>(n, "NtQueryObject");
  auto unmap = symbol<Unmap>(n, "NtUnmapViewOfSection");
  wchar_t text[128]{};
  int length =
      swprintf_s(text, L"\\RPC Control\\RaxLpcView-%lu", GetCurrentProcessId());
  if (length <= 0)
    fail("owned-port-name");
  UNICODE_STRING name{};
  name.Buffer = text;
  name.Length = USHORT(length * 2);
  name.MaximumLength = USHORT((length + 1) * 2);
  OBJECT_ATTRIBUTES attributes{};
  attributes.Length = sizeof attributes;
  attributes.ObjectName = &name;
  attributes.Attributes = 0x40;
  HANDLE server = nullptr, section = nullptr, client = nullptr, peer = nullptr;
  LONG status = create(&server, &attributes, 64, 256, 0);
  printf("create-port status=%08lX handle=%llX width=%zu\n", ULONG(status),
         static_cast<unsigned long long>(reinterpret_cast<uintptr_t>(server)),
         sizeof(void *));
  if (status < 0)
    fail("create-owned-port", ULONG(status));
  LARGE_INTEGER size{};
  size.QuadPart = 65536;
  status = section_create(&section, 0xF001F, nullptr, &size, PAGE_READWRITE,
                          SEC_COMMIT, nullptr);
  printf("create-section status=%08lX handle=%llX maximum=%lld\n",
         ULONG(status),
         static_cast<unsigned long long>(reinterpret_cast<uintptr_t>(section)),
         size.QuadPart);
  if (status < 0)
    fail("create-owned-section", ULONG(status));
  RemoteView remote{};
  remote.length = sizeof remote;
  unsigned char request_header[40]{}, request_data[32]{};
  LONG listen_status = 0, accept_status = 0, complete_status = 0;
  std::thread worker([&] {
    alignas(16) unsigned char request[4096]{};
    listen_status = listen(server, request);
    if (listen_status < 0)
      fail("listen-owned-port", ULONG(listen_status));
    auto h = reinterpret_cast<Message *>(request);
    if (h->total_length < sizeof(Message) || h->total_length > sizeof request ||
        h->data_length > h->total_length - sizeof(Message) ||
        h->data_length != 32)
      fail("bounded-connection-request");
    memcpy(request_header, h, 40);
    memcpy(request_data, request + 40, 32);
    request[40] ^= 0x5A;
    accept_status = accept(&peer, nullptr, request, TRUE, nullptr, &remote);
    if (accept_status < 0)
      fail("accept-owned-port", ULONG(accept_status));
    complete_status = complete(peer);
    if (complete_status < 0)
      fail("complete-owned-port", ULONG(complete_status));
  });
  SECURITY_QUALITY_OF_SERVICE qos{};
  qos.Length = sizeof qos;
  qos.ImpersonationLevel = SecurityImpersonation;
  qos.ContextTrackingMode = SECURITY_DYNAMIC_TRACKING;
  qos.EffectiveOnly = TRUE;
  View view{};
  view.length = sizeof view;
  view.section = reinterpret_cast<uintptr_t>(section);
  view.size = 65536;
  RemoteView returned{};
  returned.length = sizeof returned;
  ULONG max_message = 0, info_length = 32;
  unsigned char info[32];
  for (unsigned i = 0; i < 32; ++i)
    info[i] = static_cast<unsigned char>(i);
  bytes("client-view-before", &view, sizeof view);
  bytes("server-view-before", &returned, sizeof returned);
  bytes("information-before", info, sizeof info);
  status = connect(&client, &name, &qos, &view, nullptr, &returned,
                   &max_message, info, &info_length);
  printf("connect status=%08lX handle=%llX maximum-message=%lu "
         "information-length=%lu\n",
         ULONG(status),
         static_cast<unsigned long long>(reinterpret_cast<uintptr_t>(client)),
         max_message, info_length);
  if (status < 0)
    fail("connect-owned-port", ULONG(status));
  worker.join();
  printf("server listen=%08lX accept=%08lX complete=%08lX\n",
         ULONG(listen_status), ULONG(accept_status), ULONG(complete_status));
  bytes("request-header", request_header, sizeof request_header);
  bytes("request-data", request_data, sizeof request_data);
  bytes("peer-client-view", &remote, sizeof remote);

  bytes("client-view-after", &view, sizeof view);
  bytes("server-view-after", &returned, sizeof returned);
  bytes("information-after", info, sizeof info);
  if (info_length != 32 || info[0] != 0x5A || view.size != 65536 ||
      view.base == 0 || view.remote_base != remote.base || remote.size != 65536)
    fail("actual-handshake-values");
  object(query, client, "client-port");
  object(query, peer, "server-peer");
  object(query, section, "section");
  region("connected", "client", view.base);
  region("connected", "server", remote.base);
  unsigned char a = 0x69, b = 0x96;
  SIZE_T written = 0;
  void *local = reinterpret_cast<void *>(static_cast<uintptr_t>(view.base));
  void *other = reinterpret_cast<void *>(static_cast<uintptr_t>(remote.base));
  if (!WriteProcessMemory(GetCurrentProcess(), local, &a, 1, &written) ||
      written != 1)
    fail("write-client-alias");
  if (!WriteProcessMemory(GetCurrentProcess(),
                          static_cast<unsigned char *>(other) + 1, &b, 1,
                          &written) ||
      written != 1)
    fail("write-server-alias");
  region("canary", "client", view.base);
  region("canary", "server", remote.base);
  unsigned char observed[2]{};
  SIZE_T got = 0;
  if (!ReadProcessMemory(GetCurrentProcess(), local, observed, 2, &got) ||
      got != 2 || observed[0] != a || observed[1] != b)
    fail("bidirectional-shared-alias");
  HANDLE duplicate = nullptr;
  if (!strcmp(mode, "duplicate-client")) {
    if (!DuplicateHandle(GetCurrentProcess(), client, GetCurrentProcess(),
                         &duplicate, 0, FALSE, DUPLICATE_SAME_ACCESS))
      fail("duplicate-client-port");
    object(query, duplicate, "duplicate-client-port");
  }
  bool reused_private = false;
  if (!strcmp(mode, "unmap-client") || !strcmp(mode, "unmap-private-client")) {
    LONG s = unmap(GetCurrentProcess(), local);
    printf("manual-unmap status=%08lX\n", ULONG(s));
    if (s < 0)
      fail("manual-unmap-client", ULONG(s));
    region("manual-unmap", "client", view.base);
    region("manual-unmap", "server", remote.base);
    if (!strcmp(mode, "unmap-private-client")) {
      void *allocated =
          VirtualAlloc(local, 65536, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
      if (allocated != local)
        fail("reuse-client-address-private");
      reused_private = true;
      unsigned char marker[2] = {0x12, 0x34};
      SIZE_T written = 0;
      if (!WriteProcessMemory(GetCurrentProcess(), local, marker, 2,
                              &written) ||
          written != 2)
        fail("mark-reused-private");
      region("private-reused", "client", view.base);
    }
  }
  auto close_owned = [&](HANDLE &h, const char *name) {
    if (!CloseHandle(h))
      fail(name);
    h = nullptr;
    ++closed;
    printf("closed name=%s\n", name);
  };
  auto observe = [&](const char *phase) {
    region(phase, "client", view.base);
    region(phase, "server", remote.base);
  };
  close_owned(section, "section");
  observe("section-closed");
  if (!strcmp(mode, "server-first")) {
    close_owned(peer, "server-peer");
    observe("peer-closed-first");
    close_owned(client, "client-port");
    observe("client-closed-last");
  } else {
    close_owned(client, "client-port");
    observe("client-closed");
    if (duplicate) {
      close_owned(duplicate, "client-duplicate");
      observe("duplicate-closed");
    }
    close_owned(peer, "server-peer");
    observe("peer-closed");
  }
  close_owned(server, "listening-port");
  if (reused_private) {
    MEMORY_BASIC_INFORMATION q{};
    VirtualQuery(local, &q, sizeof q);
    if (q.State == MEM_COMMIT && q.Type == MEM_PRIVATE &&
        q.AllocationBase == local) {
      if (!VirtualFree(local, 0, MEM_RELEASE))
        fail("cleanup-reused-private");
      puts("cleanup-private present=1 freed=1");
    } else
      puts("cleanup-private present=0 freed=0");
  }
  unsigned unmapped = 0;
  for (uint64_t address : {uint64_t(view.base), remote.base}) {
    MEMORY_BASIC_INFORMATION q{};
    void *p = reinterpret_cast<void *>(static_cast<uintptr_t>(address));
    if (VirtualQuery(p, &q, sizeof q) && q.State == MEM_COMMIT &&
        q.Type == MEM_MAPPED && q.AllocationBase == p) {
      LONG s = unmap(GetCurrentProcess(), p);
      printf("cleanup-view address=%llX status=%08lX\n",
             static_cast<unsigned long long>(address), ULONG(s));
      if (s < 0)
        fail("unmap-owned-view", ULONG(s));
      ++unmapped;
    }
  }
  printf(
      "complete status=0 closed-handles=%u cleanup-unmaps=%u alias=1 reply=1\n",
      closed, unmapped);
  return 0;
}
