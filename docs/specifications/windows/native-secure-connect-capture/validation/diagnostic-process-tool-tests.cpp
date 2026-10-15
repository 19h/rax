#include "emulation/process_elf_fixture.hpp"
#include "emulation/process_macho_fixture.hpp"
#include <assist/common/filesystem_utf8.hpp>
#include <assist/core/debug_sink.hpp>
#include <assist/render/classification.hpp>
#include <assist/tools/tool_registry/detail/factories/emulation_solver_and_frida.hpp>
#include <cctype>
#include <cstdio>
#include <filesystem>
#include <fstream>
#include <random>
#include <stdexcept>

namespace assist {
std::string get_assist_data_directory() { return {}; }
} // namespace assist
namespace assist::debug_sink {
void write(const char*, nlohmann::json) {}
} // namespace assist::debug_sink
using Json = nlohmann::json;
static unsigned checks = 0;
static void check(bool condition, const char* message) {
    ++checks;
    if (!condition) throw std::runtime_error(message);
}
static Json execute(assist::Tool& tool, Json input) {
    auto result = tool.executor(input);
    if (!result.success) throw std::runtime_error(result.error.value_or("tool failed"));
    if (!result.structured_data) throw std::runtime_error("tool returned no structured data");
    auto out = *result.structured_data;
    out.erase("_view");
    return out;
}
struct TempDirectory {
    std::filesystem::path path;
    TempDirectory() {
        std::random_device random;
        for (unsigned n = 0; n < 100; ++n) {
            auto candidate = std::filesystem::temp_directory_path() /
                             ("assist-elf-" + std::to_string(random()) + "-" + std::to_string(random()));
            if (std::filesystem::create_directory(candidate)) {
                path = candidate;
                return;
            }
        }
        throw std::runtime_error("cannot create ELF test directory");
    }
    ~TempDirectory() {
        std::error_code ignored;
        std::filesystem::remove_all(path, ignored);
    }
};
static void linux_processes() {
    TempDirectory directory;
    auto tool = assist::tools::create_emulate_process_tool(assist::path_to_utf8(directory.path));
    for (const auto& abi : process_elf_fixture::abis) {
        const auto bytes = process_elf_fixture::image(abi);
        const auto file = directory.path / abi.name;
        {
            std::ofstream output(file, std::ios::binary);
            output.write(reinterpret_cast<const char*>(bytes.data()), bytes.size());
        }
        auto opened =
            execute(tool, {{"operation", "open"}, {"path", abi.name}, {"options", {{"personality", "linux"}}}});
        const auto id = opened.at("process_id");
        check(opened["state"]["personality"] == "linux", "ELF open summary failed");
        check(opened["state"]["loaded_program"]["entry"] == "0x401000", "ELF program entry missing");
        check(!opened["state"].contains("modules_count"), "ELF invented module count");
        auto threads = execute(tool, {{"operation", "inspect"}, {"process_id", id}, {"section", "threads"}});
        const auto tid = threads["items"][0]["id"];
        auto context = execute(tool, {{"operation", "read_context"}, {"process_id", id}, {"thread_id", tid}});
        check(context["context_format"] == "linux_prstatus", "ELF context mislabeled");
        check(context["bytes"] == abi.context_bytes, "ELF context truncated");
        check(context["architecture"] == abi.name, "ELF context architecture missing");
        execute(tool, {{"operation", "write_context"},
                       {"process_id", id},
                       {"thread_id", tid},
                       {"data_hex", context["data_hex"]}});
        const auto missing = tool.executor({{"operation", "inspect"}, {"process_id", id}, {"section", "modules"}});
        check(!missing.success && missing.error.value_or("").find("unavailable for linux") != std::string::npos,
              "ELF module limit not explicit");
        auto mappings =
            execute(tool, {{"operation", "inspect"}, {"process_id", id}, {"section", "mappings"}, {"limit", 1}});
        check(mappings["items"].size() == 1 && mappings["truncated"] == true, "ELF mapping page missing");
        auto run = execute(tool, {{"operation", "run"}, {"process_id", id}});
        check(run["run"]["reason"] == "exited" && run["run"]["exit_code"] == 37, "ELF tool execution failed");
        check(execute(tool, {{"operation", "list"}})["processes"].size() == 1, "ELF omitted from list");
        execute(tool, {{"operation", "close"}, {"process_id", id}});
    }
    check(execute(tool, {{"operation", "list"}})["processes"].empty(), "ELF session leaked");
}

