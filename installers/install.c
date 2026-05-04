/*
 * Rach installer in pure C99. Cross-platform: POSIX + Windows.
 *
 * Downloads a pre-built binary from GitHub Releases — no Rust toolchain
 * required. Falls back to `cargo build` only if RACH_FROM_SOURCE=1.
 *
 * Build:
 *   cc installers/install.c -o /tmp/rach-install
 *   cl installers\install.c /Fe:rach-install.exe
 *
 * Run:
 *   /tmp/rach-install              # default install dir
 *   /tmp/rach-install /opt         # custom (POSIX)
 *   rach-install.exe "C:\Tools"    # custom (Windows)
 *
 * Env:
 *   RACH_VERSION       pin a release tag (default: latest)
 *   RACH_REPO          override repo (default sirmir25/Rach)
 *   RACH_FROM_SOURCE=1 force `cargo build` instead of downloading
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
  #include <direct.h>
  #include <io.h>
  #define IS_WIN 1
  #define BIN_NAME "rach.exe"
  #define PATH_SEP "\\"
  #define ARCHIVE_EXT "zip"
  #define MKDIR_P(p) _mkdir(p)
#else
  #include <sys/stat.h>
  #include <sys/utsname.h>
  #include <unistd.h>
  #define IS_WIN 0
  #define BIN_NAME "rach"
  #define PATH_SEP "/"
  #define ARCHIVE_EXT "tar.gz"
  #define MKDIR_P(p) mkdir(p, 0755)
#endif

static int run(const char *cmd) {
    fprintf(stderr, "==> %s\n", cmd);
    return system(cmd);
}

static int file_exists(const char *path) {
    FILE *f = fopen(path, "rb");
    if (!f) return 0;
    fclose(f);
    return 1;
}

static int copy_file(const char *src, const char *dst) {
    FILE *in = fopen(src, "rb");
    if (!in) return 1;
    FILE *out = fopen(dst, "wb");
    if (!out) { fclose(in); return 1; }
    char buf[8192];
    size_t n;
    while ((n = fread(buf, 1, sizeof buf, in)) > 0) {
        if (fwrite(buf, 1, n, out) != n) { fclose(in); fclose(out); return 1; }
    }
    fclose(in); fclose(out);
#if !defined(_WIN32)
    chmod(dst, 0755);
#endif
    return 0;
}

static const char *envor(const char *name, const char *fallback) {
    const char *v = getenv(name);
    return (v && *v) ? v : fallback;
}

#if !defined(_WIN32)
static int detect_target_unix(char *out, size_t n) {
    struct utsname u;
    if (uname(&u) != 0) return 1;

    const char *os = NULL;
    if (strcmp(u.sysname, "Linux") == 0)       os = "unknown-linux-musl";
    else if (strcmp(u.sysname, "Darwin") == 0) os = "apple-darwin";
    else return 1;

    const char *arch = NULL;
    if (strcmp(u.machine, "x86_64") == 0 || strcmp(u.machine, "amd64") == 0) arch = "x86_64";
    else if (strcmp(u.machine, "arm64") == 0 || strcmp(u.machine, "aarch64") == 0) arch = "aarch64";
    else return 1;

    snprintf(out, n, "%s-%s", arch, os);
    return 0;
}
#endif

static int install_from_release(const char *install_dir) {
    char target[64];
#if defined(_WIN32)
    /* No native ARM64 build yet — x86_64 build runs under emulation. */
    snprintf(target, sizeof target, "x86_64-pc-windows-msvc");
#else
    if (detect_target_unix(target, sizeof target) != 0) {
        fprintf(stderr, "!! could not detect target platform\n");
        return 1;
    }
#endif

    const char *repo    = envor("RACH_REPO", "sirmir25/Rach");
    const char *version = envor("RACH_VERSION", "latest");

    char url[512];
    if (strcmp(version, "latest") == 0) {
        snprintf(url, sizeof url,
                 "https://github.com/%s/releases/latest/download/rach-%s.%s",
                 repo, target, ARCHIVE_EXT);
    } else {
        snprintf(url, sizeof url,
                 "https://github.com/%s/releases/download/%s/rach-%s.%s",
                 repo, version, target, ARCHIVE_EXT);
    }

