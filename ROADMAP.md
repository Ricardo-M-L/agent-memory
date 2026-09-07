# Roadmap

This is an early-stage library. These are proposals, not shipped features or delivery promises.
Feedback from a runnable integration takes priority over expanding the feature list.

## Next: make adoption reproducible

- Add Windows/macOS CI coverage, starting with offline examples and CLI regression tests.
- Test non-ASCII, whitespace and punctuation behavior in rule extraction; document limitations.
- Add an environment-configured, opt-in HTTP embeddings example with no hard-coded credentials.
- Complete crates.io publication and versioned API documentation once registry access is available.

## Then: reliability and measured scale

- Specify partial-failure/recovery behavior across memory, embeddings and graph writes.
- Add reproducible retrieval and graph benchmarks before making scale or accuracy claims.
- Design pagination and native Neo4j traversal while preserving backend contract tests.
- Clarify the tenant-routing API before exposing shared memory through a server.

## Explore when users need it

- MCP integration or a language-neutral service interface.
- Migration/export tooling between SQLite and Neo4j with dry-run and validation.
- Typed entity extraction and a full graph-change log, distinct from the current invalidation model.

Discuss larger changes in an issue first. Small tests and documentation improvements can go
directly to a PR. See [CONTRIBUTING.md](CONTRIBUTING.md).
