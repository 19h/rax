#include <cstdint>
#include <cstdio>
#include <windows.h>

using Query = ULONG(NTAPI *)();
using Initialize = ULONG_PTR(NTAPI *)();

static int observe(const char *phase) {
    HMODULE user = LoadLibraryW(L"user32.dll");
    if (user == nullptr) {
        return 7;
    }
    HMODULE module = LoadLibraryW(L"win32u.dll");
    if (module == nullptr) {
        return 2;
    }
    auto query = reinterpret_cast<Query>(
        GetProcAddress(module, "NtUserRemoteConnectState"));
    auto initialize =
        reinterpret_cast<Initialize>(GetProcAddress(module, "NtGdiInit2"));
    if (query == nullptr || initialize == nullptr) {
        FreeLibrary(module);
        return 3;
    }
    for (unsigned stage = 0; stage < 2; ++stage) {
        if (stage == 1 && initialize() == 0) {
            FreeLibrary(module);
            return 4;
        }
        for (unsigned sample = 0; sample < 32; ++sample) {
            SetLastError(0x12345678);
            ULONG value = query();
            DWORD error = GetLastError();
            std::printf("QUERY phase=%s stage=%u sample=%u value=%lu "
                        "last_error=%08lX\n",
                        phase, stage, sample, value, error);
        }
    }
    FreeLibrary(module);
    return 0;
}

static DWORD WINAPI worker(void *) {
    return static_cast<DWORD>(observe("worker"));
}

int main(int argc, char **argv) {
    std::setvbuf(stdout, nullptr, _IONBF, 0);
    std::printf("PROFILE %s pointer_bytes=%zu\n",
                argc > 1 ? argv[1] : "unknown", sizeof(void *));
    int code = observe("main");
    if (code != 0) {
        return code;
    }
    HANDLE thread = CreateThread(nullptr, 0, worker, nullptr, 0, nullptr);
    if (thread == nullptr) {
        return 5;
    }
    DWORD wait = WaitForSingleObject(thread, 30000);
    DWORD status = 6;
    if (wait == WAIT_OBJECT_0) {
        GetExitCodeThread(thread, &status);
    }
    CloseHandle(thread);
    return static_cast<int>(status);
}