/// What a refused read or write tells its caller: the refused byte, the mapping
/// it lies in and that mapping's protection, as a structured failure; never
/// the engine's type dump or a status number. The fixture maps its image r-x
/// at [0x400000, 0x402000).
static void refused_accesses() {
    TempDirectory directory;
    auto tool = assist::tools::create_emulate_process_tool(assist::path_to_utf8(directory.path));
    const auto& abi = process_elf_fixture::abis[0];
    const auto bytes = process_elf_fixture::image(abi);
    {
        std::ofstream output(directory.path / abi.name, std::ios::binary);
        output.write(reinterpret_cast<const char*>(bytes.data()), bytes.size());
    }
    auto opened = execute(tool, {{"operation", "open"}, {"path", abi.name}, {"options", {{"personality", "linux"}}}});
    const auto id = opened.at("process_id");

    auto mappings = execute(tool, {{"operation", "inspect"}, {"process_id", id}, {"section", "mappings"}});
    bool image_listed = false;
    for (const auto& item : mappings["items"]) {
        const auto bits = item.at("permissions").get<unsigned>();
        const std::string expected =
            std::string(bits & 1 ? "r" : "-") + (bits & 2 ? "w" : "-") + (bits & 4 ? "x" : "-");
        check(item.value("protection", std::string()) == expected, "a mapping's protection disagrees with its bits");
        image_listed = image_listed || (item["start"] == "0x400000" && item["protection"] == "r-x");
    }
    check(image_listed, "the image mapping is not listed as r-x at 0x400000");

    const auto write = tool.executor(
        {{"operation", "write_memory"}, {"process_id", id}, {"address", "0x401000"}, {"data_hex", "9090"}});
    check(!write.success && write.structured_data, "a refused write is not a structured failure");
    const auto& refused = *write.structured_data;
    const std::string error = write.error.value_or("");
    check(error.rfind("Cannot write 2 bytes at 0x401000: it is in the r-x mapping 0x400000", 0) == 0,
          "a refused write does not name its mapping");
    check(error.find("which is not writable") != std::string::npos, "a refused write does not say why");
    check(error.find("GuestMemoryFault") == std::string::npos && error.find("(8)") == std::string::npos &&
              error.find("4198400") == std::string::npos,
          "a refused write leaks engine formatting");
    check(refused["fault"]["kind"] == "permission" && refused["fault"]["access"] == "write" &&
              refused["fault"]["address"] == "0x401000" && refused["fault"]["mapping"]["protection"] == "r-x" &&
              refused["fault"]["mapping"]["start"] == "0x400000",
          "a refused write's fault fields differ");
    check(refused["next_actions"][0]["section"] == "mappings" && refused["next_actions"][0]["process_id"] == id,
          "a refused write suggests no mapping table");
    check(refused.value("engine_error", std::string()) ==
              "guest memory fault: failed to write at 0x401000: memory permission violation",
          "the engine's own words are not readable");
    check(refused["state"].contains("mappings_count") && refused["operation"] == "write_memory",
          "a refused write lost its state or operation");
    check(execute(tool, {{"operation", "read_memory"}, {"process_id", id}, {"address", "0x401000"}, {"size", 2}})
                  .at("data_hex") == "b83c",
          "a refused write changed the guest");

    const auto read =
        tool.executor({{"operation", "read_memory"}, {"process_id", id}, {"address", "0x10"}, {"size", 4}});
    check(!read.success && read.structured_data && (*read.structured_data)["fault"]["kind"] == "unmapped" &&
              (*read.structured_data)["fault"]["mapping"].is_null(),
          "an unmapped read is not reported as unmapped");
    check(read.error.value_or("") == "Cannot read 4 bytes at 0x10: no guest mapping holds it.",
          "an unmapped read's sentence differs");

    // A range that wraps the address space is a bounds error: the engine's
    // words, but never its status number.
    const auto wrapped = tool.executor(
        {{"operation", "read_memory"}, {"process_id", id}, {"address", "0xfffffffffffffffe"}, {"size", 4}});
    check(!wrapped.success && wrapped.error.value_or("").find("(9)") == std::string::npos &&
              wrapped.error.value_or("").find("RAX process operation failed") == std::string::npos,
          "a wrapped read shows a status number");

    auto threads = execute(tool, {{"operation", "inspect"}, {"process_id", id}, {"section", "threads"}});
    const auto sp = std::stoull(threads["items"][0]["sp"].get<std::string>(), nullptr, 16);
    char stack[32];
    std::snprintf(stack, sizeof(stack), "0x%llx", static_cast<unsigned long long>(sp - 128));
    check(execute(tool, {{"operation", "write_memory"}, {"process_id", id}, {"address", stack}, {"data_hex", "abcd"}})
                  .at("bytes_written") == 2,
          "a stack write is refused");
    execute(tool, {{"operation", "close"}, {"process_id", id}});
}

