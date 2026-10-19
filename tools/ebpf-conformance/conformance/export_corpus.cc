// Export the pinned eBPF Foundation bpf_conformance suite as test vectors.
//
// Programs are produced by the suite's own parser and assembler
// (parse_test_file in libbpf_conformance_core); the runner's instruction
// classification decides which tests `bpf_conformance_runner --cpu_version
// v4` with the default conformance groups executes, and which it skips.
// Raw negative tests (non-zero unused fields, "-- error" with no message)
// are exported as programs every conforming runtime must reject.
//
// Output, one line per test:
//   run    <name> <program hex> <memory hex|-> <expected r0 hex>
//   skip   <name> <reason>
//   reject <name> <program hex>

#include <bpf_conformance_core/bpf_conformance.h>
#include <bpf_conformance_core/bpf_test_parser.h>
#include <bpf_conformance_core/opcode_names.h>

#include <algorithm>
#include <cstdio>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <optional>
#include <sstream>
#include <string>
#include <vector>

std::optional<bpf_conformance_instruction_t>
get_instruction_conformance_info(ebpf_inst inst);

static std::string
hex(const uint8_t* data, size_t len)
{
    std::string out;
    char digits[3];
    for (size_t i = 0; i < len; i++) {
        std::snprintf(digits, sizeof(digits), "%02x", data[i]);
        out += digits;
    }
    return out.empty() ? "-" : out;
}

static void
export_test(const std::filesystem::path& path)
{
    auto [memory, expected, error, program] = parse_test_file(path);
    const std::string name = path.filename().string();
    if (program.empty()) {
        std::cout << "skip " << name << " no-instructions" << std::endl;
        return;
    }
    // The runner's classification (bpf_conformance_run): CPU v4 and the
    // default groups base32/base64/atomic32/atomic64/divmul32/divmul64.
    auto required_cpu = bpf_conformance_test_cpu_version_t::v1;
    auto required_groups = bpf_conformance_groups_t::none;
    for (size_t i = 0; i < program.size(); i++) {
        if (auto info = get_instruction_conformance_info(program[i])) {
            required_cpu = std::max(required_cpu, info->cpu_version);
            required_groups |= info->groups;
        }
        if (program[i].opcode == EBPF_OP_LDDW) {
            i++;
        }
    }
    if (required_cpu > bpf_conformance_test_cpu_version_t::v4 ||
        (required_groups & ~bpf_conformance_groups_t::default_groups) != bpf_conformance_groups_t::none) {
        std::cout << "skip " << name << " unsupported-group" << std::endl;
        return;
    }
    if (!error.empty()) {
        std::cout << "skip " << name << " expected-error" << std::endl;
        return;
    }
    std::cout << "run " << name << " "
              << hex(reinterpret_cast<const uint8_t*>(program.data()), program.size() * sizeof(ebpf_inst)) << " "
              << hex(memory.data(), memory.size()) << " " << std::hex << expected << std::dec << std::endl;
}

static void
export_negative(const std::filesystem::path& path)
{
    std::ifstream in(path);
    std::string line;
    std::vector<uint8_t> raw;
    bool in_raw = false;
    while (std::getline(in, line)) {
        if (auto comment = line.find('#'); comment != std::string::npos) {
            line = line.substr(0, comment);
        }
        if (line.rfind("--", 0) == 0) {
            in_raw = line.find("raw") != std::string::npos;
            continue;
        }
        if (in_raw) {
            std::stringstream tokens(line);
            unsigned value;
            while (tokens >> std::hex >> value) {
                raw.push_back(static_cast<uint8_t>(value));
            }
        }
    }
    std::cout << "reject " << path.filename().string() << " " << hex(raw.data(), raw.size()) << std::endl;
}

int
main(int argc, char** argv)
{
    if (argc != 3) {
        std::cerr << "usage: export_corpus <tests dir> <negative dir>" << std::endl;
        return 2;
    }
    std::vector<std::filesystem::path> tests;
    for (auto& entry : std::filesystem::directory_iterator(argv[1])) {
        if (entry.path().extension() == ".data") {
            tests.push_back(entry.path());
        }
    }
    std::sort(tests.begin(), tests.end());
    for (auto& test : tests) {
        export_test(test);
    }
    std::vector<std::filesystem::path> negatives;
    for (auto& entry : std::filesystem::directory_iterator(argv[2])) {
        if (entry.path().filename().string().rfind("unused-", 0) == 0) {
            negatives.push_back(entry.path());
        }
    }
    std::sort(negatives.begin(), negatives.end());
    for (auto& negative : negatives) {
        export_negative(negative);
    }
    return 0;
}
