// Native helpers for Rach, written in C++17.
//
// Linked into the Rust interpreter at build time via build.rs / cc-rs.
// Exposed to Rach scripts via the `native_*` stdlib commands.

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

extern "C" {

/// In-place sort of a comma-separated list of signed integers.
/// `inout` must be a NUL-terminated buffer with room for the result
/// (sorted output is no longer than the input). Returns 0 on success,
/// -1 on parse failure. Empty input yields empty output.
int32_t rach_sort_csv_ints(char *inout) {
    if (!inout) return -1;
    std::string s(inout);
    std::vector<int64_t> nums;
    nums.reserve(16);

    size_t i = 0;
    while (i < s.size()) {
        while (i < s.size() && (s[i] == ' ' || s[i] == ',')) i++;
        if (i >= s.size()) break;
        size_t start = i;
        if (s[i] == '-' || s[i] == '+') i++;
        while (i < s.size() && s[i] >= '0' && s[i] <= '9') i++;
        if (i == start) return -1;
        try {
            nums.push_back(std::stoll(s.substr(start, i - start)));
        } catch (...) {
            return -1;
        }
    }

    std::sort(nums.begin(), nums.end());

    std::string out;
    out.reserve(nums.size() * 4);
    for (size_t k = 0; k < nums.size(); k++) {
        if (k) out.push_back(',');
        out += std::to_string(nums[k]);
    }
    std::strcpy(inout, out.c_str());
    return 0;
}

/// Reverse a NUL-terminated UTF-8 byte string in place. Bytes only — combining
/// characters and multi-byte sequences are NOT preserved, callers should use
/// only ASCII for predictable results.
void rach_reverse_bytes(char *s) {
    if (!s) return;
    size_t n = std::strlen(s);
    std::reverse(s, s + n);
}

} // extern "C"
