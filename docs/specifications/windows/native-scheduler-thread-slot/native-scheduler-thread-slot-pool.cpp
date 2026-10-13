#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <set>
#include <vector>
#include <windows.h>
#include <winternl.h>
using Set = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG);
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
static Set set_thread, set_process;
static Query query_thread, query_process;
struct Info {
  ULONG action;
  HANDLE owner;
  void *slot;
};
struct HandleEntry {
  HANDLE h;
  uintptr_t handles, pointers;
  ULONG access, type, attributes, reserved;
};
static void fail(const char *s) {
  printf("failure=%s error=%lu\n", s, GetLastError());
  ExitProcess(21);
}
static std::set<uintptr_t> handles() {
  std::vector<uintptr_t> b(131072);
  ULONG r = 0;
  LONG s = query_process(GetCurrentProcess(), 51, b.data(),
                         ULONG(b.size() * sizeof(uintptr_t)), &r);
  if (s < 0 || b[0] > 8192 || r > b.size() * sizeof(uintptr_t))
    fail("snapshot");
  auto e = reinterpret_cast<HandleEntry *>(b.data() + 2);
  std::set<uintptr_t> v;
  for (size_t i = 0; i < b[0]; ++i)
    v.insert(uintptr_t(e[i].h));
  return v;
}
static uintptr_t slot(HANDLE thread = GetCurrentThread()) {
  uintptr_t p = 0;
  ULONG r = 0;
  LONG s = query_thread(thread, 57, &p, sizeof(p), &r);
  if (s < 0 || r != sizeof(p)) {
    printf("query thread=%llX status=%08lX returned=%lu slot=%llX\n",
           (unsigned long long)thread, s, r, (unsigned long long)p);
    if (thread == GetCurrentThread())
      fail("query-current-slot");
  }
  return p;
}
static void mapping(const char *label, uintptr_t p) {
  MEMORY_BASIC_INFORMATION q{};
  SIZE_T n = VirtualQuery(reinterpret_cast<void *>(p), &q, sizeof(q));
  printf(
      "map label=%s slot=%llX result=%zu base=%llX allocation=%llX size=%llX "
      "state=%08lX type=%08lX protect=%08lX allocation-protect=%08lX bytes=",
      label, (unsigned long long)p, n, (unsigned long long)q.BaseAddress,
      (unsigned long long)q.AllocationBase, (unsigned long long)q.RegionSize,
      q.State, q.Type, q.Protect, q.AllocationProtect);
  if (n && q.State == MEM_COMMIT && q.Protect == PAGE_READWRITE) {
    auto b = reinterpret_cast<unsigned char *>(p);
    for (unsigned i = 0; i < 64; ++i)
      printf("%02X", unsigned(b[i]));
  } else
    printf("unreadable");
  printf("\n");
}
static LONG change(const char *label, HANDLE thread, ULONG action, HANDLE owner,
                   uintptr_t input, uintptr_t *output = nullptr) {
  Info i{};
  memset(&i, 0, sizeof(i));
  i.action = action;
  i.owner = owner;
  i.slot = reinterpret_cast<void *>(input);
  uintptr_t before = slot(thread);
  LONG s = set_thread(thread, 57, &i, sizeof(i));
  uintptr_t after = slot(thread);
  printf("call label=%s thread=%llX action=%lu owner=%llX input-slot=%llX "
         "output-slot=%llX before=%llX after=%llX status=%08lX\n",
         label, (unsigned long long)thread, action, (unsigned long long)owner,
         (unsigned long long)input, (unsigned long long)i.slot,
         (unsigned long long)before, (unsigned long long)after, s);
  if (output)
    *output = uintptr_t(i.slot);
  return s;
}
static HANDLE make_owner() {
  HANDLE h = nullptr;
  LONG s = set_process(GetCurrentProcess(), 112, &h, sizeof(h));
  if (s < 0 || !h)
    fail("owner");
  return h;
}
static void release(HANDLE t) {
  if ((change("cleanup", t, 1, nullptr, ~uintptr_t(0)) < 0 || slot(t)) &&
      t == GetCurrentThread())
    fail("release");
}
static void close_owner(const char *label, HANDLE h) {
  if (!CloseHandle(h))
    fail("close");
  printf("close label=%s handle=%llX ok=1\n", label, (unsigned long long)h);
}
struct Worker {
  HANDLE ready, stop, owner;
  uintptr_t binding;
};
static DWORD WINAPI worker(void *p) {
  auto w = static_cast<Worker *>(p);
  release(GetCurrentThread());
  if (change("worker-self-assign", GetCurrentThread(), 0, w->owner, 0,
             &w->binding) < 0 ||
      !w->binding)
    fail("worker-assign");
  SetEvent(w->ready);
  WaitForSingleObject(w->stop, INFINITE);
  return 77;
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto n = GetModuleHandleW(L"ntdll.dll");
  set_thread =
      reinterpret_cast<Set>(GetProcAddress(n, "NtSetInformationThread"));
  set_process =
      reinterpret_cast<Set>(GetProcAddress(n, "NtSetInformationProcess"));
  query_thread =
      reinterpret_cast<Query>(GetProcAddress(n, "NtQueryInformationThread"));
  query_process =
      reinterpret_cast<Query>(GetProcAddress(n, "NtQueryInformationProcess"));
  if (!set_thread || !set_process || !query_thread || !query_process)
    return 22;
  printf("host width=%zu info=%zu\n", sizeof(void *), sizeof(Info));
  release(GetCurrentThread());
  for (unsigned round = 0; round < 3; ++round) {
    printf("round=%u warmup=%u\n", round, unsigned(round == 0));
    auto initial = handles();
    HANDLE a = make_owner();
    uintptr_t p = 0;
    if (change("free-before-close-assign", GetCurrentThread(), 0, a, 1, &p) <
            0 ||
        !p)
      fail("assign-a");
    mapping("free-before-close-active", p);
    release(GetCurrentThread());
    mapping("free-before-close-released", p);
    close_owner("free-before-close", a);
    mapping("free-before-close-final", p);
    HANDLE b = make_owner();
    uintptr_t q = 0;
    if (change("active-close-assign", GetCurrentThread(), 0, b, 0, &q) < 0 ||
        !q)
      fail("assign-b");
    close_owner("active-close", b);
    mapping("active-close-retained", q);
    release(GetCurrentThread());
    mapping("active-close-final", q);
    HANDLE c = make_owner(), alias = nullptr;
    if (!DuplicateHandle(GetCurrentProcess(), c, GetCurrentProcess(), &alias, 0,
                         FALSE, 0))
      fail("duplicate-zero");
    LONG zero = change("zero-grant", GetCurrentThread(), 0, alias, 0, &q);
    if (zero >= 0)
      release(GetCurrentThread());
    close_owner("zero-grant-alias", alias);
    if (change("tagged-owner", GetCurrentThread(), 0,
               reinterpret_cast<HANDLE>(uintptr_t(c) | 3), ~uintptr_t(0),
               &q) >= 0)
      release(GetCurrentThread());
    Worker w[66]{};
    HANDLE t[66]{};
    for (unsigned i = 0; i < 66; ++i) {
      w[i].owner = c;
      w[i].ready = CreateEventW(nullptr, FALSE, FALSE, nullptr);
      w[i].stop = CreateEventW(nullptr, TRUE, FALSE, nullptr);
      t[i] = CreateThread(nullptr, 0, worker, &w[i], 0, nullptr);
      if (!t[i] || WaitForSingleObject(w[i].ready, INFINITE) != WAIT_OBJECT_0)
        fail("worker");
      printf("worker-ready index=%u binding=%llX\n", i,
             (unsigned long long)w[i].binding);
    }
    uintptr_t slots[3]{};
    change("multi-main", GetCurrentThread(), 0, c, 0, &slots[0]);
    uintptr_t ignored = 0;
    change("multi-thread0", t[0], 0, c, 0, &ignored);
    change("multi-thread1", t[1], 0, c, 0, &ignored);
    slots[1] = w[0].binding;
    slots[2] = w[1].binding;
    for (unsigned i = 0; i < 3; ++i)
      if (slots[i])
        mapping(i == 0   ? "multi-main"
                : i == 1 ? "multi-thread0"
                         : "multi-thread1",
                slots[i]);
    if (slots[0]) {
      reinterpret_cast<unsigned char *>(slots[0])[0] = 0x5a;
      release(GetCurrentThread());
      mapping("multi-released-bytes", slots[0]);
      uintptr_t repeat = 0;
      change("multi-reassign", GetCurrentThread(), 0, c, 0, &repeat);
      if (repeat)
        mapping("multi-reassigned-bytes", repeat);
    }
    for (unsigned index = 0; index < 66; ++index)
      mapping("pool-worker", w[index].binding);
    close_owner("multi-final-handle", c);
    if (slots[0])
      mapping("multi-after-close", slots[0]);
    release(GetCurrentThread());
    for (unsigned i = 0; i < 66; ++i) {
      SetEvent(w[i].stop);
      if (WaitForSingleObject(t[i], INFINITE) != WAIT_OBJECT_0)
        fail("join");
      DWORD exit = 0;
      if (!GetExitCodeThread(t[i], &exit) || exit != 77)
        fail("worker-exit");
      printf("worker-exit index=%u code=%lu\n", i, exit);
      if (slots[0])
        mapping(i == 65 ? "pool-no-binding" : "pool-partial-exit", slots[0]);
      close_owner("worker", t[i]);
      close_owner("ready", w[i].ready);
      close_owner("stop", w[i].stop);
    }
    auto final = handles();
    printf("round-complete=%u exact-handle-set=%u before=%zu after=%zu "
           "final-slot=%llX\n",
           round, unsigned(final == initial), initial.size(), final.size(),
           (unsigned long long)slot());
    if (round != 0 && final != initial)
      fail("round-lifetime");
  }
  printf("complete rounds=3 warmup-excluded=1\n");
  return 0;
}
