# Configuration

| Property | Type | Default | Notes |
|---|---|---|---|
| `deprecationAt` | string (RFC 3339 + offset) | required | Start of heads-up |
| `sunsetAt` | string | required | Start of tombstone; must be after `deprecationAt` |
| `switchOffAt` | string | required | Planned removal; ≥ `sunsetAt` |
| `message` | string ≤ 500 | generic | Shown to humans and LLMs |
| `successorName` / `successorUrl` | string | "" | `successorUrl` also goes into `Link rel="successor-version"` |
| `docsUrl` | string | "" | Public notice; `Link rel="deprecation"` |
| `contactName` / `contactEmail` | string | "" | Use a role mailbox. Shown only to identified callers under `standard` |
| `protocol` | auto/mcp/a2a/llm/http | auto | Forces the response shape |
| `mcpTombstoneStyle` | toolResult/jsonRpcError | toolResult | `toolResult` is LLM-visible |
| `llmTombstoneStyle` | error/assistantMessage | error | `assistantMessage` is for human chat UIs only, never for streams |
| `disclosure` | minimal/standard/full | standard | See FAQ |
| `injectBodyMetadata` | boolean | true | Buffered JSON ≤ `maxBodyRewriteBytes` only |
| `annotateToolDescriptions` | boolean | false | Appends the notice to MCP tool descriptions in heads-up |
| `maxBodyRewriteBytes` | integer | 262144 | 1 KiB – 1 MiB |
| `taskGraceMinutes` | integer | 1440 | Running A2A/MCP task follow-ups keep working after sunset (capped at `switchOffAt`). **Only when `protocol` is `mcp` or `a2a`**, never in `auto` |
| `identifyYourselfAt` | string | "" | From this date, anonymous callers get the tombstone |
| `brownouts[]` | `{start, durationMinutes}` | [] | Must lie inside heads-up |
| `exemptions[]` | `{clientId, until, reason}` | [] | Authenticated `client_id` only. Capped at `switchOffAt` |
| `emitViolation` | boolean | true | Feeds Policy Violation alerts. Never overwrites another policy's violation |
| `cacheClampSeconds` | integer | 3600 | Clamps MCP `ttlMs` / agent-card `max-age` in heads-up |
