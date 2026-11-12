# AI Asset Sunset: phased retirement for agents, MCP servers and LLM routes

An Omni Gateway policy that retires an asset in three dated phases:

1. **Heads-up.** The asset still works. Every response carries `Deprecation` (RFC 9745), `Sunset` (RFC 8594) and `Link` headers. Buffered JSON responses also get a deprecation object (MCP `_meta`, A2A `metadata`).
2. **Tombstone.** The upstream is no longer called. The policy answers in the caller's own protocol:
   - MCP: a tool result with `isError:true`, so the model can read it.
   - A2A: an `AGENT_SUNSET` ErrorInfo.
   - LLM: an OpenAI-compatible 410.
   - REST: RFC 9457 problem+json.

   Each answer names the successor, the dates and the contact.
3. **Switch-off.** You remove the instance. The policy logs a reminder until you do.

> **Community policy.** Published via p4a.ai and licensed under Apache-2.0. **Not a MuleSoft product and not supported by MuleSoft.** It is built on the GA PDK 1.10 and GA Omni Gateway features. Test in a sandbox before production.

## Quick facts
| | |
|---|---|
| Asset types | `http`, `rest`, `mcp`, `a2a`, `a2a_v1`, `llm` |
| Min. gateway | Omni/Flex Gateway 1.12.1 (stop-iteration) |
| PDK | 1.10.0, Rust 1.89 (PDK 1.10 deps need it), `wasm32-wasip1` |
| Placement | **After** your auth policy (Client ID Enforcement / JWT); for MCP, after MCP Support |
| Identity | Only from the PDK `Authentication` object, never from raw headers |
| Fail mode | Invalid config: pass-through before `sunsetAt`, minimal 410 after it |
| Local Mode | Works; violations and alerts are unavailable there |

## Layout
```
ai-asset-sunset-definition/   gcl.yaml (schema), exchange.json, Makefile
ai-asset-sunset-flex/         Cargo.toml, src/{lib,phase,notice,tests}.rs, playground/, Makefile
docs/                         the 4 catalog tabs: overview, configuration, examples, faq
```

## Build and test
```sh
cd ai-asset-sunset-definition && make build          # needs anypoint-cli-v4 PDK plugin
cd ../ai-asset-sunset-flex && cargo test --lib        # 37 unit + pdk-unit tests
make build-asset-files && make build && make run      # local Flex 1.12.1 in Docker
```
See `../test-plan.md` for the full local and online test plan, and `../red-team-10-attacks.md` for the threat model.

## Validation status (2026-11-10)
- **Not compiled yet.** The authoring machine has no Rust toolchain. Code and tests are written against the PDK 1.10 APIs documented in `omni-gateway-pdk-skills` v1.10.0. Run `cargo test` before trusting any behaviour.
- `src/generated/config.rs` is a hand-written mirror of `gcl.yaml`. `make build-asset-files` regenerates it.
- The pdk-unit tests assume the unit-test clock starts near wall time. If it doesn't, adapt them with `tester.sleep()`.

## Related
- `Data-Contract-Version-Gate` (p4a): version-based deprecation, driven by a client-declared contract version.
- `ai-asset-sunset-ruleset`: catalog governance for deprecated assets.
- Skill `ai-asset-sunset`: plan, apply, notify and run the switch-off gate.
