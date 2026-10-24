#define WIN32_LEAN_AND_MEAN
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>

using Query = BOOL(NTAPI *)(HANDLE, void *);
using Status = LONG(NTAPI *)();
static Query query;
static Status last_status;

static bool copy_readable(const void *source, unsigned char *output,
                          size_t size) {
  __try {
    std::memcpy(output, source, size);
    return true;
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    return false;
  }
}

static void sample(const char *phase, const char *role, HANDLE handle,
                   const char *buffer, unsigned offset, unsigned iteration) {
  alignas(16) unsigned char storage[32];
  std::memset(storage, 0xA5, sizeof(storage));
  unsigned char *region = nullptr;
  void *output = storage + offset;
  if (std::strcmp(buffer, "null") == 0) {
    output = nullptr;
  } else if (std::strcmp(buffer, "one") == 0) {
    output = reinterpret_cast<void *>(1);
  } else if (std::strcmp(buffer, "stack") != 0) {
    region = static_cast<unsigned char *>(
        VirtualAlloc(nullptr, 8192, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE));
    if (!region) {
      std::printf("ALLOCATION_FAILED error=%lu\n", GetLastError());
      return;
    }
    std::memset(region, 0xA5, 8192);
    DWORD old = 0;
    if (std::strcmp(buffer, "readonly") == 0) {
      VirtualProtect(region, 4096, PAGE_READONLY, &old);
      output = region;
    } else if (std::strcmp(buffer, "noaccess") == 0) {
      VirtualProtect(region, 4096, PAGE_NOACCESS, &old);
      output = region;
    } else if (std::strcmp(buffer, "guard") == 0) {
      VirtualProtect(region, 4096, PAGE_READWRITE | PAGE_GUARD, &old);
      output = region;
    } else if (std::strcmp(buffer, "cross-readonly") == 0) {
      VirtualProtect(region + 4096, 4096, PAGE_READONLY, &old);
      output = region + 4092;
    } else {
      VirtualProtect(region + 4096, 4096, PAGE_NOACCESS, &old);
      output = region + 4092;
    }
  }
  SetLastError(0x12345678);
  LONG before = last_status();
  BOOL result = query(handle, output);
  DWORD error = GetLastError();
  LONG after = last_status();
  MEMORY_BASIC_INFORMATION info{};
  VirtualQuery(output, &info, sizeof(info));
  unsigned char data[8];
  std::memset(data, 0xCC, sizeof(data));
  bool readable = (info.Protect & (PAGE_GUARD | PAGE_NOACCESS)) == 0 &&
                  copy_readable(output, data, sizeof(data));
  std::printf("QUERY phase=%s role=%s buffer=%s offset=%u sample=%u "
              "result=%u error=%08lX nt-before=%08lX nt-after=%08lX "
              "protect=%08lX readable=%u data=",
              phase, role, buffer, offset, iteration,
              static_cast<unsigned>(result != 0), error,
              static_cast<ULONG>(before), static_cast<ULONG>(after),
              info.Protect, static_cast<unsigned>(readable));
  if (readable) {
    for (unsigned char byte : data) {
      std::printf("%02X", static_cast<unsigned>(byte));
    }
  }
  unsigned changed = 0;
  for (unsigned char byte : storage) {
    changed += byte != 0xA5;
  }
  std::printf(" stack-changed=%u\n", changed);
  if (region) {
    VirtualFree(region, 0, MEM_RELEASE);
  }
}

static HANDLE open_current(const char *role, DWORD access) {
  SetLastError(0x12345678);
  HANDLE handle = OpenProcess(access, FALSE, GetCurrentProcessId());
  DWORD error = GetLastError();
  std::printf("OPEN role=%s access=%08lX present=%u error=%08lX\n", role,
              access, static_cast<unsigned>(handle != nullptr), error);
  return handle;
}

static int observe(const char *phase) {
  struct Subject {
    const char *name;
    HANDLE handle;
    bool owned;
  } subjects[] = {
      {"current", GetCurrentProcess(), false},
      {"null", nullptr, false},
      {"thread-pseudo", GetCurrentThread(), false},
      {"constant-1234", reinterpret_cast<HANDLE>(0x1234), false},
      {"real-query",
       open_current("real-query", PROCESS_QUERY_LIMITED_INFORMATION), true},
      {"real-query-full",
       open_current("real-query-full", PROCESS_QUERY_INFORMATION), true},
      {"real-zero", open_current("real-zero", 0), true},
      {"real-synchronize", open_current("real-synchronize", SYNCHRONIZE), true},
  };
  for (const auto &subject : subjects) {
    if (subject.owned && !subject.handle) {
      std::printf("OPEN_UNAVAILABLE role=%s\n", subject.name);
      continue;
    }
    for (unsigned iteration = 0; iteration < 3; ++iteration) {
      for (unsigned offset = 0; offset < 8; ++offset) {
        sample(phase, subject.name, subject.handle, "stack", offset, iteration);
      }
      for (const char *buffer : {"null", "one", "readonly", "noaccess", "guard",
                                 "cross-readonly", "cross-noaccess"}) {
        sample(phase, subject.name, subject.handle, buffer, 0, iteration);
      }
    }
    if (subject.owned) {
      CloseHandle(subject.handle);
    }
  }
  return 0;
}

static DWORD WINAPI worker(void *) {
  return static_cast<DWORD>(observe("worker"));
}

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  std::printf("PROFILE %s pointer_bytes=%zu\n", argc > 1 ? argv[1] : "unknown",
              sizeof(void *));
  auto user = LoadLibraryW(L"user32.dll");
  auto win32u = LoadLibraryW(L"win32u.dll");
  auto nt = GetModuleHandleW(L"ntdll.dll");
  if (!user || !win32u || !nt) {
    return 2;
  }
  query = reinterpret_cast<Query>(
      GetProcAddress(win32u, "NtUserGetProcessUIContextInformation"));
  last_status =
      reinterpret_cast<Status>(GetProcAddress(nt, "RtlGetLastNtStatus"));
  if (!query || !last_status) {
    return 3;
  }
  if (argc > 2) {
    auto set = reinterpret_cast<BOOL(WINAPI *)(HANDLE)>(
        GetProcAddress(user, "SetProcessDpiAwarenessContext"));
    intptr_t value = std::strcmp(argv[2], "system") == 0 ? -2
                     : std::strcmp(argv[2], "pm") == 0   ? -3
                                                         : -4;
    SetLastError(0x12345678);
    BOOL result = set ? set(reinterpret_cast<HANDLE>(value)) : FALSE;
    std::printf("DPI_CONTROL available=%u result=%u error=%08lX\n",
                static_cast<unsigned>(set != nullptr),
                static_cast<unsigned>(result != 0), GetLastError());
  }
  int code = observe("main");
  if (code != 0) {
    return code;
  }
  HANDLE thread = CreateThread(nullptr, 0, worker, nullptr, 0, nullptr);
  if (!thread) {
    return 5;
  }
  DWORD result = 6;
  if (WaitForSingleObject(thread, 30000) == WAIT_OBJECT_0) {
    GetExitCodeThread(thread, &result);
  }
  CloseHandle(thread);
  FreeLibrary(win32u);
  FreeLibrary(user);
  return static_cast<int>(result);
}
