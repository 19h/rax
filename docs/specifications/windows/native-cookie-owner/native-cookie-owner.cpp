#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>
#include <windows.h>
#include <winternl.h>

using QueryProcess = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);

[[noreturn]] static void fail(const char *operation) {
  printf("failure operation=%s error=%lu\n", operation, GetLastError());
  ExitProcess(21);
}

static void read_exact(HANDLE process, uintptr_t address, void *output,
                       SIZE_T size) {
  SIZE_T got = 0;
  if (!ReadProcessMemory(process, reinterpret_cast<void *>(address), output,
                         size, &got) ||
      got != size)
    fail("read-exact");
}

static uintptr_t cookie_address(HANDLE process, uintptr_t base,
                                WORD *subsystem_major) {
  IMAGE_DOS_HEADER dos{};
  read_exact(process, base, &dos, sizeof dos);
  if (dos.e_magic != IMAGE_DOS_SIGNATURE || dos.e_lfanew <= 0 ||
      dos.e_lfanew > 0x100000)
    fail("DOS-header");
  IMAGE_NT_HEADERS nt{};
  read_exact(process, base + dos.e_lfanew, &nt, sizeof nt);
  if (nt.Signature != IMAGE_NT_SIGNATURE ||
      nt.OptionalHeader.Magic != IMAGE_NT_OPTIONAL_HDR_MAGIC)
    fail("NT-header");
  *subsystem_major = nt.OptionalHeader.MajorSubsystemVersion;
  auto directory =
      nt.OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_LOAD_CONFIG];
  constexpr SIZE_T prefix =
      offsetof(IMAGE_LOAD_CONFIG_DIRECTORY, SecurityCookie) + sizeof(uintptr_t);
  if (directory.Size < prefix || directory.VirtualAddress == 0)
    fail("load-config-directory");
  IMAGE_LOAD_CONFIG_DIRECTORY config{};
  read_exact(process, base + directory.VirtualAddress, &config, prefix);
  uintptr_t cookie = static_cast<uintptr_t>(config.SecurityCookie);
  if (config.Size < prefix || cookie < base ||
      cookie - base > nt.OptionalHeader.SizeOfImage - sizeof(uintptr_t))
    fail("load-config-cookie");
  return cookie;
}

static std::vector<BYTE> file_bytes(const wchar_t *path) {
  HANDLE file = CreateFileW(path, GENERIC_READ, FILE_SHARE_READ, nullptr,
                            OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
  if (file == INVALID_HANDLE_VALUE)
    fail("open-image");
  LARGE_INTEGER size{};
  if (!GetFileSizeEx(file, &size) || size.QuadPart <= 0 ||
      size.QuadPart > 16 * 1024 * 1024)
    fail("image-size");
  std::vector<BYTE> data(static_cast<size_t>(size.QuadPart));
  DWORD got = 0;
  if (!ReadFile(file, data.data(), static_cast<DWORD>(data.size()), &got,
                nullptr) ||
      got != data.size() || !CloseHandle(file))
    fail("read-image");
  return data;
}

template <typename T>
static T file_value(const std::vector<BYTE> &file, size_t offset) {
  if (offset > file.size() || sizeof(T) > file.size() - offset)
    fail("file-extent");
  T value{};
  memcpy(&value, file.data() + offset, sizeof value);
  return value;
}

static size_t raw_offset(const std::vector<BYTE> &file,
                         const IMAGE_NT_HEADERS &nt, size_t nt_offset,
                         uintptr_t rva) {
  if (rva < nt.OptionalHeader.SizeOfHeaders)
    return static_cast<size_t>(rva);
  size_t sections = nt_offset + offsetof(IMAGE_NT_HEADERS, OptionalHeader) +
                    nt.FileHeader.SizeOfOptionalHeader;
  for (unsigned i = 0; i < nt.FileHeader.NumberOfSections; ++i) {
    auto section = file_value<IMAGE_SECTION_HEADER>(
        file, sections + i * sizeof(IMAGE_SECTION_HEADER));
    if (rva >= section.VirtualAddress &&
        rva - section.VirtualAddress < section.SizeOfRawData)
      return section.PointerToRawData +
             static_cast<size_t>(rva - section.VirtualAddress);
  }
  fail("file-RVA");
}

static uintptr_t file_cookie(const std::vector<BYTE> &file) {
  auto dos = file_value<IMAGE_DOS_HEADER>(file, 0);
  auto nt = file_value<IMAGE_NT_HEADERS>(file, dos.e_lfanew);
  auto directory =
      nt.OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_LOAD_CONFIG];
  size_t config = raw_offset(file, nt, dos.e_lfanew, directory.VirtualAddress);
  auto va = file_value<uintptr_t>(
      file, config + offsetof(IMAGE_LOAD_CONFIG_DIRECTORY, SecurityCookie));
  if (va < nt.OptionalHeader.ImageBase)
    fail("file-cookie-base");
  return file_value<uintptr_t>(
      file,
      raw_offset(file, nt, dos.e_lfanew,
                 va - static_cast<uintptr_t>(nt.OptionalHeader.ImageBase)));
}