#if defined(_WIN32)
    const char *tmp = getenv("TEMP");
    if (!tmp) tmp = "C:\\Windows\\Temp";
    char tmpdir[1024];
    snprintf(tmpdir, sizeof tmpdir, "%s\\rach-install", tmp);
    _mkdir(tmpdir);
    char zip[1100];
    snprintf(zip, sizeof zip, "%s\\rach.zip", tmpdir);

    char cmd[2048];
    snprintf(cmd, sizeof cmd,
        "powershell -NoProfile -ExecutionPolicy Bypass -Command "
        "\"[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; "
        "Invoke-WebRequest -UseBasicParsing -Uri '%s' -OutFile '%s'\"",
        url, zip);
    if (run(cmd) != 0) { fprintf(stderr, "!! download failed: %s\n", url); return 1; }

    snprintf(cmd, sizeof cmd,
        "powershell -NoProfile -Command "
        "\"Expand-Archive -Path '%s' -DestinationPath '%s' -Force\"",
        zip, tmpdir);
    if (run(cmd) != 0) { fprintf(stderr, "!! extract failed\n"); return 1; }

    /* Locate rach.exe (it's nested inside rach-<target>/). */
    char src[1200];
    snprintf(src, sizeof src, "%s\\rach-%s\\rach.exe", tmpdir, target);
    if (!file_exists(src)) {
        snprintf(src, sizeof src, "%s\\rach.exe", tmpdir);
        if (!file_exists(src)) { fprintf(stderr, "!! archive missing rach.exe\n"); return 1; }
    }

    MKDIR_P(install_dir);
    char dst[1300];
    snprintf(dst, sizeof dst, "%s\\rach.exe", install_dir);
    fprintf(stderr, "==> installing to %s\n", dst);
    if (copy_file(src, dst) != 0) {
        fprintf(stderr, "!! copy failed — try Administrator or pick %%LOCALAPPDATA%%\\Programs\\rach\n");
        return 1;
    }
#else
    char tmpdir[] = "/tmp/rach-install-XXXXXX";
    if (!mkdtemp(tmpdir)) { fprintf(stderr, "!! mkdtemp failed\n"); return 1; }
    char tarball[1100];
    snprintf(tarball, sizeof tarball, "%s/rach.tar.gz", tmpdir);

    char cmd[2048];
    snprintf(cmd, sizeof cmd,
        "curl -fsSL --proto '=https' --tlsv1.2 -o '%s' '%s' || wget -qO '%s' '%s'",
        tarball, url, tarball, url);
    if (run(cmd) != 0) { fprintf(stderr, "!! download failed: %s\n", url); return 1; }

    snprintf(cmd, sizeof cmd, "tar -C '%s' -xzf '%s'", tmpdir, tarball);
    if (run(cmd) != 0) { fprintf(stderr, "!! extract failed\n"); return 1; }

    char src[1200];
    snprintf(src, sizeof src, "%s/rach-%s/rach", tmpdir, target);
    if (!file_exists(src)) {
        snprintf(src, sizeof src, "%s/rach", tmpdir);
        if (!file_exists(src)) { fprintf(stderr, "!! archive missing rach\n"); return 1; }
    }

    MKDIR_P(install_dir);
    char dst[1300];
    snprintf(dst, sizeof dst, "%s/rach", install_dir);
    fprintf(stderr, "==> installing to %s\n", dst);
    if (copy_file(src, dst) != 0) {
        char sudo_cmd[2700];
        snprintf(sudo_cmd, sizeof sudo_cmd,
            "sudo install -m 0755 '%s' '%s'", src, dst);
        if (run(sudo_cmd) != 0) {
            fprintf(stderr, "!! copy failed — re-run with elevated privileges\n");
            return 1;
        }
    }
#endif
    return 0;
}

static int install_from_source(const char *install_dir) {
    if (run(IS_WIN ? "where cargo >nul 2>&1" : "command -v cargo >/dev/null 2>&1") != 0) {
        fprintf(stderr, "xx no pre-built binary available and cargo not on PATH "
                        "(install Rust from https://rustup.rs)\n");
        return 1;
    }
    if (run("cargo build --release") != 0) {
        fprintf(stderr, "xx build failed\n");
        return 1;
    }
    const char *src = "target" PATH_SEP "release" PATH_SEP BIN_NAME;
    if (!file_exists(src)) {
        fprintf(stderr, "xx build did not produce %s\n", src);
        return 1;
    }
    MKDIR_P(install_dir);
    char dst[1300];
    snprintf(dst, sizeof dst, "%s%s%s", install_dir, PATH_SEP, BIN_NAME);
    fprintf(stderr, "==> installing to %s\n", dst);
    if (copy_file(src, dst) != 0) {
        fprintf(stderr, "xx copy failed — re-run with elevated privileges\n");
        return 1;
    }
    return 0;
}

int main(int argc, char **argv) {
    char install_dir[1024];
    if (argc > 1) {
        snprintf(install_dir, sizeof install_dir, "%s", argv[1]);
    } else if (IS_WIN) {
        const char *base = getenv("LOCALAPPDATA");
        if (!base) base = "C:\\Program Files";
        snprintf(install_dir, sizeof install_dir, "%s\\Programs\\rach", base);
    } else {
        snprintf(install_dir, sizeof install_dir, "/usr/local/bin");
    }

    const char *from_source = getenv("RACH_FROM_SOURCE");
    int rc;
    if (from_source && strcmp(from_source, "1") == 0) {
        rc = install_from_source(install_dir);
    } else {
        rc = install_from_release(install_dir);
        if (rc != 0) {
            fprintf(stderr, "!! falling back to source build\n");
            rc = install_from_source(install_dir);
        }
    }
    if (rc != 0) return rc;

    char verify[1300];
    snprintf(verify, sizeof verify, "\"%s%s%s\" version", install_dir, PATH_SEP, BIN_NAME);
    run(verify);
    fprintf(stderr, "==> installed. Try:  rach examples%shello.rach\n", PATH_SEP);
    return 0;
}