/// A guest killed by a signal reports it by name, beside its number and code:
/// the fixture's entry patched to `ud2` dies of SIGILL with ILL_ILLOPN, as the
/// boo-linux binary did in session 66edd8c3.
static void signal_death() {
    TempDirectory directory;
    auto tool = assist::tools::create_emulate_process_tool(assist::path_to_utf8(directory.path));
    auto bytes = process_elf_fixture::image(process_elf_fixture::abis[0]);
    bytes[4096] = 0x0f;
    bytes[4097] = 0x0b;
    {
        std::ofstream output(directory.path / "ud2", std::ios::binary);
        output.write(reinterpret_cast<const char*>(bytes.data()), bytes.size());
    }
    auto opened = execute(tool, {{"operation", "open"}, {"path", "ud2"}, {"options", {{"personality", "linux"}}}});
    const auto id = opened.at("process_id");
    auto run = execute(tool, {{"operation", "run"}, {"process_id", id}});
    check(run["run"]["reason"] == "failed", "a ud2 guest did not fail");
    const auto& signal = run["state"]["signal"];
    check(signal["number"] == 4 && signal["name"] == "SIGILL", "the signal is not named");
    check(signal["code"] == 2 && signal["code_name"] == "ILL_ILLOPN", "the signal's code is not named");
    check(signal["pc"] == "0x401000", "the signal's pc differs");
    check(run["state"]["diagnostic"].get<std::string>().rfind("killed by SIGILL", 0) == 0,
          "the diagnostic does not lead with the signal");
    execute(tool, {{"operation", "close"}, {"process_id", id}});
}

/// The seed takes every unsigned 64-bit value, although its schema declares no
/// maximum: a bound of 2^64 - 1 there made xAI refuse every request carrying
/// the tool list. RAX refuses anything outside that range at open. The literals
/// are parsed so each value has the JSON type a model's text would give it.
static void seed_domain() {
    TempDirectory directory;
    auto tool = assist::tools::create_emulate_process_tool(assist::path_to_utf8(directory.path));
    const auto& abi = process_elf_fixture::abis[0];
    const auto bytes = process_elf_fixture::image(abi);
    {
        std::ofstream output(directory.path / abi.name, std::ios::binary);
        output.write(reinterpret_cast<const char*>(bytes.data()), bytes.size());
    }
    const auto open = [&](const char* options) {
        return tool.executor({{"operation", "open"}, {"path", abi.name}, {"options", Json::parse(options)}});
    };

    const auto seed = tool.input_schema.at("properties").at("options").at("properties").at("seed");
    check(seed.at("minimum") == 0 && !seed.contains("maximum"), "the seed schema declares a maximum");

    const auto widest = open(R"({"personality":"linux","seed":18446744073709551615})");
    check(widest.success && widest.structured_data, "the largest unsigned 64-bit seed was refused");
    execute(tool, {{"operation", "close"}, {"process_id", widest.structured_data->at("process_id")}});

    for (const char* options :
         {R"({"personality":"linux","seed":18446744073709551616})", R"({"personality":"linux","seed":-1})"}) {
        const auto refused = open(options);
        check(!refused.success && refused.error.value_or("").find("seed") != std::string::npos,
              "a seed outside the unsigned 64-bit range was not refused by name");
    }
    check(execute(tool, {{"operation", "list"}})["processes"].empty(), "a seed probe leaked a session");
}

