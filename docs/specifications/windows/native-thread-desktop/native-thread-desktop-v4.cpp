#define WIN32_LEAN_AND_MEAN
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>

using Query = HDESK(NTAPI *)(DWORD);
using Close = LONG(NTAPI *)(HANDLE);
using QueryObject = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
static Query query;
using GetStatus = LONG(NTAPI *)();
static GetStatus get_status;
static HANDLE ready_event, finish_event;

static void describe(const char *label, HDESK desktop) {
    wchar_t name[1024]{};
    DWORD required = 0;
    SetLastError(0x12345678);
    BOOL ok = GetUserObjectInformationW(desktop, UOI_NAME, name, sizeof(name),
                                        &required);
    DWORD error = GetLastError();
    std::printf("NAME label=%s ok=%d error=%08lX required=%lu text=", label,
                ok != 0, error, required);
    if (ok) {
        for (unsigned i = 0; i < required / 2 && i < 1024; ++i) {
            std::printf("%04X", static_cast<unsigned>(name[i]));
        }
    }
    std::putchar('\n');
    DWORD flags = 0;
    SetLastError(0x12345678);
    ok = GetHandleInformation(desktop, &flags);
    error = GetLastError();
    std::printf("HANDLE_FLAGS label=%s ok=%d error=%08lX flags=%08lX\n", label,
                ok != 0, error, flags);
    auto module = GetModuleHandleW(L"ntdll.dll");
    auto nt_query =
        reinterpret_cast<QueryObject>(GetProcAddress(module, "NtQueryObject"));
    alignas(16) unsigned char basic[128];
    std::memset(basic, 0xA5, sizeof(basic));
    ULONG got = 0;
    SetLastError(0x12345678);
    LONG status = nt_query(desktop, 0, basic, 56, &got);
    error = GetLastError();
    ULONG attributes = 0, access = 0, handles = 0, pointers = 0;
    std::memcpy(&attributes, basic, 4);
    std::memcpy(&access, basic + 4, 4);
    std::memcpy(&handles, basic + 8, 4);
    std::memcpy(&pointers, basic + 12, 4);
    std::printf("BASIC label=%s status=%08lX error=%08lX returned=%lu "
                "attrs=%08lX access=%08lX handles=%lu pointers=%lu\n",
                label, static_cast<ULONG>(status), error, got, attributes,
                access, handles, pointers);
}

static void sample(const char *phase, const char *role, DWORD id,
                   HDESK current) {
    for (unsigned iteration = 0; iteration < 3; ++iteration) {
        SetLastError(0x12345678);
        LONG status_before = get_status();
        HDESK value = query(id);
        DWORD error = GetLastError();
        LONG status_after = get_status();
        std::printf("QUERY phase=%s role=%s sample=%u id=%08lX present=%d "
                    "same-current=%d value=%llX error=%08lX nt-before=%08lX "
                    "nt-after=%08lX\n",
                    phase, role, iteration, id, value != nullptr,
                    value == current,
                    static_cast<unsigned long long>(
                        reinterpret_cast<std::uintptr_t>(value)),
                    error, static_cast<ULONG>(status_before),
                    static_cast<ULONG>(status_after));
    }
}

static DWORD WINAPI worker(void *) {
    auto teb = reinterpret_cast<const unsigned char *>(NtCurrentTeb());
    std::uintptr_t marker = 0;
    std::memcpy(&marker, teb + (sizeof(void *) == 8 ? 0x78 : 0x40),
                sizeof(marker));
    std::printf("WORKER gui-converted=%d\n", marker != 0);
    SetLastError(0x12345678);
    HDESK own = query(GetCurrentThreadId());
    DWORD own_error = GetLastError();
    std::uintptr_t after = 0;
    std::memcpy(&after, teb + (sizeof(void *) == 8 ? 0x78 : 0x40),
                sizeof(after));
    std::printf(
        "WORKER_SELF_QUERY present=%d error=%08lX gui-before=%d gui-after=%d\n",
        own != nullptr, own_error, marker != 0, after != 0);
    SetEvent(ready_event);
    if (WaitForSingleObject(finish_event, 30000) != WAIT_OBJECT_0) {
        return 2;
    }
    return 0;
}

