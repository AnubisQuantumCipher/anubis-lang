# keychain_ne_cap

Real program: mint a **non-exportable** capability, prefer **Keychain bind** on macOS signed run, write a local file via causal spend, then consume the token.

```bash
# From repo root (macOS + Apple Development identity → kc: tokens)
cargo build -p anubis --release
./target/release/anubis check examples/showcase/keychain_ne_cap/main.anb
./target/release/anubis run examples/showcase/keychain_ne_cap/main.anb --out out/keychain_ne_cap

# Force soft path
ANUBIS_RUN_NO_SIGN=1 ANUBIS_KEYCHAIN_CAPS=0 \
  ./target/release/anubis run examples/showcase/keychain_ne_cap/main.anb --out out/keychain_ne_cap_soft
```

Expected under signed Development identity: `ne_token=` line contains `__anubis_cap_ne_kc:`.
