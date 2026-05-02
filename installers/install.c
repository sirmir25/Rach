/*
 * Rach installer in pure C99. Cross-platform: POSIX + Windows.
 *
 * Build:
 *   cc installers/install.c -o /tmp/rach-install   # POSIX
 *   cl installers\install.c /Fe:rach-install.exe   # MSVC
 *
 * Run:
 *   /tmp/rach-install              # default prefix
 *   /tmp/rach-install /opt         # custom (POSIX)
 *   rach-install.exe "C:\Tools"    # custom (Windows)
 *
 * Logic mirrors install.sh / install.bat: check cargo, build, copy, verify.
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
  #define MKDIR_P(p) _mkdir(p)
#else
  #include <sys/stat.h>
  #include <unistd.h>
  #define IS_WIN 0
  #define BIN_NAME "rach"
  #define PATH_SEP "/"
  #define MKDIR_P(p) mkdir(p, 0755)
#endif

static int run(const char *cmd) {
    fprintf(stderr, "==> %s\n", cmd);
    int rc = system(cmd);
    return rc;
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

int main(int argc, char **argv) {
    /* 1. Check cargo */
    if (run(IS_WIN ? "where cargo >nul 2>&1" : "command -v cargo >/dev/null 2>&1") != 0) {
        fprintf(stderr, "xx cargo not found in PATH. Install Rust from https://rustup.rs\n");
        return 1;
    }

    /* 2. Build */
    if (run("cargo build --release") != 0) {
        fprintf(stderr, "xx build failed\n");
        return 1;
    }

    /* 3. Locate src bin */
    const char *src_bin = "target" PATH_SEP "release" PATH_SEP BIN_NAME;
    if (!file_exists(src_bin)) {
        fprintf(stderr, "xx build did not produce %s\n", src_bin);
        return 1;
    }

    /* 4. Determine install dir */
    char install_dir[1024];
    if (argc > 1) {
        snprintf(install_dir, sizeof install_dir, "%s", argv[1]);
    } else if (IS_WIN) {
        const char *pf = getenv("ProgramFiles");
        if (!pf) pf = "C:\\Program Files";
        snprintf(install_dir, sizeof install_dir, "%s\\rach", pf);
    } else {
        snprintf(install_dir, sizeof install_dir, "/usr/local/bin");
    }

    /* 5. mkdir -p (single level — caller can pre-create deeper trees) */
    MKDIR_P(install_dir);

    /* 6. Copy */
    char dst_bin[1200];
    snprintf(dst_bin, sizeof dst_bin, "%s%s%s", install_dir, PATH_SEP, BIN_NAME);
    fprintf(stderr, "==> installing to %s\n", dst_bin);
    if (copy_file(src_bin, dst_bin) != 0) {
        fprintf(stderr, "xx copy failed — re-run with elevated privileges (sudo / Administrator)\n");
        return 1;
    }

    /* 7. Verify */
    char verify[1300];
    snprintf(verify, sizeof verify, "\"%s\" version", dst_bin);
    run(verify);

    fprintf(stderr, "==> installed. Try:  rach examples%shello.rach\n", PATH_SEP);
    return 0;
}
