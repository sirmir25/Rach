// Rach installer in C++17. Cross-platform: POSIX + Windows.
//
// Downloads a pre-built binary from GitHub Releases — no Rust toolchain
// required. Falls back to `cargo build` only when RACH_FROM_SOURCE=1.
//
// Build:
//   c++ -std=c++17 installers/install.cpp -o /tmp/rach-install
//   cl /std:c++17 installers\install.cpp /Fe:rach-install.exe
//
// Run:
//   /tmp/rach-install                    # default install dir
//   /tmp/rach-install /opt               # custom (POSIX)
//   rach-install.exe "C:\Tools\rach"     # custom (Windows)
//
// Env:
//   RACH_VERSION       pin a release tag (default: latest)
//   RACH_REPO          override repo (default sirmir25/Rach)
//   RACH_FROM_SOURCE=1 force `cargo build` instead of downloading

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <iostream>
#include <optional>
#include <string>

#if defined(_WIN32)
#  include <windows.h>
#else
#  include <sys/utsname.h>
#endif

namespace fs = std::filesystem;

#if defined(_WIN32)
constexpr bool kIsWin = true;
constexpr const char* kBinName = "rach.exe";
constexpr const char* kArchiveExt = "zip";
#else
constexpr bool kIsWin = false;
constexpr const char* kBinName = "rach";
constexpr const char* kArchiveExt = "tar.gz";
#endif

static int run(const std::string& cmd) {
    std::cerr << "==> " << cmd << "\n";
    return std::system(cmd.c_str());
}

[[noreturn]] static void die(const std::string& msg, int code = 1) {
    std::cerr << "xx " << msg << "\n";
    std::exit(code);
}

static std::string envor(const char* name, const std::string& fallback) {
    const char* v = std::getenv(name);
    return (v && *v) ? std::string(v) : fallback;
}

static std::optional<std::string> detect_target() {
#if defined(_WIN32)
    // No native ARM64 build yet; the x86_64 binary runs under emulation.
    return std::string("x86_64-pc-windows-msvc");
#else
    utsname u{};
    if (uname(&u) != 0) return std::nullopt;
    std::string sys = u.sysname, mach = u.machine;
    std::string os;
    if (sys == "Linux") os = "unknown-linux-musl";
    else if (sys == "Darwin") os = "apple-darwin";
    else return std::nullopt;
    std::string arch;
    if (mach == "x86_64" || mach == "amd64") arch = "x86_64";
    else if (mach == "arm64" || mach == "aarch64") arch = "aarch64";
    else return std::nullopt;
    return arch + "-" + os;
#endif
}

static int install_binary(const fs::path& src, const fs::path& install_dir) {
    std::error_code ec;
    fs::create_directories(install_dir, ec);
    if (ec && !fs::exists(install_dir)) {
        std::cerr << "!! cannot create " << install_dir << ": " << ec.message() << "\n";
        return 1;
    }
    fs::path dst = install_dir / kBinName;
    std::cerr << "==> installing to " << dst << "\n";
    fs::copy_file(src, dst, fs::copy_options::overwrite_existing, ec);
    if (ec) {
#if !defined(_WIN32)
        std::string sudo_cmd = "sudo install -m 0755 '" + src.string() + "' '" + dst.string() + "'";
        if (run(sudo_cmd) == 0) return 0;
#endif
        std::cerr << "!! copy failed: " << ec.message() << "\n";
        return 1;
    }
#if !defined(_WIN32)
    fs::permissions(dst,
        fs::perms::owner_all | fs::perms::group_read | fs::perms::group_exec
            | fs::perms::others_read | fs::perms::others_exec,
        fs::perm_options::replace, ec);
#endif
    return 0;
}

