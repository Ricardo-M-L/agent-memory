---
workflow: general-video
flow: automation
storyboard: no
message: "Facts change. Keep current memory and inspect the old relationship."
destination: github-embed
aspect: 1920x1080
language: en
length: 60s
narration: no
---

## Intent

The maintainer approved the bilingual project-launch plan and asked the assistant to complete
it. Build a 60-second developer demonstration of Alice moving from Beijing to Shanghai,
using checked program output from examples/fact_history.rs. This is an explicit application
update, not model-generated reasoning. One continuous terminal/graph composition.

## Customizations

- English screen text for the international README; the Chinese README explains the same demo.
- Inferred format: 16:9 for a desktop GitHub demo, silent so code can be read without sound.
- No external media, voice service, stock imagery, tracking or paid generation.
- Show SQLite as the actual tested backend. Neo4j is an optional capability label, not a
  fabricated server screenshot; no live Neo4j Browser capture is available in this session.

## Notes

Use Montserrat and IBM Plex Mono, a warm paper canvas and a dark terminal. Retain the source
transcript and assert every displayed output excerpt is present. Keep secrets and local paths
out of public material. The skill requires final-preview approval before MP4 rendering.
