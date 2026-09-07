# Fact-history launch preview

60 seconds, 1920 × 1080, English, deliberately silent. This is an HTML animation of
verified output excerpts and an illustrated SQLite graph, **not a terminal screen recording
or Neo4j Browser capture**. The application explicitly supplies and updates synthetic facts.

Status: preview candidate. No MP4/GIF has been exported; the maintainer must approve the
final preview first. The Rust example works independently of this optional video tooling.

## Reproduce

From the repository root, with Rust and Node.js 22+ on PATH:

```bash
node scripts/check-launch.mjs
cd videos/agent-memory-launch
npm ci
npm run check -- --snapshots
npm run dev
```

HyperFrames needs Chromium for checks and FFmpeg for export. Dependencies and fonts may
download during preparation; the Rust fact-history program itself makes no network calls.
Use the Studio URL printed by the command, including `#project/agent-memory-launch`.

For an agent-managed persistent preview, use the exact HyperFrames version pinned in
`package.json` with `preview --background`, verify with `preview --status`, and stop it
with `preview --stop` after review. A foreground development server ends with its terminal.

## Browser selection

On the launch host, automatic detection chose an old Puppeteer-cached Chrome 121 even
after downloading Chrome Headless Shell 152. The old binary failed at startup. Selecting
the newly downloaded executable with the supported override allowed checks to pass:

```bash
# Replace this placeholder with the downloaded executable's actual path.
HYPERFRAMES_BROWSER_PATH=/absolute/path/to/chrome-headless-shell npm run check -- --snapshots
```

Use `hyperframes browser ensure` / `hyperframes browser path` from the pinned CLI to
diagnose selection. Do not delete shared browser caches or use a personal browser profile.

## Evidence and review

- Verified on 2026-09-08 with HyperFrames **0.8.31** (upgraded from 0.8.30) and
  Chrome Headless Shell **152.0.7977.30**. All five snapshots were visually inspected;
  the declared paper/ink/accent palette and Montserrat/IBM Plex Mono pairing were checked.
- The final automated gate reported zero lint/runtime/layout/motion findings and
  94/94 passing WCAG AA text checks across nine timeline samples.
- [Brief](BRIEF.md), [design specification](frame.md) and [timeline plan](STORYBOARD.md).
- `scripts/check-launch.mjs` compares every `SEQUENCE` output excerpt with a fresh execution
  of `examples/fact_history.rs`. It does not claim to verify every marketing assertion or
  a video's visual quality.
- `npm run check -- --snapshots` is the separate runtime/layout/motion/contrast gate.
- [Store](snapshots/frame-01-at-16.7s.png), [update](snapshots/frame-02-at-30.0s.png),
  [reopen and verify](snapshots/frame-03-at-43.3s.png), and
  [try it](snapshots/frame-04-at-56.7s.png) are actual browser snapshots for visual review.

After preview approval, export with `npm run render -- --quality high --output out/demo.mp4`
and verify the resulting duration and frames before uploading. Generated videos and dependency
caches are intentionally excluded from Git and the Rust source package.
