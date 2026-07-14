# AETHER TUNNEL KNOWLEDGE BASE

## OVERVIEW

Independently shipped outbound relay CLI/service. Its control plane, provider egress, protocol flow control, and service installation are security-sensitive boundaries.

## WHERE TO LOOK

| Task | Location | Notes |
|---|---|---|
| CLI and managed-service conflict checks | `src/main.rs` | No subcommand starts the tunnel |
| Lifecycle orchestration | `src/app.rs` | Registration, pools, diagnostics, shutdown |
| Config precedence/validation | `src/config.rs` | CLI > env > TOML; file mode requires `[[servers]]` |
| Registration/control plane | `src/registration/client.rs`, `src/tunnel/heartbeat.rs` | Primary connection alone emits heartbeat |
| WebSocket protocol/data plane | `src/tunnel/` | Handshake, multiplexing, bounded writer, drain |
| Provider relay | `src/tunnel/stream_handler.rs`, `src/upstream_client.rs` | Redirect, replay, timeout, flow control |
| SSRF/DNS policy | `src/target_filter.rs`, `src/safe_dns.rs` | Validate ports and resolved targets |
| TUI/service/update | `src/setup/` | Root-gated service install and checksum update |

## CONVENTIONS

- Preserve the separation between control/tunnel proxy (`aether_outbound_proxy_url`) and provider egress proxy (`upstream_proxy_url`).
- Keep secrets and security mode scoped per `ServerContext`; redact proxy URLs before logging.
- Diagnostics endpoints bind loopback only. Tunnel mode opens outbound WebSocket connections, not a public listener.
- Add focused inline tests beside changed config, protocol, filtering, or relay modules.
- File config is multi-server only and rejects removed top-level server credentials.

## ANTI-PATTERNS

- Never bypass allowed-port and DNS/private/reserved target checks.
- Never forward sensitive headers across origins or hop-by-hop/blocked upstream headers.
- Never treat frame PSK as end-to-end protection: it does not protect registration, secret distribution, installer traffic, or provider egress.
- Do not break protocol-v3 window updates, bounded writer priorities, GOAWAY/drain, or autoscale min/max semantics.
- Do not let secondary connections send global heartbeat.
- Do not log raw management tokens, PSKs, or proxy credentials.

## COMMANDS

```bash
cargo check -p aether-tunnel
cargo test -p aether-tunnel
cargo clippy -p aether-tunnel --all-targets -- -D warnings
cargo build --release --locked --manifest-path apps/aether-tunnel/Cargo.toml
```

Release tags are `tunnel-v<version>` and must match `apps/aether-tunnel/Cargo.toml` exactly.
