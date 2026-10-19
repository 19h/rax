#define WIN32_LEAN_AND_MEAN
#include <array>
#include <cstdint>
#include <cstdio>
#include <windows.h>

#include <sddl.h>
int main(int argc, char **argv) {
  const uint64_t authorities[] = {0,
                                  1,
                                  5,
                                  0xFFFFFFFFULL,
                                  0x100000000ULL,
                                  0x010203040506ULL,
                                  0x123456789ABCULL,
                                  0xFFFFFFFFFFFFULL};
  unsigned cases = 0;
  for (unsigned a = 0; a < 8; ++a)
    for (unsigned n = 0; n <= 15; ++n) {
      alignas(void *) std::array<BYTE, 68> raw{};
      raw[0] = 1;
      raw[1] = BYTE(n);
      for (unsigned i = 0; i < 6; ++i)
        raw[2 + i] = BYTE(authorities[a] >> (8 * (5 - i)));
      for (unsigned i = 0; i < n; ++i) {
        uint32_t v = i % 4 == 0   ? 0
                     : i % 4 == 1 ? 0xFFFFFFFFU
                     : i % 4 == 2 ? 0x80000000U
                                  : 0x12345678U;
        for (unsigned b = 0; b < 4; ++b)
          raw[8 + 4 * i + b] = BYTE(v >> (8 * b));
      }
      LPWSTR text = nullptr;
      SetLastError(0x7E11ABCD);
      BOOL ok = ConvertSidToStringSidW(raw.data(), &text);
      DWORD error = GetLastError();
      printf(
          "SID profile=%s authority=%u count=%u ok=%u error=%08lX text=%ls\n",
          argc > 1 ? argv[1] : "unknown", a, n, unsigned(ok), error,
          text ? text : L"<null>");
      if (!ok || !text || LocalFree(text) != nullptr)
        return 2;
      ++cases;
    }
  printf("SUMMARY cases=%u\n", cases);
  return cases == 128 ? 0 : 3;
}
