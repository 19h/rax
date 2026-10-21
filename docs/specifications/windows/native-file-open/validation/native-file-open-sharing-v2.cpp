#define WIN32_LEAN_AND_MEAN
#include <cstdio>
#include <cstring>
#include <string>
#include <windows.h>
#include <winternl.h>
using Open = NTSTATUS(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES,
                               PIO_STATUS_BLOCK, ULONG, ULONG);
int wmain() {
  const auto fn = reinterpret_cast<Open>(
      GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "NtOpenFile"));
  if (!fn)
    return 10;
  wchar_t temporary[MAX_PATH]{};
  if (!GetTempPathW(MAX_PATH, temporary))
    return 11;
  const auto directory = std::wstring(temporary) + L"assist-open-sharing-v2-" +
                         std::to_wstring(GetCurrentProcessId()) + L"-" +
                         std::to_wstring(GetTickCount64());
  if (!CreateDirectoryW(directory.c_str(), nullptr))
    return 12;
  DWORD before = 0;
  GetProcessHandleCount(GetCurrentProcess(), &before);
  for (unsigned role = 0; role < 4; ++role) {
    const auto path =
        directory + L"\\fixture-" + std::to_wstring(role) + L".dat";
    HANDLE made = CreateFileW(path.c_str(), GENERIC_WRITE, 7, nullptr,
                              CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (made == INVALID_HANDLE_VALUE)
      return 13;
    DWORD written = 0;
    const bool populated =
        WriteFile(made, "retained", 8, &written, nullptr) && written == 8;
    CloseHandle(made);
    if (!populated)
      return 14;
    HANDLE held =
        CreateFileW(path.c_str(),
                    (role == 0   ? 0
                     : role == 1 ? DELETE
                     : role == 2 ? GENERIC_READ | DELETE
                                 : GENERIC_WRITE | DELETE),
                    7, nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (held == INVALID_HANDLE_VALUE)
      return 15;
    const auto native = L"\\??\\" + path;
    UNICODE_STRING name{};
    name.Length = static_cast<USHORT>(native.size() * 2);
    name.MaximumLength = name.Length;
    name.Buffer = const_cast<PWSTR>(native.data());
    OBJECT_ATTRIBUTES attrs{};
    attrs.Length = sizeof(attrs);
    attrs.ObjectName = &name;
    attrs.Attributes = 0x840;
    HANDLE candidate = nullptr;
    IO_STATUS_BLOCK io{};
    const auto code =
        fn(&candidate, DELETE | SYNCHRONIZE, &attrs, &io, 3, 0x1060);
    const bool expected =
        static_cast<ULONG>(code) == (role == 0 ? 0 : 0xc0000043u);
    const bool published =
        candidate != nullptr && candidate != INVALID_HANDLE_VALUE;
    if (published)
      CloseHandle(candidate);
    CloseHandle(held);
    SetLastError(0);
    const DWORD attributes = GetFileAttributesW(path.c_str());
    const DWORD error = GetLastError();
    const bool exists = attributes != INVALID_FILE_ATTRIBUTES;
    std::printf("case role=%u held-access=%08lX status=%08lX published=%u "
                "exists-after-close=%u query-error=%lu\n",
                role,
                (role == 0   ? 0ul
                 : role == 1 ? static_cast<ULONG>(DELETE)
                 : role == 2 ? static_cast<ULONG>(GENERIC_READ | DELETE)
                             : static_cast<ULONG>(GENERIC_WRITE | DELETE)),
                static_cast<ULONG>(code), published, exists, error);
    if (!expected || exists != (role != 0))
      return 16;
    if (exists && !DeleteFileW(path.c_str()))
      return 17;
  }
  DWORD after = 0;
  GetProcessHandleCount(GetCurrentProcess(), &after);
  if (!RemoveDirectoryW(directory.c_str()))
    return 18;
  std::printf("end handle-count-before=%lu handle-count-after=%lu\n", before,
              after);
  return before == after ? 0 : 19;
}
