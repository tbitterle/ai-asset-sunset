# Examples

## Heads-up for an MCP server, with two brownouts and one exemption
```json
{
  "deprecationAt": "2026-12-01T00:00:00Z",
  "sunsetAt": "2027-01-15T00:00:00Z",
  "switchOffAt": "2027-02-15T00:00:00Z",
  "message": "The orders MCP server v1 is being retired.",
  "successorName": "orders-mcp-v2",
  "successorUrl": "https://gw.example.com/orders-mcp-v2/mcp",
  "docsUrl": "https://example.com/deprecations/orders-mcp-v1",
  "contactEmail": "ai-platform@example.com",
  "brownouts": [
    {"start": "2026-12-15T09:00:00Z", "durationMinutes": 60},
    {"start": "2027-01-05T09:00:00Z", "durationMinutes": 240}
  ],
  "exemptions": [{"clientId": "7f3c…", "until": "2027-01-29T00:00:00Z", "reason": "CHG-4711 payroll freeze"}]
}
```

## What an MCP client sees in tombstone (`tools/call`)
```json
{"jsonrpc":"2.0","id":42,"result":{
  "content":[{"type":"text","text":"[DEPRECATED SERVICE] The orders MCP server v1 is being retired. Sunset: 2027-01-15T00:00:00+00:00. Use the replacement 'orders-mcp-v2' (https://gw.example.com/orders-mcp-v2/mcp). Details: https://example.com/deprecations/orders-mcp-v1"}],
  "isError":true,
  "_meta":{"ai.p4a/deprecation":{"status":"sunset","phase":"tombstone","sunsetAt":"2027-01-15T00:00:00+00:00"}}}}
```

## Heads-up response headers
```
deprecation: @1796083200
sunset: Fri, 15 Jan 2027 00:00:00 GMT
link: <https://example.com/deprecations/orders-mcp-v1>; rel="deprecation", <https://gw.example.com/orders-mcp-v2/mcp>; rel="successor-version"
x-p4a-sunset-phase: heads-up
```
