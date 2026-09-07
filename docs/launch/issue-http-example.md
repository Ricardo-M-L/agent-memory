## Why

The optional HTTP embedder exists, but the repository has no standalone, environment-configured
example showing how to supply a real embedding model while keeping the default demo offline.

**Difficulty: medium.** Good for a contributor familiar with Rust error handling and HTTP clients.

## Scope and acceptance criteria

- Add an example requiring the `http` feature, with endpoint/model/dimensions/key read explicitly
  by the example from environment variables. Do not make the library read global environment state.
- Validate missing configuration before making a request. Never print keys or request headers.
- Use HTTPS for remote endpoints; document loopback HTTP separately for local development.
- Configure the `AgentMemory::with_store` path and explain why stored and query embeddings must
  use a compatible model/dimension. Use a fresh, explicitly selected demo database.
- Add offline transport-based tests for request/response wiring. CI must not make billed model calls.
- Document exact run instructions and that an actual endpoint/key may incur provider charges.
- Keep default-feature builds free of network dependencies.

Start in `src/http_embed.rs`, `src/memory.rs`, `Cargo.toml` and `examples/`.
Existing `HttpTransport` injection can be reused for tests. Discuss API changes before implementing them.

[Contribution instructions](https://github.com/Ricardo-M-L/agent-memory/blob/main/CONTRIBUTING.md).
Project license: MIT; no additional contributor agreement.
