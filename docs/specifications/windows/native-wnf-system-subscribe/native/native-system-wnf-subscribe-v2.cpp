#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
extern "C" __declspec(dllimport) BOOL WINAPI
ConvertStringSecurityDescriptorToSecurityDescriptorW(LPCWSTR, DWORD,
                                                     PSECURITY_DESCRIPTOR *,
                                                     PULONG);
#include <windows.h>
#include <winternl.h>

using Create = LONG(NTAPI *)(uint64_t *, ULONG, ULONG, BOOLEAN, const GUID *,
                             ULONG, PSECURITY_DESCRIPTOR);
using Delete = LONG(NTAPI *)(const uint64_t *);
using Subscribe = LONG(NTAPI *)(const uint64_t *, ULONG, ULONG, uint64_t *);
using Version = LONG(NTAPI *)(PRTL_OSVERSIONINFOW);

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 2;
  HMODULE module = GetModuleHandleW(L"ntdll.dll");
  auto create =
      reinterpret_cast<Create>(GetProcAddress(module, "NtCreateWnfStateName"));
  auto remove =
      reinterpret_cast<Delete>(GetProcAddress(module, "NtDeleteWnfStateName"));
  auto sub = reinterpret_cast<Subscribe>(
      GetProcAddress(module, "NtSubscribeWnfStateChange"));
  auto unsub = reinterpret_cast<Delete>(
      GetProcAddress(module, "NtUnsubscribeWnfStateChange"));
  auto version =
      reinterpret_cast<Version>(GetProcAddress(module, "RtlGetVersion"));
  if (!create || !remove || !sub || !unsub || !version)
    return 3;
  RTL_OSVERSIONINFOW os{};
  os.dwOSVersionInfoSize = sizeof os;
  if (version(&os) < 0)
    return 4;
  printf("identity profile=%s width=%zu build=%lu.%lu.%lu\n", argv[1],
         sizeof(void *), os.dwMajorVersion, os.dwMinorVersion,
         os.dwBuildNumber);
  const wchar_t *descriptors[] = {
      L"O:SYG:SYD:(A;;0x10000;;;WD)",
      L"O:SYG:SYD:(A;;0x10001;;;WD)",
      L"O:SYG:SYD:(A;;0x10002;;;WD)",
      L"O:SYG:SYD:(A;;0x10003;;;WD)",
      L"O:SYG:SYD:(A;;GR;;;WD)(A;;0x10000;;;WD)",
      L"O:SYG:SYD:(A;;GW;;;WD)(A;;0x10000;;;WD)",
      L"O:SYG:SYD:(A;;GA;;;WD)",
      L"O:SYG:SYD:(A;;GR;;;WD)(A;;0x10000;;;WD)S:(ML;;NW;;;S-1-16-0)"};
  const ULONG stamps[] = {0, 1, 0xFFFFFFFF};
  unsigned cases = 0, created = 0, deleted = 0, freed = 0, subscriptions = 0,
           ended = 0;
  bool failed = false;
  for (unsigned index = 0; index < 8; ++index) {
    PSECURITY_DESCRIPTOR sd{};
    ULONG length{};
    if (!ConvertStringSecurityDescriptorToSecurityDescriptorW(
            descriptors[index], 1, &sd, &length)) {
      printf("descriptor-failure index=%u error=%lu\n", index, GetLastError());
      failed = true;
      continue;
    }
    printf("descriptor index=%u bytes=", index);
    for (ULONG i = 0; i < length; ++i)
      printf("%02X", static_cast<const BYTE *>(sd)[i]);
    puts("");
    uint64_t name{};
    LONG status = create(&name, 3, 0, FALSE, nullptr, 64, sd);
    printf("create index=%u status=%08lX name=%016llX\n", index, ULONG(status),
           static_cast<unsigned long long>(name));
    if (status >= 0) {
      ++created;
      for (ULONG stamp : stamps)
        for (ULONG mask = 0; mask < 32; ++mask) {
          LONG before = unsub(&name);
          uint64_t id = 0xA5A5A5A5A5A5A5A5ULL;
          SetLastError(0x7E11ABCD);
          LONG actual = sub(&name, stamp, mask, &id);
          DWORD error = GetLastError();
          LONG after = unsub(&name);
          if (actual >= 0) {
            ++subscriptions;
            if (after >= 0)
              ++ended;
            else
              failed = true;
          }
          printf("case ordinal=%u descriptor=%u stamp=%08lX mask=%08lX "
                 "before=%08lX status=%08lX id=%016llX id-nonzero=%u "
                 "after=%08lX last-error=%08lX\n",
                 cases++, index, stamp, mask, ULONG(before), ULONG(actual),
                 static_cast<unsigned long long>(id), id != 0, ULONG(after),
                 error);
        }
      LONG result = remove(&name);
      printf("delete index=%u status=%08lX\n", index, ULONG(result));
      if (result >= 0)
        ++deleted;
      else
        failed = true;
    } else
      failed = true;
    if (LocalFree(sd) == nullptr)
      ++freed;
    else
      failed = true;
  }
  printf("complete cases=%u created=%u deleted=%u freed=%u subscriptions=%u "
         "unsubscribed=%u failed=%u\n",
         cases, created, deleted, freed, subscriptions, ended, failed);
  return failed ? 5 : 0;
}
