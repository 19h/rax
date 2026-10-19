#include <assist/emulation/rax/rax_process.hpp>
#include <chrono>
#include <cstdio>
#include <fstream>
#include <stdexcept>
using namespace assist::emulation::rax;
static std::string hex(const std::vector<std::uint8_t> &v) {
  static constexpr char digits[] = "0123456789abcdef";
  std::string out;
  for (auto byte : v) {
    out.push_back(digits[byte >> 4]);
    out.push_back(digits[byte & 15]);
  }
  return out;
}
int main(int argc, char **argv) {
  RaxApi api{};
  api.version = &rax_version;
  api.strerror = &rax_strerror;
  api.process_open_image = &rax_process_open_image;
  api.process_close = &rax_process_close;
  api.process_run = &rax_process_run;
  api.process_set_cancelled = &rax_process_set_cancelled;
  api.process_info_json = &rax_process_info_json;
  api.process_last_error = &rax_process_last_error;
  api.process_mem_read = &rax_process_mem_read;
  api.process_mem_write = &rax_process_mem_write;
  api.process_context_read = &rax_process_context_read;
  api.process_context_write = &rax_process_context_write;
  api.process_stdin_feed = &rax_process_stdin_feed;
  api.process_output_read = &rax_process_output_read;
  for (int index = 1; index < argc; ++index) {
    const char *path = argv[index];
    std::ifstream input(path, std::ios::binary);
    if (!input)
      throw std::runtime_error("missing ordinary input");
    std::vector<std::uint8_t> image(std::istreambuf_iterator<char>(input), {});
    RaxProcess process(&api, image,
                       {{"native_runtime", true},
                        {"guest_path", "C:\\app\\probe.exe"},
                        {"memory_bytes", 268435456},
                        {"slice_instructions", 4096}});
    auto opened = process.info();
    std::printf("api-open program=%s modules=%zu\n", path,
                opened["modules"].size());
    std::fflush(stdout);
    std::uint64_t turns = 0, elapsed = 0;
    rax_process_result result{};
    bool terminal = false;
    for (unsigned attempt = 0; attempt < 40; ++attempt) {
      result = process.run(1000000, 15000);
      turns += result.turns_started;
      elapsed += result.elapsed_us;
      std::printf("api-progress program=%s attempt=%u reason=%u turns=%llu "
                  "elapsed-us=%llu\n",
                  path, attempt, result.reason,
                  static_cast<unsigned long long>(turns),
                  static_cast<unsigned long long>(elapsed));
      std::fflush(stdout);
      if (result.reason != RAX_PROCESS_BUDGET &&
          result.reason != RAX_PROCESS_TIMEOUT) {
        terminal = true;
        break;
      }
    }
    if (!terminal)
      throw std::runtime_error(
          "ordinary C API run did not terminate within 40 bounded calls");
    auto state = process.info();
    auto drain = [&](std::uint32_t stream) {
      std::vector<std::uint8_t> bytes;
      for (;;) {
        auto part = process.output(stream, 4096);
        if (part.empty())
          break;
        bytes.insert(bytes.end(), part.begin(), part.end());
      }
      return hex(bytes);
    };
    nlohmann::json record = {
        {"program", path},
        {"reason", result.reason},
        {"exit-code", result.exit_code},
        {"turns-started", turns},
        {"elapsed-us", elapsed},
        {"diagnostic", state.value("diagnostic", nlohmann::json())},
        {"stdout-hex", drain(RAX_PROCESS_STDOUT)},
        {"stderr-hex", drain(RAX_PROCESS_STDERR)},
        {"native-runtime", true},
        {"slice-instructions", 4096}};
    std::printf("api-result %s\n", record.dump().c_str());
    std::fflush(stdout);
  }
}
