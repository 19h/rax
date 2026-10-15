#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <thread>
#include <vector>
#include <windows.h>
#include <winternl.h>

struct alignas(8) View {
  ULONG length, pad0;
  uint64_t section;
  ULONG offset, pad1;
  uint64_t size, base, remote;
};
struct alignas(8) Remote {
  ULONG length, pad;
  uint64_t size, base;
};
static_assert(sizeof(View) == 48 && sizeof(Remote) == 24);
using Connect = LONG(NTAPI *)(HANDLE *, UNICODE_STRING *,
                              SECURITY_QUALITY_OF_SERVICE *, void *, void *,
                              void *, ULONG *, void *, ULONG *);
using CreateSection = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *,
                                    LARGE_INTEGER *, ULONG, ULONG, HANDLE);
struct Arguments {
  uintptr_t values[9];
};
static LONG invoke(Connect fn, const Arguments &a, ULONG *exception) {
  __try {
    return fn(reinterpret_cast<HANDLE *>(a.values[0]),
              reinterpret_cast<UNICODE_STRING *>(a.values[1]),
              reinterpret_cast<SECURITY_QUALITY_OF_SERVICE *>(a.values[2]),
              reinterpret_cast<void *>(a.values[3]),
              reinterpret_cast<void *>(a.values[4]),
              reinterpret_cast<void *>(a.values[5]),
              reinterpret_cast<ULONG *>(a.values[6]),
              reinterpret_cast<void *>(a.values[7]),
              reinterpret_cast<ULONG *>(a.values[8]));
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
    return LONG(0xFFFFFFFF);
  }
}
static void fail(const char *why) {
  printf("failure=%s error=%lu\n", why, GetLastError());
  ExitProcess(21);
}
static void dump(const char *name, const void *bytes, size_t size) {
  printf("bytes name=%s size=%zu hex=", name, size);
  auto b = static_cast<const unsigned char *>(bytes);
  for (size_t i = 0; i < size; ++i)
    printf("%02X", b[i]);
  puts("");
}
struct Case {
  std::string name;
  unsigned out = 0, name_mode = 0, qos = 0, client = 0, sid = 0, server = 0,
           message = 0, data = 0, length_pointer = 0;
  ULONG length = 32, qos_length = 0, impersonation = 2, tracking = 1,
        effective = 1, client_length = 48, server_length = 24;
  uint64_t section = 0;
  ULONG offset = 0;
  uint64_t view_size = 65536;
};
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  std::thread([] {
    Sleep(60000);
    ExitProcess(78);
  }).detach();
  auto n = GetModuleHandleW(L"ntdll.dll");
  auto fn = reinterpret_cast<Connect>(GetProcAddress(n, "NtSecureConnectPort"));
  auto create =
      reinterpret_cast<CreateSection>(GetProcAddress(n, "NtCreateSection"));
  if (!fn || !create)
    fail("native-exports");
  HANDLE section = nullptr;
  LARGE_INTEGER maximum{};
  maximum.QuadPart = 65536;
  LONG s = create(&section, 0xF001F, nullptr, &maximum, PAGE_READWRITE,
                  SEC_COMMIT, nullptr);
  if (s < 0)
    fail("owned-section");
  HANDLE event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
  if (!event)
    fail("owned-event");
  auto block = static_cast<unsigned char *>(VirtualAlloc(
      nullptr, 8 * 4096, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!block)
    fail("owned-pages");
  wchar_t missing[160]{};
  LARGE_INTEGER nonce{};
  QueryPerformanceCounter(&nonce);
  int chars = swprintf_s(missing, L"\\Windows\\RaxLpcMissing-%lu-%llX",
                         GetCurrentProcessId(),
                         static_cast<unsigned long long>(nonce.QuadPart));
  if (chars <= 0)
    fail("bounded-missing-name");
  std::vector<Case> cases;
  cases.push_back(Case{"baseline"});
  for (unsigned value : {1u, 2u, 3u, 4u, 5u}) {
    Case c{"output-" + std::to_string(value)};
    c.out = value;
    cases.push_back(c);
  }
  for (unsigned value : {1u, 2u, 3u, 4u, 5u, 6u, 7u, 8u}) {
    Case c{"name-" + std::to_string(value)};
    c.name_mode = value;
    cases.push_back(c);
  }
  for (unsigned value : {1u, 2u}) {
    Case c{"qos-pointer-" + std::to_string(value)};
    c.qos = value;
    cases.push_back(c);
  }
  for (ULONG value : {1u, 11u, 12u, 13u, 0xffffffffu}) {
    Case c{"qos-length-" + std::to_string(value)};
    c.qos_length = value;
    cases.push_back(c);
  }
  for (ULONG value : {0u, 1u, 3u, 4u, 0xffffffffu}) {
    Case c{"impersonation-" + std::to_string(value)};
    c.impersonation = value;
    cases.push_back(c);
  }
  for (ULONG value : {0u, 2u, 255u}) {
    Case c{"tracking-" + std::to_string(value)};
    c.tracking = value;
    cases.push_back(c);
    c.name = "effective-" + std::to_string(value);
    c.tracking = 1;
    c.effective = value;
    cases.push_back(c);
  }
  for (unsigned value : {1u, 2u}) {
    Case c{"client-pointer-" + std::to_string(value)};
    c.client = value;
    cases.push_back(c);
    c.name = "server-pointer-" + std::to_string(value);
    c.client = 0;
    c.server = value;
    cases.push_back(c);
  }
  for (ULONG value : {0u, 1u, 24u, 47u, 49u}) {
    Case c{"client-length-" + std::to_string(value)};
    c.client_length = value;
    cases.push_back(c);
  }
  for (ULONG value : {0u, 1u, 23u, 25u, 48u}) {
    Case c{"server-length-" + std::to_string(value)};
    c.server_length = value;
    cases.push_back(c);
  }
  for (uint64_t value :
       {0ull, 1ull, ~0ull,
        0x100000000ull | reinterpret_cast<uintptr_t>(section)}) {
    Case c{"section-" + std::to_string(value)};
    c.section = value;
    c.client = 3;
    cases.push_back(c);
  }
  {
    Case c{"section-event"};
    c.client = 3;
    c.section = reinterpret_cast<uintptr_t>(event);
    cases.push_back(c);
  }
  for (ULONG value : {1u, 65536u}) {
    Case c{"offset-" + std::to_string(value)};
    c.offset = value;
    cases.push_back(c);
  }
  for (uint64_t value : {0ull, 1ull, 65537ull}) {
    Case c{"view-size-" + std::to_string(value)};
    c.view_size = value;
    cases.push_back(c);
  }
  for (unsigned value : {1u, 2u, 3u, 4u, 5u}) {
    Case c{"sid-" + std::to_string(value)};
    c.sid = value;
    cases.push_back(c);
  }
  for (unsigned value : {1u, 2u, 3u, 4u}) {
    Case c{"message-pointer-" + std::to_string(value)};
    c.message = value;
    cases.push_back(c);
    c.name = "length-pointer-" + std::to_string(value);
    c.message = 0;
    c.length_pointer = value;
    cases.push_back(c);
  }
  for (unsigned value : {1u, 2u, 3u}) {
    Case c{"data-pointer-" + std::to_string(value)};
    c.data = value;
    cases.push_back(c);
  }
  for (ULONG value : {0u, 1u, 31u, 48u, 64u, 128u, 129u, 4096u}) {
    Case c{"information-length-" + std::to_string(value)};
    c.length = value;
    cases.push_back(c);
  }
  for (unsigned priority = 0; priority < 8; ++priority) {
    Case c{"priority-" + std::to_string(priority)};
    c.out = (priority & 1) ? 2 : 0;
    c.name_mode = (priority & 2) ? 2 : 0;
    c.qos = (priority & 4) ? 2 : 0;
    cases.push_back(c);
  }
  printf("profile pid=%lu width=%zu cases=%zu\n", GetCurrentProcessId(),
         sizeof(void *), cases.size());
  for (size_t index = 0; index < cases.size(); ++index) {
    const auto &c = cases[index];
    memset(block, 0xA5, 8 * 4096);
    wchar_t name_text[160];
    memcpy(name_text, missing, sizeof missing);
    UNICODE_STRING name{};
    name.Buffer = name_text;
    name.Length = USHORT(chars * 2);
    name.MaximumLength = USHORT((chars + 1) * 2);
    if (c.name_mode == 3)
      name.Length |= 1;
    if (c.name_mode == 4)
      name.Buffer = nullptr;
    if (c.name_mode == 5)
      name.Buffer = reinterpret_cast<wchar_t *>(1);
    if (c.name_mode == 6)
      name.MaximumLength = 0;
    if (c.name_mode == 7)
      name.Length = 0;
    if (c.name_mode == 8)
      for (auto &v : name_text)
        if (v >= L'A' && v <= L'Z')
          v += L'a' - L'A';
    SECURITY_QUALITY_OF_SERVICE qos{};
    qos.Length = c.qos_length;
    memcpy(reinterpret_cast<unsigned char *>(&qos) + 4, &c.impersonation, 4);
    qos.ContextTrackingMode = static_cast<BOOLEAN>(c.tracking);
    qos.EffectiveOnly = static_cast<BOOLEAN>(c.effective);
    View view{};
    view.length = c.client_length;
    view.section =
        c.client == 3 ? c.section : reinterpret_cast<uintptr_t>(section);
    view.offset = c.offset;
    view.size = c.view_size;
    Remote remote{};
    remote.length = c.server_length;
    unsigned char sid[96]{};
    sid[0] = 1;
    sid[1] = 1;
    sid[7] = 5;
    sid[8] = 18;
    if (c.sid == 3)
      sid[0] = 2;
    if (c.sid == 4)
      sid[1] = 16;
    if (c.sid == 5)
      sid[1] = 0;
    auto select = [](unsigned mode, uintptr_t valid, uintptr_t crossed) {
      return mode == 1   ? uintptr_t(0)
             : mode == 2 ? uintptr_t(1)
             : mode == 3 ? valid + 1
             : mode == 4 ? crossed
                         : valid;
    };
    uintptr_t out = select(c.out, reinterpret_cast<uintptr_t>(block + 64),
                           reinterpret_cast<uintptr_t>(block + 4093));
    uintptr_t message =
        select(c.message, reinterpret_cast<uintptr_t>(block + 2 * 4096 + 64),
               reinterpret_cast<uintptr_t>(block + 3 * 4096 - 2));
    uintptr_t length = select(
        c.length_pointer, reinterpret_cast<uintptr_t>(block + 4 * 4096 + 64),
        reinterpret_cast<uintptr_t>(block + 5 * 4096 - 2));
    uintptr_t data = c.data == 1 ? 0
                     : c.data == 2
                         ? 1
                         : reinterpret_cast<uintptr_t>(
                               block + 6 * 4096 + (c.data == 3 ? 4080 : 64));
    memcpy(block + 4 * 4096 + 64 + (c.length_pointer == 3 ? 1 : 0), &c.length,
           4);
    if (c.length_pointer == 4)
      memcpy(block + 5 * 4096 - 2, &c.length, 4);
    Arguments a{{out,
                 c.name_mode == 1   ? 0
                 : c.name_mode == 2 ? 1
                                    : reinterpret_cast<uintptr_t>(&name),
                 c.qos == 1   ? 0
                 : c.qos == 2 ? 1
                              : reinterpret_cast<uintptr_t>(&qos),
                 c.client == 1   ? 0
                 : c.client == 2 ? 1
                                 : reinterpret_cast<uintptr_t>(&view),
                 c.sid == 1   ? 0
                 : c.sid == 2 ? 1
                              : reinterpret_cast<uintptr_t>(sid),
                 c.server == 1   ? 0
                 : c.server == 2 ? 1
                                 : reinterpret_cast<uintptr_t>(&remote),
                 message, data, length}};
    printf("input case=%zu name-length=%hu name-maximum=%hu args=", index,
           name.Length, name.MaximumLength);
    for (uintptr_t value : a.values)
      printf("%llX,", static_cast<unsigned long long>(value));
    puts("");
    dump("input-client-view", &view, sizeof view);
    dump("input-server-view", &remote, sizeof remote);
    dump("input-qos", &qos, sizeof qos);
    dump("input-sid", sid, sizeof sid);
    dump("input-name-header", &name, sizeof name);
    dump("input-name-text", name_text, chars * 2);
    uint64_t input_section = view.section;
    DWORD old = 0;
    if (c.out == 4 && !VirtualProtect(block + 4096, 4096, PAGE_NOACCESS, &old))
      fail("out-guard");
    if (c.out == 5 && !VirtualProtect(block, 4096, PAGE_READONLY, &old))
      fail("out-readonly");
    if (c.message == 4 &&
        !VirtualProtect(block + 3 * 4096, 4096, PAGE_NOACCESS, &old))
      fail("message-guard");
    if (c.length_pointer == 4 &&
        !VirtualProtect(block + 5 * 4096, 4096, PAGE_NOACCESS, &old))
      fail("length-guard");
    if (c.data == 3 &&
        !VirtualProtect(block + 7 * 4096, 4096, PAGE_NOACCESS, &old))
      fail("data-guard");
    DWORD before_handles = 0, after_handles = 0;
    if (!GetProcessHandleCount(GetCurrentProcess(), &before_handles))
      fail("handle-count-before");
    ULONG exception = 0;
    LONG status = invoke(fn, a, &exception);
    if (!GetProcessHandleCount(GetCurrentProcess(), &after_handles))
      fail("handle-count-after");
    if (!VirtualProtect(block, 8 * 4096, PAGE_READWRITE, &old))
      fail("restore-owned-pages");
    printf("case=%zu name=%s status=%08lX exception=%08lX out=%u name-mode=%u "
           "qos=%u client=%u sid=%u server=%u message=%u data=%u "
           "length-pointer=%u length=%lu qos-length=%lu impersonation=%lu "
           "tracking=%lu effective=%lu client-length=%lu server-length=%lu "
           "section=%llX offset=%lu view-size=%llu handles-before=%lu "
           "handles-after=%lu\n",
           index, c.name.c_str(), ULONG(status), exception, c.out, c.name_mode,
           c.qos, c.client, c.sid, c.server, c.message, c.data,
           c.length_pointer, c.length, c.qos_length, c.impersonation,
           c.tracking, c.effective, c.client_length, c.server_length,
           static_cast<unsigned long long>(input_section), c.offset,
           static_cast<unsigned long long>(c.view_size), before_handles,
           after_handles);
    dump("output", block + (c.out == 4 ? 4096 - 32 : 64), 64);
    dump("message", block + 2 * 4096 + (c.message == 4 ? 4096 - 32 : 64), 64);
    dump("length", block + 4 * 4096 + (c.length_pointer == 4 ? 4096 - 32 : 64),
         64);
    dump("client-view", &view, sizeof view);
    dump("server-view", &remote, sizeof remote);
    dump("information", block + 6 * 4096 + (c.data == 3 ? 4096 - 32 : 64), 64);
    if (status >= 0 && exception == 0)
      fail("unexpected-success-missing-endpoint");
    if (before_handles != after_handles)
      fail("failed-call-handle-leak");
  }
  if (!CloseHandle(event) || !CloseHandle(section) ||
      !VirtualFree(block, 0, MEM_RELEASE))
    fail("owned-cleanup");
  printf("complete cases=%zu closed-handles=2 no-native-success=1\n",
         cases.size());
  return 0;
}
