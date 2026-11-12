# FAQ and see also

**Does the policy route callers to the successor automatically?**
No, never. Silent rerouting changes behaviour, cost and data residency. Use Model Proxy routing as a separate, explicit decision.

**How do I know who is still calling?**
Place Client ID Enforcement or JWT Validation before this policy. Then use three sources:
- An API alert of type *Policy Violation*. It e-mails you, but doesn't name the caller.
- Monitoring → *Requests by Client ID*.
- The `[ai-asset-sunset]` log lines.

For per-owner e-mails, build the scheduled notifier described in the project docs.

**What does `disclosure` do?**
- `minimal`: shows status, dates and docs only.
- `standard`: adds successor and contact for identified callers.
- `full`: shows everything to everyone.

**Can someone bypass it?**
Only by not going through the gateway. Close direct upstream access at sunset (private network, key rotation). Raw `client_id` headers never grant an exemption.

**What if I break the config?**
Before `sunsetAt` the policy passes traffic through and logs an error. After `sunsetAt` it returns a minimal 410, so a broken config cannot re-open a retired asset.

**Streaming LLM responses?**
In heads-up they get headers only; streams are never buffered. In tombstone the stream is never started.

**Support.**
This is a community policy and is not supported by MuleSoft. Issues go to the GitHub repository.

**See also:**
- `Data-Contract-Version-Gate`: deprecation by client-declared version.
- `ai-asset-sunset-ruleset`: catalog rules for deprecated assets.
- RFC 9745, RFC 8594, RFC 9457.
