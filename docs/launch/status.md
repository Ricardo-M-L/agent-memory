# Launch execution record

Snapshot: 2026-09-08 (Asia/Shanghai). This is a dated record, not a live dashboard.

## Published

- [GitHub repository](https://github.com/Ricardo-M-L/agent-memory): bilingual onboarding,
  accurate feature boundaries, description, homepage and eight relevant topics.
- [v0.1.1 prerelease](https://github.com/Ricardo-M-L/agent-memory/releases/tag/v0.1.1):
  source tag at `c6ec731481e0c51756b0e9ed00ebc03f582bb010`, verified source package and
  [SHA256SUMS](SHA256SUMS). This is an early-stage release, not a production-readiness claim.
- Three contribution opportunities with acceptance criteria:
  [cross-platform examples](https://github.com/Ricardo-M-L/agent-memory/issues/1),
  [extraction edge cases](https://github.com/Ricardo-M-L/agent-memory/issues/2), and
  [opt-in HTTP embeddings example](https://github.com/Ricardo-M-L/agent-memory/issues/3).
- [Technical walkthrough](../fact-history.md) and an executable three-process SQLite demo.
  AI assistance is disclosed in the article and community submission.

## Submitted, awaiting editorial review

[This Week in Rust PR #8711](https://github.com/rust-lang/this-week-in-rust/pull/8711)
targets the September 9 draft. It proposes the technical walkthrough and two contribution
tasks. At this check the PR is open; a submission is **not** newsletter acceptance or publication.
Do not duplicate the submission while it is pending. Respond to concrete editorial feedback.

## Not published

- **V2EX / Rust Chinese community:** [Chinese drafts](posts.zh-CN.md) are prepared, but there
  is no working authenticated browser connection available to this session. No posts were sent.
  Restore the browser connection, confirm the account and current channel rules, and publish
  at most one useful introduction per relevant community. Record actual post URLs afterward.
- **crates.io:** registry publication credentials were unavailable. Do not advertise
  `cargo add agent-memory` as an installation path; use the tested `v0.1.1` Git tag instead.
- **MP4/GIF:** the 60-second [composition source](../../videos/agent-memory-launch/README.md)
  is a preview candidate, not a delivered video. Final preview approval is still required
  before exporting and attaching a video to the release.
- **Hacker News / r/rust:** no AI-generated launch posts or comments were submitted.
  See the [community policy notes](community-policy.md).

## Verification

- [Release-source CI](https://github.com/Ricardo-M-L/agent-memory/actions/runs/34143375675)
  passed both the default/all-feature job and the real Neo4j job.
- Local default tests: 73 passed. All-feature tests: 84 passed; the five server-dependent
  Neo4j tests are separate and passed in CI, not silently counted as local coverage.
- Formatting, default/all-feature Clippy with warnings denied, documentation build,
  source-package verification, quickstart and fact-history examples passed.
- CLI installation was tested in fresh temporary prefixes both from the checkout and
  directly from the remote `v0.1.1` tag; the tagged binary reports `agent-memory 0.1.1`.
- `node scripts/check-launch.mjs` runs the real example, checks displayed output excerpts,
  compiles the exact Rust snippets in both READMEs, and checks local Markdown file links.
  CI now runs this check without a browser or model call. Node is a documentation-maintenance
  tool here, not an agent-memory runtime dependency.

## Follow-up criteria

1. Watch for substantive feedback on the newsletter PR and contribution issues; do not
   repeatedly repost or ask people to vote.
2. After browser access is restored, publish the two tailored Chinese introductions and
   answer integration questions in the original threads.
3. Measure genuine integration reports, reproducible issues and external contributions,
   alongside dated star counts. Views and clones may include automation and our own checks;
   they are not evidence of users or adoption.
4. Prefer a follow-up showing a real integration or shipped improvement over another
   generic launch announcement. No paid stars, reciprocal-star schemes or mass messages.
