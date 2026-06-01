# Protocol Onboarding Evidence Template

> Copy this file to `crates/integration-tests/onboarding/<protocol>/evidence.md` for each protocol onboarding run.
> This is a completion gate, not a nice-to-have note. If any mandatory row is missing, the phase is incomplete.

## Run Metadata

| field | value |
|---|---|
| protocol | |
| branch | |
| worktree | |
| date | |
| main agent | |
| base commit | |

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Codex current-session research executed | pending | |
| Claude Code or sub-agent research executed | pending | |
| Claude/sub-agent exact prompt or command recorded | pending | |
| Codex-only candidates listed | pending | |
| Claude/sub-agent-only candidates listed | pending | |
| dropped-unverified candidates listed with reason | pending | |
| final contract inventory verified against first-party sources | pending | |
| token-surface inventory completed or explicitly scoped out | pending | |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | pending | |
| `npm run check:surface` output recorded | pending | |

## P2 Synthetic Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| fuzz command with seed recorded | pending | |
| iterations >= 5000 or justified lower bound | pending | |
| fixed edge-case matrix recorded | pending | |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | pending | |
| representative pass/error corpus entries committed or justified | pending | |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | pending | |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | pending | |
| Etherscan `api_calls_used` recorded | pending | |
| Etherscan `raw_txs_seen` recorded | pending | |
| Etherscan `unique_selectors_seen` recorded | pending | |
| Etherscan real tx coverage per COVER selector recorded | pending | |
| Dune MCP/API availability checked | pending | |
| Dune usage baseline recorded | pending | |
| Dune calibration/query executed with partition WHERE or explicitly blocked | pending | |
| Dune `executionCostCredits` / usage delta recorded | pending | |
| Dune rows returned / selected tx hashes recorded | pending | |
| representative real-tx corpus/golden entries committed or justified | pending | |

## Blockers

If a mandatory item cannot be completed, write `blocked` rather than `done`.

| blocker | source | next action |
|---|---|---|
| | | |

## Final Completion Claim

Do not write "onboarding complete" unless every mandatory P0/P2/P4 row is `done` or has a concrete, user-visible `blocked` disposition.