int main(int argc, char **argv) {
  SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX |
               SEM_NOOPENFILEERRORBOX);
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc == 3 && strcmp(argv[1], "--child") == 0) {
    WORD major = 0;
    auto base = reinterpret_cast<uintptr_t>(GetModuleHandleW(nullptr));
    uintptr_t cookie = 0;
    read_exact(GetCurrentProcess(),
               cookie_address(GetCurrentProcess(), base, &major), &cookie,
               sizeof cookie);
    HANDLE file = CreateFileA(argv[2], GENERIC_WRITE, 0, nullptr, CREATE_NEW,
                              FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE)
      fail("child-output-create");
    char text[128]{};
    int length = snprintf(text, sizeof text, "cookie=%016llX major=%u\n",
                          static_cast<unsigned long long>(cookie), major);
    DWORD wrote = 0;
    if (length <= 0 ||
        !WriteFile(file, text, static_cast<DWORD>(length), &wrote, nullptr) ||
        wrote != static_cast<DWORD>(length) || !CloseHandle(file))
      fail("child-output-write");
    return 0;
  }
  if (argc != 2)
    fail("profile-argument");
  wchar_t executable[32768]{};
  if (!GetModuleFileNameW(nullptr, executable, 32768))
    fail("self-path");
  auto file = file_bytes(executable);
  uintptr_t disk = file_cookie(file);
  auto query = reinterpret_cast<QueryProcess>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtQueryInformationProcess"));
  if (!query)
    fail("query-export");
  const char *names[] = {"untouched", "restore-default", "preseed", "zero"};
  uintptr_t values[] = {0, disk,
                        sizeof(uintptr_t) == 8 ? uintptr_t(0x123456789ABCULL)
                                               : uintptr_t(0x12345678),
                        0};
  for (unsigned ordinal = 0; ordinal < 4; ++ordinal) {
    char output[128]{};
    snprintf(output, sizeof output, "cookie-%s-%lu-%u.txt", argv[1],
             GetCurrentProcessId(), ordinal);
    if (GetFileAttributesA(output) != INVALID_FILE_ATTRIBUTES)
      fail("refuse-existing-output");
    std::wstring command = L"\"" + std::wstring(executable) + L"\" --child ";
    for (const char *p = output; *p; ++p)
      command.push_back(static_cast<wchar_t>(*p));
    STARTUPINFOW startup{};
    startup.cb = sizeof startup;
    PROCESS_INFORMATION process{};
    if (!CreateProcessW(executable, command.data(), nullptr, nullptr, FALSE,
                        CREATE_SUSPENDED | DETACHED_PROCESS, nullptr, nullptr,
                        &startup, &process))
      fail("create-owned-suspended-child");
    PROCESS_BASIC_INFORMATION basic{};
    ULONG returned = 0;
    if (query(process.hProcess, 0, &basic, sizeof basic, &returned) < 0)
      fail("query-child-PEB");
    uintptr_t base = 0;
    read_exact(process.hProcess,
               reinterpret_cast<uintptr_t>(basic.PebBaseAddress) +
                   2 * sizeof(uintptr_t),
               &base, sizeof base);
    WORD major = 0;
    uintptr_t address = cookie_address(process.hProcess, base, &major);
    uintptr_t before = 0;
    read_exact(process.hProcess, address, &before, sizeof before);
    if (ordinal != 0) {
      SIZE_T wrote = 0;
      if (!WriteProcessMemory(process.hProcess,
                              reinterpret_cast<void *>(address),
                              &values[ordinal], sizeof(uintptr_t), &wrote) ||
          wrote != sizeof(uintptr_t))
        fail("write-owned-child-cookie");
    }
    if (ResumeThread(process.hThread) == DWORD(-1))
      fail("resume-owned-child");
    DWORD wait = WaitForSingleObject(process.hProcess, 30000);
    if (wait == WAIT_TIMEOUT) {
      if (!TerminateProcess(process.hProcess, 0xE00000FF) ||
          WaitForSingleObject(process.hProcess, 30000) != WAIT_OBJECT_0)
        fail("terminate-owned-timeout-child");
    } else if (wait != WAIT_OBJECT_0) {
      fail("wait-owned-child");
    }
    DWORD code = 0;
    if (!GetExitCodeProcess(process.hProcess, &code))
      fail("child-exit-code");
    char child[128]{};
    DWORD count = 0;
    HANDLE result = CreateFileA(output, GENERIC_READ, FILE_SHARE_READ, nullptr,
                                OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
    bool reported = result != INVALID_HANDLE_VALUE;
    if (reported) {
      if (!ReadFile(result, child, sizeof child - 1, &count, nullptr) ||
          !CloseHandle(result) || !DeleteFileA(output))
        fail("read-clean-owned-output");
      for (DWORD i = 0; i < count; ++i)
        if (child[i] == '\n' || child[i] == '\r')
          child[i] = ' ';
    }
    if (!CloseHandle(process.hThread) || !CloseHandle(process.hProcess))
      fail("close-owned-child-handles");
    printf("case profile=%s name=%s pointer-bytes=%zu major=%u "
           "file-cookie=%016llX "
           "suspended-cookie=%016llX cookie-va=%016llX image-base=%016llX "
           "written=%u written-value=%016llX wait=%08lX exit=%08lX reported=%u "
           "child=[%s] cleanup=pass\n",
           argv[1], names[ordinal], sizeof(uintptr_t), major,
           static_cast<unsigned long long>(disk),
           static_cast<unsigned long long>(before),
           static_cast<unsigned long long>(address),
           static_cast<unsigned long long>(base), ordinal != 0,
           static_cast<unsigned long long>(values[ordinal]), wait, code,
           reported, child);
  }
  return 0;
}