static void darwin_processes() {
    TempDirectory directory;
    auto tool = assist::tools::create_emulate_process_tool(assist::path_to_utf8(directory.path));
    const auto& schema = tool.input_schema.at("properties").at("options").at("properties");
    check(schema.at("personality").at("enum") == Json::array({"windows", "linux", "darwin"}),
          "Darwin personality missing from discovery");
    check(schema.at("architecture").at("enum") == Json::array({"x86_64", "aarch64"}),
          "Mach-O architecture selection missing from discovery");
    for (const auto& abi : process_macho_fixture::abis) {
        const auto bytes = process_macho_fixture::image(abi);
        const auto file = directory.path / abi.name;
        {
            std::ofstream output(file, std::ios::binary);
            output.write(reinterpret_cast<const char*>(bytes.data()), bytes.size());
        }
        auto opened =
            execute(tool, {{"operation", "open"}, {"path", abi.name}, {"options", {{"personality", "darwin"}}}});
        const auto id = opened.at("process_id");
        check(opened["state"]["personality"] == "darwin", "Mach-O open summary failed");
        check(opened["state"]["loaded_program"]["entry"] == "0x100001000", "Mach-O program entry missing");
        check(!opened["state"].contains("modules_count"), "Mach-O invented module count");
        auto threads = execute(tool, {{"operation", "inspect"}, {"process_id", id}, {"section", "threads"}});
        const auto tid = threads["items"][0]["id"];
        auto context = execute(tool, {{"operation", "read_context"}, {"process_id", id}, {"thread_id", tid}});
        check(context["context_format"] == "darwin_thread_state64", "Mach-O context mislabeled");
        check(context["context_flavor"] == abi.flavor, "Mach context flavor missing");
        check(context["bytes"] == abi.context_bytes, "Mach-O context truncated");
        check(context["architecture"] == abi.name, "Mach-O context architecture missing");
        execute(tool, {{"operation", "write_context"},
                       {"process_id", id},
                       {"thread_id", tid},
                       {"data_hex", context["data_hex"]}});
        const auto missing = tool.executor({{"operation", "inspect"}, {"process_id", id}, {"section", "modules"}});
        check(!missing.success && missing.error.value_or("").find("unavailable for darwin") != std::string::npos,
              "Mach-O module limit not explicit");
        auto mappings =
            execute(tool, {{"operation", "inspect"}, {"process_id", id}, {"section", "mappings"}, {"limit", 1}});
        check(mappings["items"].size() == 1 && mappings["truncated"] == true, "Mach-O mapping page missing");
        auto run = execute(tool, {{"operation", "run"}, {"process_id", id}});
        check(run["run"]["reason"] == "exited" && run["run"]["exit_code"] == 37, "Mach-O tool execution failed");
        check(execute(tool, {{"operation", "list"}})["processes"].size() == 1, "Mach-O omitted from list");
        execute(tool, {{"operation", "close"}, {"process_id", id}});
    }
    const auto fat = process_macho_fixture::fat_image();
    {
        std::ofstream output(directory.path / "fat", std::ios::binary);
        output.write(reinterpret_cast<const char*>(fat.data()), fat.size());
    }
    for (const auto& abi : process_macho_fixture::abis) {
        auto opened = execute(tool, {{"operation", "open"},
                                     {"path", "fat"},
                                     {"options", {{"personality", "darwin"}, {"architecture", abi.name}}}});
        check(opened["state"]["architecture"] == abi.name, "tool lost Mach-O architecture option");
        execute(tool, {{"operation", "close"}, {"process_id", opened.at("process_id")}});
    }
    check(!tool.executor({{"operation", "open"},
                          {"path", "fat"},
                          {"options", {{"personality", "darwin"}, {"architecture", "arm"}}}})
               .success,
          "unsupported Mach-O architecture accepted");
    check(execute(tool, {{"operation", "list"}})["processes"].empty(), "Mach-O session leaked");
}

