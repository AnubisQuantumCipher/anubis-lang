# Industry programs (Anubis)

## Sovereign General Ledger + Risk Engine (`sovereign_gl_risk_engine.anb`)

**Domain:** fintech / banking / payments core control plane  
**Currency model:** integer **USD cents** (no floating-point money)

### What it does

1. Seeds a chart of accounts + opening balances (GAAP-ish classes)
2. Posts a full double-entry journal day (debits must equal credits)
3. Builds a trial balance and proves **Assets = Liabilities + Equity + Revenue − Expense**
4. Runs a bank reconciliation (deposits in transit, outstanding checks)
5. Multilateral payment **netting** across counterparties (gross vs net funding)
6. Single-name **concentration risk** in basis points vs a policy limit
7. Hash-chained **audit checksum** over journal fingerprints + control outcomes
8. Seals a **period-close verdict** (`PERIOD_CLOSED_BALANCED` or control fail)

### Run

```bash
cd /Users/sicarii/anubis-lang
cargo build --release -p anubis

./target/release/anubis run examples/industry/sovereign_gl_risk_engine.anb \
  --evidence --out out/industry_gl
```

Success signal:

- last line `0` (verdict code Balanced)
- `SUMMARY ok=1 residual=0`
- `close.verdict=PERIOD_CLOSED_BALANCED`

### Why this is “industry needed”

Every regulated money-moving org needs:

| Control | Where in the program |
|--------|----------------------|
| Double-entry integrity | `validate_journal_shape` + `post_journal` |
| Period books balance | `equation_residual` / trial balance |
| Cash vs bank | `bank_recon` |
| Treasury liquidity efficiency | netting `efficiency_bps` |
| Credit concentration | `concentration_bps` vs `limit_bps` |
| Audit trail seal | `chain_hash` + close verdict |

This is a **computational core** (policy + math + seal), not a database or UI product shell.
You would wrap it with storage, auth, and a report pipeline in production.
