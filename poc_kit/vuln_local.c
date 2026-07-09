/*
 * Intentionally vulnerable local lab target for Anubis PoC kit gold fixtures.
 *
 * NOT for production. NOT networked. Compile with:
 *   bash poc_kit/build_vuln.sh
 *
 * Crash model (reliable across hardening):
 *   - If stdin length > 64 bytes, process aborts with SIGABRT (simulates
 *     a bounds-check failure / sanitizer abort class finding).
 *   - Also demonstrates an intentional unchecked memcpy into a small buffer
 *     when ANUBIS_VULN_MEMCPY=1 (optional, may be stack-protected).
 *
 * This is a bounty-lab crash oracle, not a remote exploit chain.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void process(const unsigned char *in, size_t n) {
    char buf[32];
    /* Gold-path crash: oversized input => abort (always reliable). */
    if (n > 64) {
        fprintf(stderr, "vuln_local: oversized input len=%zu\n", n);
        abort();
    }
    /* Optional memcpy path for ASan demos when explicitly enabled. */
    if (getenv("ANUBIS_VULN_MEMCPY") != NULL) {
        /* Intentionally wrong: copies min(n, 128) into 32-byte buf. */
        size_t copy_n = n < 128 ? n : 128;
        memcpy(buf, in, copy_n);
        if (buf[0] == 'X') {
            fputs("ok\n", stdout);
        }
    } else if (n > 0 && in[0] == 'X') {
        fputs("ok\n", stdout);
    }
}

int main(void) {
    unsigned char in[4096];
    size_t n = fread(in, 1, sizeof(in), stdin);
    process(in, n);
    return 0;
}