static void native_runtime_processes() {
    TempDirectory directory;
    auto tool = assist::tools::create_emulate_process_tool(assist::path_to_utf8(directory.path));
#if defined(__linux__) || defined(__APPLE__)
#if defined(__linux__)
    const char* personality = "linux";
    const auto program = std::filesystem::path("/bin/true");
#else
    const char* personality = "darwin";
    const auto program = std::filesystem::path("/usr/bin/true");
#endif
    std::filesystem::copy_file(program, directory.path / "program");
    auto opened = execute(tool, {{"operation", "open"}, {"path", "program"},
                                 {"options", {{"personality", personality}, {"native_runtime", true},
                                              {"memory_bytes", 256U << 20}, {"slice_instructions", 4096}}}});
    check(opened["dependencies"] == 0, "native process unexpectedly requires supplied dependencies");
    check(opened["state"]["capabilities"]["native_runtime"] == true, "selected runtime is not reported");
    check(opened["state"]["capabilities"]["host_filesystem"] == false, "native grant exposed general files");
    check(opened["state"]["capabilities"]["host_services"] == false, "native grant exposed host services");
    auto run = execute(tool, {{"operation", "run"}, {"process_id", opened["process_id"]},
                              {"max_turns", 1000000}, {"timeout_ms", 30000}});
    std::fprintf(stderr, "native-diagnostic %s\n", run.dump().c_str());
    check(run["run"]["reason"] == "exited" && run["run"]["exit_code"] == 0,
          "installed runtime process failed through Assist tool");
    execute(tool, {{"operation", "close"}, {"process_id", opened["process_id"]}});
#elif defined(_WIN32)
    // Minimal PE32+ native-subsystem application, no DLL imports. Native
    // NTDLL selection is observable at open; its checked NT leaves are covered
    // by the C API native-runtime test on this same Windows host.
    std::vector<std::uint8_t> image(8192);
    const auto put = [&](std::size_t at, std::uint64_t value, unsigned size) {
        for (unsigned i = 0; i < size; ++i) image.at(at + i) = std::uint8_t(value >> (8 * i));
    };
    put(0, 0x5a4d, 2); put(0x3c, 0x80, 4); put(0x80, 0x4550, 4);
#if defined(_M_ARM64) || defined(__aarch64__)
    put(0x84, 0xaa64, 2);
#else
    put(0x84, 0x8664, 2);
#endif
    put(0x86, 1, 2); put(0x94, 240, 2); put(0x96, 0x22, 2);
    const std::size_t optional = 0x98;
    put(optional, 0x20b, 2); put(optional + 16, 0x1000, 4);
    put(optional + 24, 0x140000000ULL, 8); put(optional + 32, 4096, 4);
    put(optional + 36, 4096, 4); put(optional + 56, 8192, 4); put(optional + 60, 4096, 4);
    put(optional + 68, 1, 2); put(optional + 72, 1048576, 8); put(optional + 80, 4096, 8);
    put(optional + 88, 1048576, 8); put(optional + 96, 4096, 8); put(optional + 108, 16, 4);
    const std::size_t section = optional + 240;
    std::copy_n(".text", 5, image.begin() + section);
    put(section + 8, 4096, 4); put(section + 12, 4096, 4);
    put(section + 16, 4096, 4); put(section + 20, 4096, 4); put(section + 36, 0x60000020, 4);
    {
        std::ofstream output(directory.path / "program", std::ios::binary);
        output.write(reinterpret_cast<const char*>(image.data()), image.size());
    }
    auto opened = execute(tool, {{"operation", "open"}, {"path", "program"},
                                 {"options", {{"native_runtime", true}, {"memory_bytes", 256U << 20}}}});
    check(opened["state"]["capabilities"]["native_runtime"] == true, "Windows native runtime not selected");
    check(opened["state"]["capabilities"]["host_filesystem"] == false, "Windows native grant exposed files");
    auto modules = execute(tool, {{"operation", "inspect"}, {"process_id", opened["process_id"]},
                                  {"section", "modules"}, {"limit", 32}});
    bool installed = false;
    for (const auto& module : modules["items"]) {
        if (module["name"] == "ntdll.dll") {
            auto path = module["path"].get<std::string>();
            std::transform(path.begin(), path.end(), path.begin(), [](unsigned char c) { return std::tolower(c); });
            installed = path.find("system32") != std::string::npos;
        }
    }
    check(installed, "installed NTDLL provenance is absent from tool inspection");
    execute(tool, {{"operation", "close"}, {"process_id", opened["process_id"]}});
#endif
    check(execute(tool, {{"operation", "list"}})["processes"].empty(), "native tool process leaked");
}

