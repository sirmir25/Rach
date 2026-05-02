// Rach installer in C++17. Cross-platform: POSIX + Windows.
//
// Build:
//   c++ -std=c++17 installers/install.cpp -o /tmp/rach-install
//   cl /std:c++17 installers\install.cpp /Fe:rach-install.exe
//
// Run:
//   /tmp/rach-install                    # default prefix
//   /tmp/rach-install /opt               # custom (POSIX)
//   rach-install.exe "C:\Tools\rach"     # custom (Windows)

#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <iostream>
#include <string>
#include <vector>

namespace fs = std::filesystem;

#if defined(_WIN32)
constexpr bool kIsWin = true;
constexpr const char* kBinName = "rach.exe";
constexpr const char* kCargoCheck = "where cargo >nul 2>&1";
#else
constexpr bool kIsWin = false;
constexpr const char* kBinName = "rach";
constexpr const char* kCargoCheck = "command -v cargo >/dev/null 2>&1";
#endif

static int run(const std::string& cmd) {
    std::cerr << "==> " << cmd << "\n";
    return std::system(cmd.c_str());
}

[[noreturn]] static void die(const std::string& msg, int code = 1) {
    std::cerr << "xx " << msg << "\n";
    std::exit(code);
}

int main(int argc, char** argv) {
    if (run(kCargoCheck) != 0) {
        die("cargo not found in PATH. Install Rust from https://rustup.rs");
    }

    if (run("cargo build --release") != 0) {
        die("build failed");
    }

    fs::path src_bin = fs::path("target") / "release" / kBinName;
    if (!fs::exists(src_bin)) {
        die("build did not produce " + src_bin.string());
    }

    fs::path install_dir;
    if (argc > 1) {
        install_dir = argv[1];
    } else if (kIsWin) {
        const char* pf = std::getenv("ProgramFiles");
        install_dir = fs::path(pf ? pf : "C:\\Program Files") / "rach";
    } else {
        install_dir = "/usr/local/bin";
    }

    std::error_code ec;
    fs::create_directories(install_dir, ec);
    if (ec && !fs::exists(install_dir)) {
        die("cannot create " + install_dir.string() + ": " + ec.message());
    }

    fs::path dst_bin = install_dir / kBinName;
    std::cerr << "==> installing to " << dst_bin << "\n";
    fs::copy_file(src_bin, dst_bin, fs::copy_options::overwrite_existing, ec);
    if (ec) {
        die("copy failed: " + ec.message() + " — re-run with elevated privileges");
    }
#if !defined(_WIN32)
    fs::permissions(dst_bin,
        fs::perms::owner_all | fs::perms::group_read | fs::perms::group_exec
            | fs::perms::others_read | fs::perms::others_exec,
        fs::perm_options::replace, ec);
#endif

    run("\"" + dst_bin.string() + "\" version");
    std::cerr << "==> installed. Try:  rach examples" << (kIsWin ? "\\" : "/") << "hello.rach\n";
    return 0;
}
