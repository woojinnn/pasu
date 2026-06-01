# Protocol Onboarding Evidence

Each protocol onboarding branch should create:

```text
crates/integration-tests/onboarding/<protocol>/evidence.md
```

Start from `../ONBOARDING_EVIDENCE_TEMPLATE.md`.

This directory stores committed completion evidence, not raw tx dumps. Raw Etherscan/Dune exports belong in `/tmp` or ignored scratch logs. The evidence file records exact commands, source summaries, counts, blocker dispositions, and gate output needed to prove that the onboarding framework actually ran end-to-end.

Do not mark any onboarding phase complete unless that phase's mandatory evidence rows are `done` or concrete `blocked`.
