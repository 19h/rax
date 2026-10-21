#define WIN32_LEAN_AND_MEAN
#include <cstdio>
#include <cstring>
#include <string>
#include <windows.h>
#include <winternl.h>

using Open = NTSTATUS(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES,
                               PIO_STATUS_BLOCK, ULONG, ULONG);
struct Call {
  PHANDLE handle;
  ACCESS_MASK access;
  POBJECT_ATTRIBUTES attrs;
  PIO_STATUS_BLOCK io;
  ULONG share;
  ULONG options;
};
static NTSTATUS invoke(Open fn, const Call *call, DWORD *exception) {
  __try {
    return fn(call->handle, call->access, call->attrs, call->io, call->share,
              call->options);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
    return static_cast<NTSTATUS>(0xffffffff);
  }
}
static DWORD protection(const void *p) {
  MEMORY_BASIC_INFORMATION info{};
  return VirtualQuery(p, &info, sizeof(info)) ? info.Protect : 0;
}
static bool observe(const void *source, void *destination, SIZE_T size) {
  MEMORY_BASIC_INFORMATION info{};
  if (!VirtualQuery(source, &info, sizeof(info)) || info.State != MEM_COMMIT ||
      (info.Protect & (PAGE_GUARD | PAGE_NOACCESS)) != 0)
    return false;
  const auto start = reinterpret_cast<ULONG_PTR>(source);
  const auto base = reinterpret_cast<ULONG_PTR>(info.BaseAddress);
  if (start < base || size > info.RegionSize ||
      start - base > info.RegionSize - size)
    return false;
  __try {
    std::memcpy(destination, source, size);
    return true;
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    return false;
  }
}
static void hex(const BYTE *bytes, SIZE_T count, bool readable) {
  if (!readable) {
    std::printf("-");
    return;
  }
  for (SIZE_T i = 0; i < count; ++i)
    std::printf("%02X", bytes[i]);
}
static std::wstring native(const std::wstring &path) {
  return L"\\??\\" + path;
}
int wmain() {
  auto fn = reinterpret_cast<Open>(
      GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "NtOpenFile"));
  if (!fn)
    return 10;
  wchar_t temporary[MAX_PATH]{};
  if (!GetTempPathW(MAX_PATH, temporary))
    return 11;
  const std::wstring directory = std::wstring(temporary) +
                                 L"assist-native-open-v4-" +
                                 std::to_wstring(GetCurrentProcessId()) + L"-" +
                                 std::to_wstring(GetTickCount64());
  if (!CreateDirectoryW(directory.c_str(), nullptr))
    return 12;
  const std::wstring fixture = directory + L"\\fixture.dat";
  HANDLE created = CreateFileW(fixture.c_str(), GENERIC_WRITE, 7, nullptr,
                               CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
  if (created == INVALID_HANDLE_VALUE)
    return 13;
  DWORD written = 0;
  const bool populated =
      WriteFile(created, "fixture", 7, &written, nullptr) && written == 7;
  CloseHandle(created);
  if (!populated)
    return 14;
  HANDLE root_file = CreateFileW(fixture.c_str(), GENERIC_READ, 7, nullptr,
                                 OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
  HANDLE root_dir =
      CreateFileW(directory.c_str(), GENERIC_READ, 7, nullptr, OPEN_EXISTING,
                  FILE_FLAG_BACKUP_SEMANTICS, nullptr);
  HANDLE event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
  auto memory = static_cast<BYTE *>(VirtualAlloc(
      nullptr, 6 * 4096, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!memory || root_file == INVALID_HANDLE_VALUE ||
      root_dir == INVALID_HANDLE_VALUE || !event)
    return 15;
  std::printf(
      "profile pointer-bytes=%zu attrs-bytes=%zu iosb-bytes=%zu roles=161\n",
      sizeof(void *), sizeof(OBJECT_ATTRIBUTES), sizeof(IO_STATUS_BLOCK));
  std::fflush(stdout);
  DWORD before = 0;
  GetProcessHandleCount(GetCurrentProcess(), &before);
  for (unsigned role = 1; role <= 161; ++role) {
    DWORD previous = 0;
    VirtualProtect(memory, 6 * 4096, PAGE_READWRITE, &previous);
    std::memset(memory, 0, 6 * 4096);
    VirtualProtect(memory + 4 * 4096, 4096, PAGE_NOACCESS, &previous);
    VirtualProtect(memory + 5 * 4096, 4096, PAGE_READWRITE | PAGE_GUARD,
                   &previous);
    auto attrs = reinterpret_cast<OBJECT_ATTRIBUTES *>(memory + 3 * 4096);
    auto name = reinterpret_cast<UNICODE_STRING *>(memory + 3 * 4096 + 64);
    auto buffer = reinterpret_cast<wchar_t *>(memory + 3 * 4096 + 128);
    auto security = memory + 3 * 4096 + 1024;
    auto qos = memory + 3 * 4096 + 1152;
    std::wstring path = native(fixture);
    if (role == 2)
      path = L"\\??\\C:\\WINDOWS\\SYSTEM32\\VERSION.dll";
    if (role == 3)
      path = native(directory + L"\\missing.dat");
    if (role == 4)
      path = native(directory + L"\\missing\\child.dat");
    if (role == 5 || role == 71 || role == 72 || role == 73)
      path = native(directory);
    if (role == 37 || role == 38 || role == 40 || role == 41)
      path = L"fixture.dat";
    if (role == 39 || role == 42)
      path.clear();
    std::wstring transient;
    if (role >= 114 && role <= 121) {
      transient = directory + L"\\delete-" + std::to_wstring(role) + L".dat";
      HANDLE made = CreateFileW(transient.c_str(), GENERIC_WRITE, 7, nullptr,
                                CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
      if (made == INVALID_HANDLE_VALUE)
        return 17;
      CloseHandle(made);
      path = native(transient);
    }
    std::memcpy(buffer, path.data(), path.size() * sizeof(wchar_t));
    name->Length = static_cast<USHORT>(path.size() * sizeof(wchar_t));
    name->MaximumLength = name->Length;
    name->Buffer = buffer;
    attrs->Length = sizeof(*attrs);
    attrs->Attributes = 0x840;
    attrs->ObjectName = name;
    auto out = reinterpret_cast<PHANDLE>(memory);
    auto io = reinterpret_cast<PIO_STATUS_BLOCK>(memory + 4096);
    std::memset(out, 0xa5, 32);
    std::memset(io, 0xa5, 32);
    const auto missing = memory + 4 * 4096;
    const auto guarded = memory + 5 * 4096;
    Call call{out, 0x100021, attrs, io, 5, 0x60};
    switch (role) {
    case 6:
      call.options = 0x21;
      break;
    case 7:
      call.options = 0x20;
      break;
    case 8:
      call.handle = nullptr;
      break;
    case 9:
      call.io = nullptr;
      break;
    case 10:
      call.attrs = nullptr;
      break;
    case 11:
      call.attrs = reinterpret_cast<POBJECT_ATTRIBUTES>(1);
      break;
    case 12:
      attrs->Length = 0;
      break;
    case 13:
    case 14:
    case 15:
      call.handle = reinterpret_cast<PHANDLE>(memory + (1u << (role - 13)));
      break;
    case 16:
    case 17:
    case 18:
      call.io = reinterpret_cast<PIO_STATUS_BLOCK>(memory + 4096 +
                                                   (1u << (role - 16)));
      break;
    case 19:
      call.handle = reinterpret_cast<PHANDLE>(missing);
      break;
    case 20:
      call.io = reinterpret_cast<PIO_STATUS_BLOCK>(missing);
      break;
    case 21:
      call.attrs = reinterpret_cast<POBJECT_ATTRIBUTES>(missing);
      break;
    case 22:
      attrs->ObjectName = reinterpret_cast<PUNICODE_STRING>(missing);
      break;
    case 23:
      name->Buffer = reinterpret_cast<PWSTR>(missing);
      break;
    case 24:
      call.handle = reinterpret_cast<PHANDLE>(guarded);
      break;
    case 25:
      call.io = reinterpret_cast<PIO_STATUS_BLOCK>(guarded);
      break;
    case 26:
      call.attrs = reinterpret_cast<POBJECT_ATTRIBUTES>(guarded);
      break;
    case 27:
      attrs->ObjectName = reinterpret_cast<PUNICODE_STRING>(guarded);
      break;
    case 28:
      name->Buffer = reinterpret_cast<PWSTR>(guarded);
      break;
    case 29:
      call.handle = nullptr;
      call.attrs = reinterpret_cast<POBJECT_ATTRIBUTES>(1);
      break;
    case 30:
      call.io = nullptr;
      call.attrs = reinterpret_cast<POBJECT_ATTRIBUTES>(1);
      break;
    case 31:
      call.handle = reinterpret_cast<PHANDLE>(guarded);
      call.attrs = reinterpret_cast<POBJECT_ATTRIBUTES>(1);
      break;
    case 32:
      call.io = reinterpret_cast<PIO_STATUS_BLOCK>(guarded);
      call.attrs = reinterpret_cast<POBJECT_ATTRIBUTES>(1);
      break;
    case 33:
      call.handle = reinterpret_cast<PHANDLE>(missing);
      call.io = reinterpret_cast<PIO_STATUS_BLOCK>(guarded);
      break;
    case 34:
      call.handle = reinterpret_cast<PHANDLE>(guarded);
      call.io = reinterpret_cast<PIO_STATUS_BLOCK>(missing);
      break;
    case 35:
      attrs->Attributes = 0xffffffff;
      break;
    case 36:
      attrs->RootDirectory = reinterpret_cast<HANDLE>(0x6ffc);
      break;
    case 37:
      attrs->RootDirectory = event;
      break;
    case 38:
      attrs->RootDirectory = root_dir;
      break;
    case 39:
    case 40:
      attrs->RootDirectory = root_file;
      break;
    case 43:
      name->Length = 3;
      break;
    case 44:
      std::memmove(reinterpret_cast<BYTE *>(buffer) + 1, buffer, name->Length);
      name->Buffer =
          reinterpret_cast<PWSTR>(reinterpret_cast<BYTE *>(buffer) + 1);
      break;
    case 45:
      call.handle = reinterpret_cast<PHANDLE>(missing - sizeof(HANDLE) + 1);
      break;
    case 46:
      call.io = reinterpret_cast<PIO_STATUS_BLOCK>(missing -
                                                   sizeof(IO_STATUS_BLOCK) + 1);
      break;
    case 47:
      VirtualProtect(memory, 4096, PAGE_READONLY, &previous);
      break;
    case 48:
      VirtualProtect(memory + 4096, 4096, PAGE_READONLY, &previous);
      break;
    case 49:
      call.handle = reinterpret_cast<PHANDLE>(guarded);
      attrs->Length = 0;
      break;
    case 50:
      attrs->RootDirectory = event;
      break;
    case 51:
      attrs->SecurityQualityOfService = reinterpret_cast<void *>(1);
      break;
    case 52:
      attrs->SecurityQualityOfService = missing;
      break;
    case 53:
    case 54:
      *reinterpret_cast<ULONG *>(qos) = 12;
      *reinterpret_cast<ULONG *>(qos + 4) = role == 53 ? 4 : 2;
      qos[8] = role == 54 ? 2 : 0;
      attrs->SecurityQualityOfService = qos;
      break;
    case 55:
      attrs->SecurityDescriptor = reinterpret_cast<void *>(1);
      break;
    case 56:
      attrs->SecurityDescriptor = missing;
      break;
    case 57:
      attrs->SecurityDescriptor = security;
      break;
    case 58:
      security[0] = 1;
      *reinterpret_cast<ULONG_PTR *>(security + (sizeof(void *) == 8 ? 8 : 4)) =
          1;
      attrs->SecurityDescriptor = security;
      break;
    case 59:
      attrs->ObjectName = reinterpret_cast<PUNICODE_STRING>(1);
      break;
    case 60:
      call.access = 0x100080;
      break;
    case 61:
      call.access = 0;
      break;
    case 62:
      call.access = 0;
      call.options = 0x40;
      break;
    case 63:
      call.share = 8;
      break;
    case 64:
      call.options = 0xffffffff;
      break;
    case 65:
      call.options = 0x70;
      break;
    case 66:
      call.access = GENERIC_READ;
      break;
    case 67:
      call.access = MAXIMUM_ALLOWED;
      break;
    case 68:
      call.access = DELETE | SYNCHRONIZE;
      break;
    case 69:
      call.access = 0x400000;
      break;
    case 70:
      call.options = 0x68;
      break;
    case 71:
      call.options = 0x21;
      break;
    case 72:
      call.options = 0x21;
      call.access = GENERIC_READ;
      break;
    case 74:
      attrs->Attributes |= 2;
      break;
    case 75:
      call.handle = reinterpret_cast<PHANDLE>(attrs);
      break;
    case 76:
      call.io = reinterpret_cast<PIO_STATUS_BLOCK>(attrs);
      break;
    case 77:
      call.handle = reinterpret_cast<PHANDLE>(buffer);
      break;
    case 78:
      call.io = reinterpret_cast<PIO_STATUS_BLOCK>(buffer);
      break;
    case 79:
      call.io = reinterpret_cast<PIO_STATUS_BLOCK>(1);
      break;
    case 80:
      call.handle = reinterpret_cast<PHANDLE>(1);
      break;
    case 81:
      attrs->ObjectName = reinterpret_cast<PUNICODE_STRING>(1);
      call.handle = reinterpret_cast<PHANDLE>(guarded);
      break;
    case 82:
      attrs->SecurityQualityOfService = reinterpret_cast<void *>(1);
      call.io = reinterpret_cast<PIO_STATUS_BLOCK>(guarded);
      break;
    case 83:
      attrs->SecurityDescriptor = missing;
      call.io = reinterpret_cast<PIO_STATUS_BLOCK>(guarded);
      break;
    case 84:
      attrs->Attributes = 0xffffffff;
      call.handle = reinterpret_cast<PHANDLE>(guarded);
      break;
    case 85:
      name->Length = 0;
      name->Buffer = reinterpret_cast<PWSTR>(1);
      break;
    case 86:
      name->Length = 65535;
      break;
    case 87:
      name->Length = 65534;
      break;
    }
    if (role >= 88 && role <= 104) {
      const SIZE_T available = role - 88;
      call.io = reinterpret_cast<PIO_STATUS_BLOCK>(missing - available);
      if (available)
        std::memset(call.io, 0xa5, available);
    }
    if (role >= 105 && role <= 113) {
      const SIZE_T available = role - 105;
      call.handle = reinterpret_cast<PHANDLE>(missing - available);
      if (available)
        std::memset(call.handle, 0xa5, available);
    }
    if (role >= 114 && role <= 121) {
      call.access |= DELETE;
      call.options |= 0x1000;
      switch (role - 114) {
      case 0:
        call.handle = nullptr;
        break;
      case 1:
        call.handle = reinterpret_cast<PHANDLE>(missing);
        break;
      case 2:
        call.handle = reinterpret_cast<PHANDLE>(guarded);
        break;
      case 3:
        call.io = nullptr;
        break;
      case 4:
        call.io = reinterpret_cast<PIO_STATUS_BLOCK>(missing);
        break;
      case 5:
        call.io = reinterpret_cast<PIO_STATUS_BLOCK>(guarded);
        break;
      case 6:
        call.io = reinterpret_cast<PIO_STATUS_BLOCK>(
            missing - sizeof(IO_STATUS_BLOCK) + 1);
        std::memset(call.io, 0xa5, sizeof(IO_STATUS_BLOCK) - 1);
        break;
      }
    }
    if (role >= 122 && role <= 153)
      attrs->Attributes = 1u << (role - 122);
    if (role >= 154 && role <= 161) {
      const ACCESS_MASK access[] = {GENERIC_READ | SYNCHRONIZE,
                                    GENERIC_WRITE | SYNCHRONIZE,
                                    GENERIC_EXECUTE | SYNCHRONIZE,
                                    MAXIMUM_ALLOWED | SYNCHRONIZE,
                                    0x100080,
                                    0x100020,
                                    0x100001,
                                    SYNCHRONIZE};
      call.access = access[role - 154];
    }
    if (role == 45)
      std::memset(call.handle, 0xa5, sizeof(HANDLE) - 1);
    if (role == 46)
      std::memset(call.io, 0xa5, sizeof(IO_STATUS_BLOCK) - 1);
    BYTE prior_handle[sizeof(HANDLE)]{};
    const bool prior_readable =
        observe(call.handle, prior_handle, sizeof(prior_handle));
    DWORD role_before = 0;
    GetProcessHandleCount(GetCurrentProcess(), &role_before);
    DWORD exception = 0;
    SetLastError(0x12345678);
    const NTSTATUS status = invoke(fn, &call, &exception);
    const DWORD syscall_error = GetLastError();
    DWORD attrs_before_cleanup = 0, error_before_cleanup = 0;
    if (!transient.empty()) {
      SetLastError(0);
      attrs_before_cleanup = GetFileAttributesW(transient.c_str());
      error_before_cleanup = GetLastError();
    }
    BYTE handle_bytes[sizeof(HANDLE)]{}, io_bytes[sizeof(IO_STATUS_BLOCK)]{};
    const DWORD hp = protection(call.handle), ip = protection(call.io);
    const bool hr = observe(call.handle, handle_bytes, sizeof(handle_bytes));
    const bool ir = observe(call.io, io_bytes, sizeof(io_bytes));
    ULONG granted = 0;
    NTSTATUS basic_status = static_cast<NTSTATUS>(0xffffffff);
    NTSTATUS standard_status = basic_status, network_status = basic_status,
             root_status = basic_status;
    unsigned directory_flag = 0;
    bool valid = false, closed = false;
    DWORD handle_flags = 0;
    const bool changed =
        hr && (!prior_readable || std::memcmp(prior_handle, handle_bytes,
                                              sizeof(handle_bytes)) != 0);
    if (exception == 0 && hr && (status == 0 || changed)) {
      HANDLE handle = nullptr;
      std::memcpy(&handle, handle_bytes, sizeof(handle));
      valid = GetHandleInformation(handle, &handle_flags) != FALSE;
      if (valid) {
        using QO = NTSTATUS(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);
        using QI =
            NTSTATUS(NTAPI *)(HANDLE, PIO_STATUS_BLOCK, PVOID, ULONG, ULONG);
        using QA = NTSTATUS(NTAPI *)(POBJECT_ATTRIBUTES, PVOID);
        const auto module = GetModuleHandleW(L"ntdll.dll");
        auto qo = reinterpret_cast<QO>(GetProcAddress(module, "NtQueryObject"));
        auto qi = reinterpret_cast<QI>(
            GetProcAddress(module, "NtQueryInformationFile"));
        auto qa = reinterpret_cast<QA>(
            GetProcAddress(module, "NtQueryAttributesFile"));
        alignas(8) BYTE object[56]{}, basic[40]{}, standard[24]{},
            network[56]{};
        ULONG returned = 0;
        IO_STATUS_BLOCK local{};
        if (!qo || !qi || !qa)
          return 18;
        if (qo(handle, 0, object, sizeof(object), &returned) == 0)
          std::memcpy(&granted, object + 4, 4);
        basic_status = qi(handle, &local, basic, sizeof(basic), 4);
        standard_status = qi(handle, &local, standard, sizeof(standard), 5);
        network_status = qi(handle, &local, network, sizeof(network), 34);
        if (standard_status == 0)
          directory_flag = standard[21];
        wchar_t empty[] = L"";
        UNICODE_STRING root_name{};
        root_name.Buffer = empty;
        OBJECT_ATTRIBUTES rooted{};
        rooted.Length = sizeof(rooted);
        rooted.RootDirectory = handle;
        rooted.ObjectName = &root_name;
        rooted.Attributes = 0x840;
        root_status = qa(&rooted, basic);
        if (handle_flags & HANDLE_FLAG_PROTECT_FROM_CLOSE)
          SetHandleInformation(handle, HANDLE_FLAG_PROTECT_FROM_CLOSE, 0);
        closed = CloseHandle(handle) != FALSE;
      }
    }
    BYTE partial_io[sizeof(IO_STATUS_BLOCK)]{},
        io_mask[sizeof(IO_STATUS_BLOCK)]{};
    for (SIZE_T i = 0; i < sizeof(IO_STATUS_BLOCK); ++i)
      io_mask[i] = observe(reinterpret_cast<const BYTE *>(call.io) + i,
                           partial_io + i, 1)
                       ? 0xff
                       : 0;
    DWORD attrs_after_cleanup = 0, error_after_cleanup = 0;
    if (!transient.empty()) {
      SetLastError(0);
      attrs_after_cleanup = GetFileAttributesW(transient.c_str());
      error_after_cleanup = GetLastError();
      DeleteFileW(transient.c_str());
    }
    DWORD role_after = 0;
    GetProcessHandleCount(GetCurrentProcess(), &role_after);
    std::printf("case role=%u status=%08lX exception=%08lX hprot=%08lX "
                "iprot=%08lX hvalid=%u closed=%u hflags=%08lX hout=",
                role, static_cast<ULONG>(status), exception, hp, ip, valid,
                closed, handle_flags);
    hex(handle_bytes, sizeof(handle_bytes), hr);
    std::printf(" iosb=");
    hex(io_bytes, sizeof(io_bytes), ir);
    std::printf(" io-prefix=");
    hex(partial_io, sizeof(partial_io), true);
    std::printf(" io-readable=");
    hex(io_mask, sizeof(io_mask), true);
    std::printf(" handle-count-before=%lu handle-count-after=%lu changed=%u\n",
                role_before, role_after, static_cast<unsigned>(changed));
    std::printf("diag role=%u api-last-error=%08lX before-attrs=%08lX "
                "before-error=%lu after-attrs=%08lX after-error=%lu\n",
                role, syscall_error, attrs_before_cleanup, error_before_cleanup,
                attrs_after_cleanup, error_after_cleanup);
    std::printf(
        "owned role=%u granted=%08lX basic-status=%08lX standard-status=%08lX "
        "network-status=%08lX directory=%u rooted-basic-status=%08lX\n",
        role, granted, static_cast<ULONG>(basic_status),
        static_cast<ULONG>(standard_status), static_cast<ULONG>(network_status),
        directory_flag, static_cast<ULONG>(root_status));
    std::fflush(stdout);
  }
  DWORD after = 0;
  GetProcessHandleCount(GetCurrentProcess(), &after);
  std::printf("finished=1 handle-count-before=%lu handle-count-after=%lu\n",
              before, after);
  CloseHandle(event);
  CloseHandle(root_file);
  CloseHandle(root_dir);
  VirtualFree(memory, 0, MEM_RELEASE);
  DeleteFileW(fixture.c_str());
  RemoveDirectoryW(directory.c_str());
  return before == after ? 0 : 16;
}
