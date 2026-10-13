#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <set>
#include <vector>
#include <windows.h>
#include <winternl.h>
using Open = LONG(NTAPI *)(HANDLE, ACCESS_MASK, BOOLEAN, HANDLE *);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
static Open open_token;
static Query query_process, query_object;
static unsigned char *memory;
static HANDLE threads[24], signal_event, ready_event;
static ULONG token_type_index;
static std::set<uintptr_t> input_handles;
struct Entry {
  HANDLE h;
  uintptr_t handles, pointers;
  ULONG access, type, attributes, reserved;
};
static void fail(const char *what) {
  printf("failure=%s error=%lu\n", what, GetLastError());
  ExitProcess(21);
}
static std::vector<Entry> entries() {
  std::vector<uintptr_t> b(131072);
  ULONG returned = 0;
  LONG s = query_process(GetCurrentProcess(), 51, b.data(),
                         ULONG(b.size() * sizeof(uintptr_t)), &returned);
  if (s < 0 || b[0] > 8192 || returned > b.size() * sizeof(uintptr_t))
    fail("handle-snapshot");
  auto first = reinterpret_cast<Entry *>(b.data() + 2);
  return {first, first + b[0]};
}
static std::set<uintptr_t> handles(const std::vector<Entry> &e) {
  std::set<uintptr_t> s;
  for (auto x : e)
    s.insert(uintptr_t(x.h));
  return s;
}
static std::set<uintptr_t> token_handles(const std::vector<Entry> &e) {
  std::set<uintptr_t> s;
  for (auto x : e)
    if (x.type == token_type_index)
      s.insert(uintptr_t(x.h));
  return s;
}
static bool inputs_intact(const std::vector<Entry> &e) {
  auto s = handles(e);
  for (auto h : input_handles)
    if (!s.count(h))
      return false;
  return true;
}
static bool token_type(HANDLE h) {
  unsigned char b[2048]{};
  ULONG returned = 0;
  LONG s = query_object(h, 2, b, sizeof(b), &returned);
  if (s < 0)
    return false;
  auto u = reinterpret_cast<UNICODE_STRING *>(b);
  return u->Length == 10 && !memcmp(u->Buffer, L"Token", 10);
}
static DWORD WINAPI worker(void *) {
  if (WaitForSingleObject(signal_event, 0) != WAIT_TIMEOUT ||
      !SetEvent(ready_event))
    fail("worker-ready");
  if (WaitForSingleObject(signal_event, INFINITE) != WAIT_OBJECT_0)
    fail("worker-wait");
  return 77;
}
static DWORD WINAPI ended(void *) { return 66; }
static LONG invoke(HANDLE h, ULONG desired, BOOLEAN self, HANDLE *out,
                   ULONG *exception) {
  __try {
    return open_token(h, desired, self, out);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
    return LONG(*exception);
  }
}
static void protect(unsigned page, DWORD flags) {
  DWORD old = 0;
  if (!VirtualProtect(memory + page * 4096, 4096, flags, &old))
    fail("protect");
}
static void dump(unsigned offset, unsigned n) {
  for (unsigned i = 0; i < n; i++)
    printf("%02X", unsigned(memory[offset + i]));
}
static void one(unsigned ordinal, unsigned family, unsigned state,
                unsigned thread, ULONG desired, unsigned self, unsigned mode) {
  protect(0, PAGE_READWRITE);
  protect(1, PAGE_READWRITE);
  memset(memory, 0xA5, 8192);
  uintptr_t out = uintptr_t(memory + 128);
  switch (mode) {
  case 1:
    out = 0;
    break;
  case 2:
    out = 1;
    break;
  case 3:
    out = uintptr_t(memory + 129);
    break;
  case 4:
    out = uintptr_t(memory + 132);
    break;
  case 5:
    out = uintptr_t(memory + 136);
    break;
  case 6:
  case 7:
  case 8:
  case 9:
    out = uintptr_t(memory + 4096);
    break;
  case 10:
  case 11:
  case 12:
  case 13:
    out = uintptr_t(memory + 4092);
    break;
  case 14:
    out = uintptr_t(memory + 4096 - sizeof(HANDLE));
    break;
  case 15:
    out = uintptr_t(-1) - 7;
    break;
  case 16:
    out = uintptr_t(memory + 4094);
    break;
  case 17:
    out = uintptr_t(memory + 4095);
    break;
  default:
    break;
  }
  DWORD flags = PAGE_READWRITE;
  if (mode == 6 || mode == 12)
    flags = PAGE_READONLY;
  if (mode == 7 || mode == 10)
    flags = PAGE_NOACCESS;
  if (mode == 8 || mode == 11)
    flags = PAGE_READWRITE | PAGE_GUARD;
  if (mode == 9 || mode == 13)
    flags = PAGE_READONLY | PAGE_GUARD;
  protect(1, flags);
  auto before_entries = entries();
  auto before = handles(before_entries);
  auto tokens_before = token_handles(before_entries);
  if (!inputs_intact(before_entries))
    fail("input-handle-before");
  ULONG exception = 0;
  LONG status = invoke(threads[thread], desired, BOOLEAN(self),
                       reinterpret_cast<HANDLE *>(out), &exception);
  MEMORY_BASIC_INFORMATION q0{}, q1{};
  if (!VirtualQuery(memory, &q0, sizeof(q0)) ||
      !VirtualQuery(memory + 4096, &q1, sizeof(q1)))
    fail("query-protection");
  auto after_entries = entries();
  auto after = handles(after_entries);
  unsigned births = 0;
  uintptr_t published = 0;
  protect(0, PAGE_READWRITE);
  protect(1, PAGE_READWRITE);
  if (out >= uintptr_t(memory) &&
      out <= uintptr_t(memory + 8192) - sizeof(HANDLE))
    memcpy(&published, reinterpret_cast<void *>(out), sizeof(HANDLE));
  printf("case=%u family=%u state=%u thread=%u access=%08lX as-self=%u "
         "buffer=%u width=%u input-thread=%llX output=%llX status=%08lX "
         "exception=%08lX result=%llX protection=%lX,%lX, births=",
         ordinal, family, state, thread, desired, self, mode,
         unsigned(sizeof(HANDLE)),
         (unsigned long long)uintptr_t(threads[thread]),
         (unsigned long long)out, ULONG(status), exception,
         (unsigned long long)published, q0.Protect, q1.Protect);
  for (auto e : after_entries)
    if (e.type == token_type_index && !tokens_before.count(uintptr_t(e.h)))
      births++;
  printf("%u new-handles=", births);
  for (auto e : after_entries)
    if (e.type == token_type_index && !tokens_before.count(uintptr_t(e.h))) {
      if (!token_type(e.h))
        fail("new-handle-not-token");
      printf("%llX:Token:%08lX,", (unsigned long long)uintptr_t(e.h), e.access);
    }
  printf(" bytes0=");
  dump(112, 64);
  printf(" bytes1=");
  dump(4064, 64);
  for (auto e : after_entries)
    if (e.type == token_type_index && !tokens_before.count(uintptr_t(e.h)))
      if (!CloseHandle(e.h))
        fail("close-owned-token");
  auto restored_entries = entries();
  auto restored = handles(restored_entries);
  bool exact = token_handles(restored_entries) == tokens_before &&
               inputs_intact(restored_entries);
  printf(" handles-before=%zu handles-after=%zu global-exact-handle-set=%u "
         "exact-token-and-input-handles=%u\n",
         before.size(), restored.size(), unsigned(restored == before),
         unsigned(exact));
  if (!exact)
    fail("handle-restoration");
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  puts("native-thread-token-open-probe");
  auto nt = GetModuleHandleW(L"ntdll.dll");
  open_token = Open(GetProcAddress(nt, "NtOpenThreadToken"));
  query_process = Query(GetProcAddress(nt, "NtQueryInformationProcess"));
  query_object = Query(GetProcAddress(nt, "NtQueryObject"));
  if (!open_token || !query_process || !query_object)
    fail("exports");
  memory = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 8192, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!memory)
    fail("allocate");
  threads[0] = GetCurrentThread();
  threads[1] = nullptr;
  threads[2] = GetCurrentProcess();
  threads[3] = reinterpret_cast<HANDLE>(uintptr_t(0x1234));
  ULONG grants[] = {0, 0x40, 0x800, 0x840, 0x1FFFFF, 0x20, 0x100000};
  for (unsigned i = 0; i < 7; i++)
    if (!DuplicateHandle(GetCurrentProcess(), GetCurrentThread(),
                         GetCurrentProcess(), &threads[4 + i], grants[i], FALSE,
                         0))
      fail("duplicate-current");
  threads[11] = CreateEventW(nullptr, TRUE, FALSE, nullptr);
  if (!threads[11])
    fail("event");
  if (!DuplicateHandle(GetCurrentProcess(), GetCurrentProcess(),
                       GetCurrentProcess(), &threads[12], 0x200, FALSE, 0))
    fail("duplicate-process");
  threads[13] = reinterpret_cast<HANDLE>(uintptr_t(threads[8]) | 3);
  signal_event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
  if (!signal_event)
    fail("worker-event");
  ready_event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
  if (!ready_event)
    fail("ready-event");
  threads[14] = CreateThread(nullptr, 0, worker, nullptr, 0, nullptr);
  if (!threads[14])
    fail("worker-create");
  ULONG wg[] = {0x40, 0x800, 0};
  for (unsigned i = 0; i < 3; i++)
    if (!DuplicateHandle(GetCurrentProcess(), threads[14], GetCurrentProcess(),
                         &threads[15 + i], wg[i], FALSE, 0))
      fail("duplicate-worker");
  threads[18] = CreateThread(nullptr, 0, ended, nullptr, 0, nullptr);
  if (!threads[18] || WaitForSingleObject(threads[18], 30000) != WAIT_OBJECT_0)
    fail("ended-thread");
  if (WaitForSingleObject(ready_event, 30000) != WAIT_OBJECT_0)
    fail("wait-worker-ready");
  DWORD ended_exit = 0;
  if (!GetExitCodeThread(threads[18], &ended_exit) || ended_exit != 66)
    fail("ended-exit");
  HANDLE primary = nullptr;
  if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &primary) ||
      !token_type(primary))
    fail("primary-type-oracle");
  TOKEN_TYPE primary_kind{};
  DWORD primary_returned = 0;
  if (!GetTokenInformation(primary, TokenType, &primary_kind,
                           sizeof(primary_kind), &primary_returned) ||
      primary_kind != TokenPrimary)
    fail("primary-type");
  for (auto e : entries())
    if (e.h == primary)
      token_type_index = e.type;
  if (!token_type_index || !CloseHandle(primary))
    fail("primary-oracle-close");
  for (unsigned i = 4; i < 19; ++i)
    if (i != 13)
      input_handles.insert(uintptr_t(threads[i]));
  input_handles.insert(uintptr_t(signal_event));
  input_handles.insert(uintptr_t(ready_event));
  for (unsigned i = 19; i < 24; ++i)
    threads[i] = reinterpret_cast<HANDLE>(uintptr_t(16 - i));
  auto initial_entries = entries();
  auto initial = token_handles(initial_entries);
  if (!inputs_intact(initial_entries))
    fail("initial-input-set");
  printf("token-type-index=%lu owned-input-handle-count=%zu primary-type=%u\n",
         token_type_index, input_handles.size(), unsigned(primary_kind));
  unsigned ordinal = 0;
  ULONG desired[] = {0,          1,          8,          0x20008,
                     0x2001C,    0xF01FF,    0xFFFFFFFF, 0x80000000,
                     0x40000000, 0x02000000, 0x01000000, 0x1000};
  for (unsigned state = 0; state < 5; state++) {
    if (!RevertToSelf())
      fail("revert");
    if (state && !ImpersonateSelf(SECURITY_IMPERSONATION_LEVEL(state - 1)))
      fail("impersonate");
    HANDLE oracle = nullptr;
    ULONG ex = 0;
    LONG os = invoke(GetCurrentThread(), TOKEN_QUERY, TRUE, &oracle, &ex);
    printf("state=%u oracle-status=%08lX oracle-exception=%08lX", state,
           ULONG(os), ex);
    if (os >= 0) {
      TOKEN_TYPE ty{};
      SECURITY_IMPERSONATION_LEVEL level{};
      DWORD ret = 0;
      if (!GetTokenInformation(oracle, TokenType, &ty, sizeof(ty), &ret) ||
          !GetTokenInformation(oracle, TokenImpersonationLevel, &level,
                               sizeof(level), &ret))
        fail("oracle-token-information");
      printf(" token-type=%u impersonation-level=%u", unsigned(ty),
             unsigned(level));
      if (!CloseHandle(oracle))
        fail("close-oracle");
    }
    puts("");
    for (unsigned ti = 0; ti < 24; ti++)
      for (ULONG d : desired)
        for (unsigned self : {0, 1, 2, 255})
          one(ordinal++, 0, state, ti, d, self, 0);
    for (ULONG d : {ULONG(0), ULONG(0x2001C), ULONG(0xFFFFFFFF)})
      for (unsigned self : {0, 1})
        for (unsigned mode = 0; mode < 18; mode++)
          one(ordinal++, 1, state, 0, d, self, mode);
    if (state == 0)
      for (unsigned ti : {1, 2, 3, 4, 5, 6, 11, 18})
        for (unsigned self : {0, 1})
          for (unsigned mode = 0; mode < 18; mode++)
            one(ordinal++, 2, state, ti, 0x2001C, self, mode);
  }
  if (!RevertToSelf())
    fail("final-revert");
  if (token_handles(entries()) != initial || !inputs_intact(entries()))
    fail("pre-join-owned-set");
  if (!SetEvent(signal_event) ||
      WaitForSingleObject(threads[14], 30000) != WAIT_OBJECT_0)
    fail("worker-join");
  DWORD exit = 0;
  if (!GetExitCodeThread(threads[14], &exit) || exit != 77)
    fail("worker-exit");
  auto final_entries = entries();
  if (token_handles(final_entries) != initial || !inputs_intact(final_entries))
    fail("final-token-and-input-set");
  for (unsigned i = 4; i < 19; i++)
    if (i != 13 && !CloseHandle(threads[i]))
      fail("close-input-handle");
  if (!CloseHandle(signal_event) || !CloseHandle(ready_event) ||
      !VirtualFree(memory, 0, MEM_RELEASE))
    fail("final-cleanup");
  printf("complete cases=%u exact-token-and-input-handles=1 worker-exit=77 "
         "ended-exit=66 "
         "owned-input-handles-closed=14 final-impersonation=0\n",
         ordinal);
  return 0;
}
