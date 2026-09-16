# Third-party assets in this composition

| Path | Source | License |
|---|---|---|
| `sfx/impact/`, `sfx/interface/`, `sfx/ui/` | [Kenney](https://kenney.nl/) via the `/brag` skill | CC0 (public domain) |
| `sfx/keyboard/` | [Keyboard Soundpack #1](https://opengameart.org/content/keyboard-soundpack-1-typing-and-single-keystrokes) by unicae_games, via the `/brag` skill | CC0 (public domain) |
| `fonts/Inter.woff2` | [Inter](https://fonts.google.com/specimen/Inter) by Rasmus Andersson (Google Fonts latin subset) | SIL Open Font License 1.1 |
| `fonts/JetBrainsMono.woff2` | [JetBrains Mono](https://fonts.google.com/specimen/JetBrains+Mono) (Google Fonts latin subset) | SIL Open Font License 1.1 |
| `vendor/gsap.min.js` | [GSAP](https://gsap.com/) 3.14.2 | GreenSock standard "no charge" license |
| `music/*.mp3` | "Happy Beats / Business Moves" by [ende.app](https://ende.app/en) | **Not committed** — see `music/README.md` |

Inter and JetBrains Mono are vendored rather than linked because the renderer
must resolve fonts locally and deterministically; GSAP is vendored because the
render browser has no trust anchor for this environment's egress proxy and the
CDN fetch fails with `ERR_CERT_AUTHORITY_INVALID`.
