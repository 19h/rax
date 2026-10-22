#include <windows.h>
#include <winternl.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <vector>
#include <array>

using ReadMemory = NTSTATUS(NTAPI *)(HANDLE, PVOID, PVOID, SIZE_T, PSIZE_T);
using QueryObject = NTSTATUS(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);
constexpr SIZE_T page = 4096;
constexpr SIZE_T extent = page * 80;
static unsigned calls;

static DWORD invoke(ReadMemory read, HANDLE handle, void *source, void *output,
                    SIZE_T length, SIZE_T *returned, ULONG *status) {
    __try {
        *status = static_cast<ULONG>(read(handle, source, output, length, returned));
        return 0;
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        return GetExceptionCode();
    }
}

struct Arena {
    unsigned char *base{};
    std::vector<unsigned char> before;
    std::array<DWORD, 3> protections{};
    unsigned fault_page;
    Arena(unsigned role, unsigned pattern, DWORD middle, DWORD right, unsigned fault = 2) : before(extent), fault_page(fault) {
        base = static_cast<unsigned char *>(VirtualAlloc(nullptr, extent,
            MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
        if (!base) std::abort();
        for (SIZE_T i = 0; i < extent; ++i) {
            before[i] = role == 0 ? static_cast<unsigned char>(i * 17 + 5 + pattern)
                : static_cast<unsigned char>((role == 1 ? 0xA5 : 0xD3) ^ pattern);
        }
        std::memcpy(base, before.data(), extent);
        protections = {PAGE_READWRITE, middle, right};
        DWORD old;
        if (!VirtualProtect(base + page, page, middle, &old)
            || !VirtualProtect(base + page * fault_page, page, right, &old)) std::abort();
    }
    ~Arena() { if (base) VirtualFree(base, 0, MEM_RELEASE); }
    void observe() {
        for (unsigned i = 0; i < 3; ++i) {
            MEMORY_BASIC_INFORMATION info{};
            if (!VirtualQuery(base + (i == 2 ? fault_page : i) * page, &info, sizeof(info))) std::abort();
            protections[i] = info.Protect;
        }
        DWORD old;
        if (!VirtualProtect(base, extent, PAGE_READWRITE, &old)) std::abort();
    }
};

static uintptr_t pointer(unsigned kind, unsigned role, Arena *a[3]) {
    auto middle = reinterpret_cast<uintptr_t>(a[role]->base + page);
    switch (kind) {
    case 0: return middle + 16;
    case 1: return 0;
    case 2: return 1;
    case 3: return middle + 17;
    case 4: return reinterpret_cast<uintptr_t>(a[role]->base + a[role]->fault_page * page - 4);
    case 5: return reinterpret_cast<uintptr_t>(a[role]->base + a[role]->fault_page * page);
    case 6: return sizeof(void *) == 8 ? uintptr_t(0x7FFFFFFFFFF0ULL) : uintptr_t(0xFFFFFFF0);
    case 8: return reinterpret_cast<uintptr_t>(a[0]->base + page + 16);
    case 9: return reinterpret_cast<uintptr_t>(a[0]->base + page + 17);
    case 10: return reinterpret_cast<uintptr_t>(a[0]->base + page + 24);
    case 11: return reinterpret_cast<uintptr_t>(a[0]->base + page + 15);
    case 12: return reinterpret_cast<uintptr_t>(a[0]->base + page + 8);
    default: return ~uintptr_t(0);
    }
}

static bool row(ReadMemory read, HANDLE handle, unsigned handle_kind, ULONG grant,
                SIZE_T length, unsigned source_kind, unsigned output_kind,
                unsigned return_kind, DWORD source_protect, DWORD output_protect,
                DWORD return_protect, unsigned pattern, unsigned fault_role = 3, unsigned fault_page = 2) {
    Arena source(0, pattern, source_protect, PAGE_NOACCESS, (fault_role == 0 || fault_role == 3) ? fault_page : 79);
    Arena output(1, pattern, output_protect, PAGE_NOACCESS, (fault_role == 1 || fault_role == 3) ? fault_page : 79);
    Arena returned(2, pattern, return_protect, PAGE_NOACCESS, (fault_role == 2 || fault_role == 3) ? fault_page : 79);
    Arena *arenas[3] = {&source, &output, &returned};
    uintptr_t src = pointer(source_kind, 0, arenas);
    uintptr_t dst = pointer(output_kind, 1, arenas);
    uintptr_t ret;
    switch (return_kind) {
    case 0: ret = pointer(0, 2, arenas); break;
    case 1: ret = 0; break;
    case 2: ret = 1; break;
    case 3: ret = pointer(3, 2, arenas); break;
    case 4: ret = dst; break;
    case 5: ret = dst + 4; break;
    case 6: ret = src; break;
    case 7: ret = pointer(4, 2, arenas); break;
    default: ret = ~uintptr_t(0); break;
    }
    ULONG status = 0xCCCCCCCC;
    SetLastError(0x12345678);
    DWORD exception = invoke(read, handle, reinterpret_cast<void *>(src),
        reinterpret_cast<void *>(dst), length, reinterpret_cast<SIZE_T *>(ret), &status);
    DWORD error = GetLastError();
    for (auto *arena : arenas) arena->observe();
    std::printf("{\"id\":%u,\"handle\":%u,\"grant\":%lu,\"length\":%llu,"
        "\"source\":%u,\"output\":%u,\"returned\":%u,\"source-protect\":%lu,"
        "\"output-protect\":%lu,\"returned-protect\":%lu,\"pattern\":%u,"
        "\"status\":%lu,\"exception\":%lu,\"last-error\":%lu,\"protections\":[",
        calls, handle_kind, grant, static_cast<unsigned long long>(length),
        source_kind, output_kind, return_kind, source_protect, output_protect,
        return_protect, pattern, status, exception, error);
    for (unsigned role = 0; role < 3; ++role) {
        if (role) std::printf(",");
        std::printf("[%lu,%lu,%lu]", arenas[role]->protections[0],
            arenas[role]->protections[1], arenas[role]->protections[2]);
    }
    std::printf("],\"extent\":%llu,\"fault-pages\":[%u,%u,%u],\"changes\":[", static_cast<unsigned long long>(extent), source.fault_page, output.fault_page, returned.fault_page);
    bool comma = false;
    for (unsigned role = 0; role < 3; ++role) {
        auto *a = arenas[role];
        SIZE_T at = 0;
        constexpr SIZE_T block = 256;
        while (at < extent) {
            if (std::memcmp(a->base + at, a->before.data() + at, block) == 0) { at += block; continue; }
            SIZE_T begin = at;
            at += block;
            while (at < extent && std::memcmp(a->base + begin, a->base + at, block) == 0
                && std::memcmp(a->base + at, a->before.data() + at, block) != 0) at += block;
            if (comma) std::printf(",");
            comma = true;
            std::printf("{\"role\":%u,\"offset\":%llu,\"repeat\":%llu,\"data\":\"", role,
                static_cast<unsigned long long>(begin), static_cast<unsigned long long>((at - begin) / block));
            for (SIZE_T i = begin; i < begin + block; ++i) std::printf("%02X", a->base[i]);
            std::printf("\"}");
        }
    }
    std::printf("]}\n");
    ++calls;
    return exception == 0;
}

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    auto module = GetModuleHandleW(L"ntdll.dll");
    auto read = reinterpret_cast<ReadMemory>(GetProcAddress(module, "NtReadVirtualMemory"));
    auto query = reinterpret_cast<QueryObject>(GetProcAddress(module, "NtQueryObject"));
    if (!read || !query) return 3;
    SYSTEM_INFO info{}; GetSystemInfo(&info);
    if (info.dwPageSize != page) return 4;
    std::printf("META profile=%s pointer=%zu page=%lu\n", argv[1], sizeof(void *), info.dwPageSize);
    HANDLE event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (!event) return 5;
    struct Handle { HANDLE value; ULONG grant; bool owned; };
    std::vector<Handle> handles = {{GetCurrentProcess(), 0xFFFFFFFF, false},
        {nullptr, 0, false}, {GetCurrentThread(), 0, false}, {event, 0, false}};
    for (DWORD desired : {0ul, 0x10ul, 0x400ul, 0x1000ul}) {
        HANDLE duplicate = nullptr;
        if (!DuplicateHandle(GetCurrentProcess(), GetCurrentProcess(), GetCurrentProcess(),
                &duplicate, desired, FALSE, 0)) return 6;
        alignas(16) ULONG object[16]{};
        ULONG bytes = 0;
        if (query(duplicate, 0, object, 56, &bytes) != 0) return 7;
        handles.push_back({duplicate, object[1], true});
        std::printf("HANDLE index=%zu desired=%lu granted=%lu\n", handles.size()-1, desired, object[1]);
    }
    bool ok = true;
    for (unsigned pattern : {0u, 0x5Au})
        for (unsigned dst : {8u, 9u, 10u, 11u, 12u})
            for (SIZE_T length : {SIZE_T(1), SIZE_T(8), SIZE_T(17), SIZE_T(511), SIZE_T(512), SIZE_T(513), SIZE_T(4096), SIZE_T(4097)})
                for (unsigned ret : {0u, 4u, 5u, 6u})
                    ok &= row(read, handles[0].value, 0, handles[0].grant, length,
                        0, dst, ret, 4, 4, 4, pattern, 3, 79);
    for (unsigned role : {0u, 1u})
        for (unsigned fault : {2u, 3u, 5u, 9u, 15u, 17u, 18u, 33u, 49u, 65u})
            for (SIZE_T length : {SIZE_T(512), SIZE_T(513), SIZE_T(4096), SIZE_T(4097), SIZE_T(32768), SIZE_T(32769), SIZE_T(65536), SIZE_T(65537), SIZE_T(131072), SIZE_T(262144)})
                ok &= row(read, handles[0].value, 0, handles[0].grant, length,
                    0, 0, 0, 4, 4, 4, 0, role, fault);
    for (auto handle : handles) if (handle.owned) CloseHandle(handle.value);
    CloseHandle(event);
    std::printf("END calls=%u all-returned=%u\n", calls, ok ? 1 : 0);
    return ok ? 0 : 8;
}
