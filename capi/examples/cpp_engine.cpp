// cpp_engine.cpp — the C++ wrapper (rax.hpp) in action.
//
// Shows RAII lifetime, typed register access, a lambda code hook, exceptions
// for error handling, and a context round-trip.
//
// Build:
//   c++ -std=c++17 -I capi/include capi/examples/cpp_engine.cpp \
//       -L target/debug -lrax -o cpp_engine
//   DYLD_LIBRARY_PATH=target/debug ./cpp_engine
#include "rax.hpp"

#include <cstdint>
#include <cstdio>
#include <vector>

int main() {
    try {
        rax::Engine e(rax::Arch::X86, RAX_MODE_64);

        // mov rax,0x1337 ; mov rcx,1 ; add rax,rcx ; hlt
        const std::vector<uint8_t> code = {
            0x48, 0xC7, 0xC0, 0x37, 0x13, 0x00, 0x00,
            0x48, 0xC7, 0xC1, 0x01, 0x00, 0x00, 0x00,
            0x48, 0x01, 0xC8,
            0xF4,
        };
        const uint64_t entry = 0x1000;
        e.memWrite(entry, code);

        int traced = 0;
        e.hookCode([&](rax::Engine&, uint64_t addr, uint32_t) {
            std::printf("  exec 0x%llx\n", (unsigned long long)addr);
            ++traced;
        });

        e.setReg(RAX_X86_REG_RSP, uint64_t(0x8000));
        e.start(entry);

        rax::Exit ex = e.lastExit();
        uint64_t rax = e.regU64(RAX_X86_REG_RAX);
        std::printf("reason=%d traced=%d icount=%llu RAX=0x%llx\n",
                    ex.reason, traced, (unsigned long long)e.icount(),
                    (unsigned long long)rax);

        // Context round-trip via std::vector.
        e.setReg(RAX_X86_REG_RBX, uint64_t(0xABCD));
        std::vector<uint8_t> snap = e.contextSave();
        e.setReg(RAX_X86_REG_RBX, uint64_t(0));
        e.contextRestore(snap);
        bool ctx_ok = e.regU64(RAX_X86_REG_RBX) == 0xABCD;

        bool ok = ex.reason == RAX_STOP_HLT && rax == 0x1338 && traced >= 3 && ctx_ok;
        std::puts(ok ? "OK" : "FAILED");
        return ok ? 0 : 1;
    } catch (const rax::Error& err) {
        std::fprintf(stderr, "rax error: %s\n", err.what());
        return 1;
    }
}
