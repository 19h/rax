#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstddef>
#include <cstdio>
#include <cwchar>
#include <vector>
#include <windows.h>

// phnt ntpsapi.h PROC_THREAD_BNOISOLATION_ATTRIBUTE, attribute number 19.
struct BnoAttribute {
  BOOL enabled;
  WCHAR prefix[0x88];
};
static_assert(sizeof(BnoAttribute) == 276);
int wmain(int argc, wchar_t **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 5 || wcschr(argv[1], L'"') || wcschr(argv[2], L'"'))
    return 21;
  BnoAttribute bno{};
  bno.enabled = wcscmp(argv[3], L"enabled") == 0;
  if (wcscmp(argv[4], L"ascii") == 0) {
    swprintf_s(bno.prefix, L"AssistBnoProof_%lu", GetCurrentProcessId());
  } else if (wcscmp(argv[4], L"unicode") == 0) {
    swprintf_s(bno.prefix, L"AssistBno_\x03a9_\x4e2d_%lu",
               GetCurrentProcessId());
  } else if (wcscmp(argv[4], L"long") == 0) {
    swprintf_s(bno.prefix, L"AssistBnoLong_%lu_", GetCurrentProcessId());
    for (size_t i = wcslen(bno.prefix); i < 135; ++i)
      bno.prefix[i] = WCHAR(L'A' + i % 26);
    bno.prefix[135] = 0;
  } else if (wcscmp(argv[4], L"empty") != 0) {
    return 22;
  }
  SIZE_T bytes = 0;
  InitializeProcThreadAttributeList(nullptr, 1, 0, &bytes);
  if (!bytes || GetLastError() != ERROR_INSUFFICIENT_BUFFER)
    return 23;
  std::vector<BYTE> buffer(bytes);
  auto list = reinterpret_cast<LPPROC_THREAD_ATTRIBUTE_LIST>(buffer.data());
  if (!InitializeProcThreadAttributeList(list, 1, 0, &bytes))
    return 24;
  const DWORD_PTR attribute = ProcThreadAttributeValue(19, FALSE, TRUE, FALSE);
  if (!UpdateProcThreadAttribute(list, 0, attribute, &bno, sizeof bno, nullptr,
                                 nullptr)) {
    printf("attribute-failed error=%lu attribute=%llX bytes=%zu\n",
           GetLastError(), static_cast<unsigned long long>(attribute),
           sizeof bno);
    DeleteProcThreadAttributeList(list);
    return 25;
  }
  STARTUPINFOEXW startup{};
  startup.StartupInfo.cb = sizeof startup;
  startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
  startup.StartupInfo.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
  startup.StartupInfo.hStdOutput = GetStdHandle(STD_OUTPUT_HANDLE);
  startup.StartupInfo.hStdError = GetStdHandle(STD_ERROR_HANDLE);
  startup.lpAttributeList = list;
  wchar_t command[32768];
  swprintf_s(command, L"\"%s\" %s 44", argv[1], argv[2]);
  PROCESS_INFORMATION process{};
  printf("child-control enabled=%u prefix-units=%zu prefix-utf16=",
         unsigned(bno.enabled), wcslen(bno.prefix));
  for (const auto *unit = bno.prefix; *unit; ++unit)
    printf("%04X", unsigned(*unit));
  puts("");
  const BOOL created = CreateProcessW(argv[1], command, nullptr, nullptr, TRUE,
                                      EXTENDED_STARTUPINFO_PRESENT, nullptr,
                                      nullptr, &startup.StartupInfo, &process);
  DWORD error = created ? ERROR_SUCCESS : GetLastError();
  DeleteProcThreadAttributeList(list);
  printf("child-create ok=%u error=%lu\n", unsigned(created), error);
  if (!created)
    return 26;
  DWORD wait = WaitForSingleObject(process.hProcess, 120000);
  DWORD code = 0;
  if (wait != WAIT_OBJECT_0) {
    // The only process terminated is this fixture's directly owned child.
    TerminateProcess(process.hProcess, 27);
    WaitForSingleObject(process.hProcess, 10000);
  }
  const BOOL queried = GetExitCodeProcess(process.hProcess, &code);
  const BOOL thread_closed = CloseHandle(process.hThread);
  const BOOL process_closed = CloseHandle(process.hProcess);
  printf("child-complete wait=%08lX queried=%u exit=%lu closed=%u\n", wait,
         unsigned(queried), code, unsigned(thread_closed && process_closed));
  return wait == WAIT_OBJECT_0 && queried && code == 0 && thread_closed &&
                 process_closed
             ? 0
             : 28;
}