static int install_from_release(const fs::path& install_dir) {
    auto target = detect_target();
    if (!target) {
        std::cerr << "!! could not detect platform\n";
        return 1;
    }
    std::string repo    = envor("RACH_REPO", "sirmir25/Rach");
    std::string version = envor("RACH_VERSION", "latest");

    std::string url = (version == "latest")
        ? "https://github.com/" + repo + "/releases/latest/download/rach-" + *target + "." + kArchiveExt
        : "https://github.com/" + repo + "/releases/download/" + version + "/rach-" + *target + "." + kArchiveExt;

    std::error_code ec;
    fs::path tmp = fs::temp_directory_path(ec) / ("rach-install-" + std::to_string((long long)std::rand()));
    fs::create_directories(tmp, ec);
    if (ec) { std::cerr << "!! mkdir tmp failed: " << ec.message() << "\n"; return 1; }

#if defined(_WIN32)
    fs::path archive = tmp / "rach.zip";
    std::string dl =
        "powershell -NoProfile -ExecutionPolicy Bypass -Command "
        "\"[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; "
        "Invoke-WebRequest -UseBasicParsing -Uri '" + url + "' -OutFile '" + archive.string() + "'\"";
    if (run(dl) != 0) { std::cerr << "!! download failed: " << url << "\n"; return 1; }
    std::string ex =
        "powershell -NoProfile -Command "
        "\"Expand-Archive -Path '" + archive.string() + "' -DestinationPath '" + tmp.string() + "' -Force\"";
    if (run(ex) != 0) { std::cerr << "!! extract failed\n"; return 1; }
#else
    fs::path archive = tmp / "rach.tar.gz";
    std::string dl =
        "curl -fsSL --proto '=https' --tlsv1.2 -o '" + archive.string() + "' '" + url + "' "
        "|| wget -qO '" + archive.string() + "' '" + url + "'";
    if (run(dl) != 0) { std::cerr << "!! download failed: " << url << "\n"; return 1; }
    std::string ex = "tar -C '" + tmp.string() + "' -xzf '" + archive.string() + "'";
    if (run(ex) != 0) { std::cerr << "!! extract failed\n"; return 1; }
#endif

    fs::path src;
    for (auto& p : fs::recursive_directory_iterator(tmp, ec)) {
        if (p.is_regular_file() && p.path().filename() == kBinName) {
            src = p.path();
            break;
        }
    }
    if (src.empty()) { std::cerr << "!! archive missing " << kBinName << "\n"; return 1; }

    int rc = install_binary(src, install_dir);
    fs::remove_all(tmp, ec);
    return rc;
}

static int install_from_source(const fs::path& install_dir) {
    const char* check = kIsWin ? "where cargo >nul 2>&1" : "command -v cargo >/dev/null 2>&1";
    if (run(check) != 0) {
        die("no pre-built binary available and cargo not on PATH (install Rust from https://rustup.rs)");
    }
    if (run("cargo build --release") != 0) {
        die("build failed");
    }
    fs::path src = fs::path("target") / "release" / kBinName;
    if (!fs::exists(src)) {
        die("build did not produce " + src.string());
    }
    return install_binary(src, install_dir);
}

int main(int argc, char** argv) {
    fs::path install_dir;
    if (argc > 1) {
        install_dir = argv[1];
    } else if (kIsWin) {
        const char* base = std::getenv("LOCALAPPDATA");
        install_dir = fs::path(base ? base : "C:\\Program Files") / "Programs" / "rach";
    } else {
        install_dir = "/usr/local/bin";
    }

    const char* from_source = std::getenv("RACH_FROM_SOURCE");
    int rc;
    if (from_source && std::string(from_source) == "1") {
        rc = install_from_source(install_dir);
    } else {
        rc = install_from_release(install_dir);
        if (rc != 0) {
            std::cerr << "!! falling back to source build\n";
            rc = install_from_source(install_dir);
        }
    }
    if (rc != 0) return rc;

    fs::path dst = install_dir / kBinName;
    run("\"" + dst.string() + "\" version");
    std::cerr << "==> installed. Try:  rach examples" << (kIsWin ? "\\" : "/") << "hello.rach\n";
    return 0;
}