int main(int argc, char **argv) {
    std::setvbuf(stdout, nullptr, _IONBF, 0);
    HMODULE user = LoadLibraryW(L"user32.dll");
    HMODULE win32u = LoadLibraryW(L"win32u.dll");
    query = reinterpret_cast<Query>(
        GetProcAddress(win32u, "NtUserGetThreadDesktop"));
    if (user == nullptr || win32u == nullptr || query == nullptr) {
        return 3;
    }
    std::printf("PROFILE %s pointer-bytes=%zu\n",
                argc > 1 ? argv[1] : "unknown", sizeof(void *));
    get_status = reinterpret_cast<GetStatus>(
        GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "RtlGetLastNtStatus"));
    if (!get_status) {
        return 7;
    }
    const DWORD id = GetCurrentThreadId();
    HDESK desktop = GetThreadDesktop(id);
    std::printf("CURRENT present=%d\n", desktop != nullptr);
    if (desktop == nullptr) {
        return 4;
    }
    describe("current", desktop);
    for (DWORD other : {0UL, 1UL, 2UL, 3UL, 4UL, 0x7FFFFFFFUL, 0x80000000UL,
                        0xFFFFFFFCUL, 0xFFFFFFFFUL}) {
        sample("main", "constant", other, desktop);
    }
    sample("main", "current", id, desktop);
    sample("main", "current-tag1", id | 1, desktop);
    sample("main", "current-tag2", id | 2, desktop);
    sample("main", "current-tag3", id | 3, desktop);
    sample("main", "process-id", GetCurrentProcessId(), desktop);
    ready_event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    finish_event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    DWORD thread_id = 0;
    HANDLE thread = CreateThread(nullptr, 0, worker, nullptr, 0, &thread_id);
    if (!ready_event || !finish_event || !thread ||
        WaitForSingleObject(ready_event, 30000) != WAIT_OBJECT_0) {
        return 5;
    }
    sample("live", "worker", thread_id, desktop);
    sample("live", "worker-tag1", thread_id | 1, desktop);
    SetEvent(finish_event);
    if (WaitForSingleObject(thread, 30000) != WAIT_OBJECT_0) {
        return 6;
    }
    sample("exited-open-handle", "worker", thread_id, desktop);
    CloseHandle(thread);
    sample("exited-closed-handle", "worker", thread_id, desktop);
    CloseHandle(ready_event);
    CloseHandle(finish_event);
    ready_event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    finish_event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    using Create =
        LONG(NTAPI *)(HANDLE *, ULONG, void *, HANDLE, LPTHREAD_START_ROUTINE,
                      void *, ULONG, SIZE_T, SIZE_T, SIZE_T, void *);
    auto create = reinterpret_cast<Create>(
        GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "NtCreateThreadEx"));
    HANDLE skipped = nullptr;
    LONG created = create(&skipped, 0x1FFFFF, nullptr, GetCurrentProcess(),
                          worker, nullptr, 2, 0, 0, 0, nullptr);
    std::printf("CREATE_SKIP status=%08lX present=%d\n",
                static_cast<ULONG>(created), skipped != nullptr);
    if (created < 0 || !skipped ||
        WaitForSingleObject(ready_event, 30000) != WAIT_OBJECT_0) {
        return 8;
    }
    DWORD skipped_id = GetThreadId(skipped);
    sample("skip-live", "worker", skipped_id, desktop);
    SetEvent(finish_event);
    if (WaitForSingleObject(skipped, 30000) != WAIT_OBJECT_0) {
        return 9;
    }
    sample("skip-exited-open", "worker", skipped_id, desktop);
    CloseHandle(skipped);
    sample("skip-exited-closed", "worker", skipped_id, desktop);
    CloseHandle(ready_event);
    CloseHandle(finish_event);
    HANDLE duplicate = nullptr;
    SetLastError(0x12345678);
    BOOL ok = DuplicateHandle(GetCurrentProcess(), desktop, GetCurrentProcess(),
                              &duplicate, 0, FALSE, DUPLICATE_SAME_ACCESS);
    DWORD error = GetLastError();
    std::printf("DUPLICATE ok=%d present=%d same=%d error=%08lX\n", ok != 0,
                duplicate != nullptr, duplicate == desktop, error);
    if (ok) {
        describe("duplicate", static_cast<HDESK>(duplicate));
        SetLastError(0x12345678);
        BOOL closed = CloseDesktop(static_cast<HDESK>(duplicate));
        error = GetLastError();
        std::printf("CLOSE_DUPLICATE ok=%d error=%08lX\n", closed != 0, error);
        if (!closed) {
            CloseHandle(duplicate);
        }
    }
    if (argc > 2 && std::strcmp(argv[2], "close-desktop") == 0) {
        SetLastError(0x12345678);
        BOOL closed = CloseDesktop(desktop);
        error = GetLastError();
        std::printf("CLOSE_CURRENT ok=%d error=%08lX\n", closed != 0, error);
        sample("after-close-desktop", "current", id, desktop);
    }
    if (argc > 2 && std::strcmp(argv[2], "nt-close") == 0) {
        auto close = reinterpret_cast<Close>(
            GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "NtClose"));
        SetLastError(0x12345678);
        LONG status = close(desktop);
        error = GetLastError();
        std::printf("NT_CLOSE_CURRENT status=%08lX error=%08lX\n",
                    static_cast<ULONG>(status), error);
        sample("after-nt-close", "current", id, desktop);
    }
    if (argc > 2 && std::strcmp(argv[2], "dup-close-source") == 0) {
        HANDLE copy = nullptr;
        SetLastError(0x12345678);
        BOOL copied = DuplicateHandle(
            GetCurrentProcess(), desktop, GetCurrentProcess(), &copy, 0, FALSE,
            DUPLICATE_SAME_ACCESS | DUPLICATE_CLOSE_SOURCE);
        DWORD error = GetLastError();
        std::printf("DUP_CLOSE_CURRENT ok=%d copy-present=%d error=%08lX\n",
                    copied != 0, copy != nullptr, error);
        sample("after-dup-close", "current", id, desktop);
        if (copy) {
            describe("dup-close-copy", static_cast<HDESK>(copy));
            CloseDesktop(static_cast<HDESK>(copy));
        }
    }
    describe("current-final", query(id));
    FreeLibrary(win32u);
    FreeLibrary(user);
    return 0;
}
