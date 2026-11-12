# Overview

**AI Asset Sunset** retires an agent, MCP server, LLM route or API on a published schedule. It never surprises the people and agents who call the asset.

| Phase | When | What callers experience |
|---|---|---|
| Heads-up | `deprecationAt` → `sunsetAt` | Normal answers, plus `Deprecation`/`Sunset`/`Link` headers and a machine-readable notice |
| Brownout (optional) | announced windows | Short tombstone windows that wake up clients which ignore headers |
| Tombstone | `sunsetAt` → removal | Only the notice: protocol-correct, LLM-readable, no upstream call |

**Key caveat:**
- This is a community policy, not supported by MuleSoft.
- It only governs traffic that passes through Omni Gateway. Close direct access to the upstream at sunset.

Who uses it, in AI Control Tower terms:
- **AI Steward:** sets the dates and grants exemptions.
- **AI GRC Expert:** owns the notice-period rules (ruleset) and approves the switch-off.
- **AI Product Owner:** publishes the successor and answers the contact address.
