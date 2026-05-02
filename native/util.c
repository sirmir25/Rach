/*
 * Native helpers for Rach, written in C.
 *
 * Linked into the Rust interpreter at build time via build.rs / cc-rs.
 * Exposed to Rach scripts via the `native_*` stdlib commands.
 */

#include <stddef.h>
#include <stdint.h>
#include <string.h>

/* CRC-32 (IEEE 802.3, reflected). Pure C, no allocations. */
uint32_t rach_crc32(const uint8_t *data, size_t len) {
    static uint32_t table[256];
    static int initialised = 0;
    if (!initialised) {
        for (uint32_t i = 0; i < 256; i++) {
            uint32_t c = i;
            for (int j = 0; j < 8; j++) {
                c = (c & 1) ? 0xEDB88320u ^ (c >> 1) : c >> 1;
            }
            table[i] = c;
        }
        initialised = 1;
    }
    uint32_t crc = 0xFFFFFFFFu;
    for (size_t i = 0; i < len; i++) {
        crc = table[(crc ^ data[i]) & 0xFFu] ^ (crc >> 8);
    }
    return crc ^ 0xFFFFFFFFu;
}

/* Standard base64 encode. `out` must have room for ceil(len/3)*4 + 1 bytes.
 * Returns the number of bytes written (excluding the NUL terminator). */
size_t rach_base64_encode(const uint8_t *in, size_t len, char *out) {
    static const char alpha[] =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    size_t o = 0;
    size_t i = 0;
    while (i + 3 <= len) {
        uint32_t v = ((uint32_t)in[i] << 16) | ((uint32_t)in[i + 1] << 8) | in[i + 2];
        out[o++] = alpha[(v >> 18) & 0x3F];
        out[o++] = alpha[(v >> 12) & 0x3F];
        out[o++] = alpha[(v >> 6)  & 0x3F];
        out[o++] = alpha[ v        & 0x3F];
        i += 3;
    }
    if (i < len) {
        uint32_t v = (uint32_t)in[i] << 16;
        if (i + 1 < len) v |= (uint32_t)in[i + 1] << 8;
        out[o++] = alpha[(v >> 18) & 0x3F];
        out[o++] = alpha[(v >> 12) & 0x3F];
        out[o++] = (i + 1 < len) ? alpha[(v >> 6) & 0x3F] : '=';
        out[o++] = '=';
    }
    out[o] = '\0';
    return o;
}