int main() {
    try {
        const auto root = std::filesystem::path(ASSIST_RAX_PROCESS_FIXTURES);
        auto tool = assist::tools::create_emulate_process_tool(assist::path_to_utf8(root));
        check(!tool.runtime_unavailability(), "runtime is unavailable");
        check(tool.runs_off_main_thread && !tool.modifies_database && !tool.parallel_safe,
              "process scheduling or mutation metadata differs");
        check(assist::render::classify_tool("emulate_process") == assist::render::ToolFamily::EmulateProcess,
              "process tool classification missing");
        check(tool.call_permission_group({{"operation", "open"}, {"path", "bin/x64/smoke.exe"}}) == "read_files",
              "in-root executable permission differs");
        const auto& native_option = tool.input_schema["properties"]["options"]["properties"]["native_runtime"];
        check(native_option["type"] == "boolean" && native_option["default"] == false,
              "native runtime grant is missing from discovery");
        for (const auto* personality : {"windows", "linux", "darwin"}) {
            check(tool.call_permission_group({{"operation", "open"}, {"path", "bin/x64/smoke.exe"},
                                               {"options", {{"personality", personality}, {"native_runtime", true}}}})
                      == "read_files_outside", "installed runtime read bypassed outside-file permission");
            check(tool.call_permission_group({{"operation", "open"}, {"path", "bin/x64/smoke.exe"},
                                               {"options", {{"personality", personality}, {"native_runtime", false}}}})
                      == "read_files", "supplied-only mode unnecessarily escalated permission");
        }
        check(tool.call_permission_group({{"operation", "run"}, {"options", {{"native_runtime", true}}}})
                  == "read_files", "a refused non-open option changed permission");
        for (const auto& malformed : {Json(nullptr), Json(1), Json("true"), Json::array(), Json::object()}) {
            const auto rejected = tool.executor({{"operation", "open"}, {"path", "missing/never-read.exe"},
                                                   {"options", {{"native_runtime", malformed}}}});
            check(!rejected.success && rejected.error.value_or("") == "native_runtime must be a boolean",
                  "malformed native option was not refused before image reads");
        }
        const auto outside = assist::path_to_utf8(root.parent_path() / "outside.dll");
        check(tool.call_permission_group({{"operation", "open"}, {"path", outside}}) == "read_files_outside",
              "outside executable bypassed permission boundary");
        check(tool.call_permission_group(
                  {{"operation", "open"},
                   {"path", "bin/x64/smoke.exe"},
                   {"dependencies", Json::array({{{"path", outside}, {"guest_path", "C:\\outside.dll"}}})}}) ==
                  "read_files_outside",
              "outside DLL bypassed permission boundary");
        for (const auto* arch : {"x86", "x64", "arm64"}) {
            auto opened = execute(tool, {{"operation", "open"},
                                         {"path", std::string("bin/") + arch + "/smoke.exe"},
                                         {"options", {{"memory_bytes", 64U << 20}}}});
            const auto id = opened.at("process_id");
            check(opened["state"]["architecture"] == arch, "wrong guest architecture");
            check(opened["state"]["capabilities"]["host_filesystem"] == false, "guest host file access exposed");
            check(opened["state"]["capabilities"]["native_runtime"] == false, "default runtime selection changed");
            auto threads =
                execute(tool, {{"operation", "inspect"}, {"process_id", id}, {"section", "threads"}, {"limit", 1}});
            check(threads["items"].size() == 1, "thread inspection missing");
            const auto tid = threads["items"][0]["id"];
            auto context = execute(tool, {{"operation", "read_context"}, {"process_id", id}, {"thread_id", tid}});
            execute(tool, {{"operation", "write_context"},
                           {"process_id", id},
                           {"thread_id", tid},
                           {"data_hex", context.at("data_hex")}});
            auto modules =
                execute(tool, {{"operation", "inspect"}, {"process_id", id}, {"section", "modules"}, {"limit", 1}});
            check(modules["truncated"] == true && modules["next_offset"] == 1, "module pagination missing");
            const auto base = modules["items"][0]["base"];
            auto read =
                execute(tool, {{"operation", "read_memory"}, {"process_id", id}, {"address", base}, {"size", 2}});
            check(read["data_hex"] == "4d5a", "binary memory response corrupted");
            check(!tool.executor(
                           {{"operation", "write_memory"}, {"process_id", id}, {"address", base}, {"data_hex", "abcd"}})
                       .success,
                  "read-only mapping modified");
            const auto misplaced =
                tool.executor({{"operation", "run"}, {"process_id", id}, {"data_hex", "abcd"}});
            check(!misplaced.success
                      && misplaced.error.value_or("").find("feed_stdin") != std::string::npos,
                  "a field run does not take is refused naming the operation that takes it");
            const auto limit =
                tool.executor({{"operation", "run"}, {"process_id", id}, {"limit", 32}});
            check(!limit.success
                      && limit.error.value_or("")
                             == "run does not take limit (inspect does); run accepts process_id, "
                                "max_turns, timeout_ms",
                  "limit sent to run names inspect and what run accepts");
            const auto nulls = tool.executor({{"operation", "inspect"},
                                              {"process_id", id},
                                              {"section", nullptr},
                                              {"limit", nullptr},
                                              {"max_turns", nullptr}});
            check(nulls.success, "null fields are omitted fields, whichever operation owns them");
            execute(tool, {{"operation", "cancel"}, {"process_id", id}});
            auto run = execute(tool, {{"operation", "run"}, {"process_id", id}});
            check(run["run"]["reason"] == "cancelled", "persistent cancellation missing");
            execute(tool, {{"operation", "cancel"}, {"process_id", id}, {"cancelled", false}});
            run = execute(tool, {{"operation", "run"}, {"process_id", id}, {"max_turns", 100000}});
            check(run["run"]["reason"] == "exited" && run["run"]["exit_code"] == 0, "guest did not exit");
            check(execute(tool, {{"operation", "close"}, {"process_id", id}})["closed"] == true, "close missing");
            check(!tool.executor({{"operation", "inspect"}, {"process_id", id}}).success, "closed process still live");
        }
        // Field ownership is judged before the process is looked up, so a
        // misplaced field on an unknown process is reported as the field.
        const auto unknown = tool.executor(
            {{"operation", "inspect"}, {"process_id", "process_999"}, {"max_turns", 10}});
        check(!unknown.success
                  && unknown.error.value_or("").find("inspect does not take max_turns") == 0,
              "a misplaced field is reported before the unknown process_id");
        // And a malformed dependency record before any image is read.
        const auto record = tool.executor(
            {{"operation", "open"},
             {"path", "missing/never-read.exe"},
             {"dependencies",
              Json::array({{{"path", "k.dll"}, {"guest_path", "C:\\k.dll"}, {"name", "k"}}})}});
        check(!record.success
                  && record.error.value_or("")
                         == "a dependency record does not take name; dependency records "
                            "accept path, guest_path",
              "a dependency record with an extra key is refused before any file is read");
        auto opened = execute(tool, {{"operation", "open"}, {"path", "crt_stdio/bin/x64/msvcrt/streams.exe"}});
        const auto id = opened.at("process_id");
        execute(tool, {{"operation", "run"}, {"process_id", id}, {"max_turns", 100000}});
        check(!tool.executor({{"operation", "read_output"}, {"process_id", id}, {"stream", "invalid"}}).success,
              "invalid output stream accepted");
        auto output = execute(tool, {{"operation", "read_output"}, {"process_id", id}, {"stream", "stdout"}});
        check(output["data_hex"] == "737464696f2d6f75740d0a", "stdout bytes altered or prematurely drained");
        check(execute(tool, {{"operation", "read_output"}, {"process_id", id}, {"stream", "stdout"}})["bytes"] == 0,
              "stdout drain replayed bytes");
        auto independent = assist::tools::create_emulate_process_tool(assist::path_to_utf8(root));
        check(!independent.executor({{"operation", "inspect"}, {"process_id", id}}).success,
              "process escaped its owning tool host");
        Json dependencies = Json::array();
        for (const auto* name : {"root.dll", "leaf.dll", "observer.dll"})
            dependencies.push_back(
                {{"path", std::string("lifecycle/bin/x64/") + name}, {"guest_path", std::string("C:\\app\\") + name}});
        auto imported = execute(independent, {{"operation", "open"},
                                              {"path", "lifecycle/bin/x64/startup.exe"},
                                              {"dependencies", dependencies},
                                              {"options", {{"guest_path", "C:\\app\\program.exe"}}}});
        auto imported_run =
            execute(independent, {{"operation", "run"}, {"process_id", imported["process_id"]}, {"max_turns", 100000}});
        check(imported_run["run"]["reason"] == "exited" && imported_run["run"]["exit_code"] == 0,
              "tool supplied-DLL initialization failed");
        execute(independent, {{"operation", "close"}, {"process_id", imported["process_id"]}});
        {
            assist::tools::ScopedToolCancellation cancelled([] { return true; });
            check(!independent.executor({{"operation", "open"}, {"path", "bin/x64/smoke.exe"}}).success,
                  "cancelled open created a session");
        }
        check(execute(independent, {{"operation", "list"}})["processes"].empty(), "cancelled open leaked a session");
        for (int i = 0; i < 3; ++i)
            execute(tool, {{"operation", "open"}, {"path", "bin/x64/smoke.exe"}});
        check(!tool.executor({{"operation", "open"}, {"path", "bin/x64/smoke.exe"}}).success,
              "session capacity was not enforced");
        auto live = execute(tool, {{"operation", "list"}});
        check(live["processes"].size() == 4, "session list lost a process");
        for (const auto& row : live["processes"])
            execute(tool, {{"operation", "close"}, {"process_id", row["process_id"]}});
        check(execute(tool, {{"operation", "list"}})["processes"].empty(), "processes survived close");
        linux_processes();
        refused_accesses();
        signal_death();
        seed_domain();
        darwin_processes();
        native_runtime_processes();
        std::printf("process tool: %u checks passed\n", checks);
        return 0;
    } catch (const std::exception& error) {
        std::fprintf(stderr, "process tool failed after %u checks: %s\n", checks, error.what());
        return 1;
    }
}
