# Music (not committed)

The bed this composition renders with is
`happy-beats-business-moves-vol-12-by-ende-dot-app.mp3` (109.96 BPM) from the
[`/brag`](https://github.com/latent-spaces/brag) skill's bundled library —
"Happy Beats / Business Moves" by [ende.app](https://ende.app/en).

It is **deliberately untracked** (see `brag-output/.gitignore`): the skill's own
`skills/brag/assets/music/README.md` says the exact license terms must be
verified and documented before the track is redistributed, and this repository
is public. The rendered `brag-output/brag.mp4` already contains the mixed audio.

`happy-beats-business-moves-vol-12-by-ende-dot-app.music-cues.json` *is*
committed — it is generated beat/cue metadata, and the composition's beat locks
reference its timestamps.

## To re-render

```bash
git clone https://github.com/latent-spaces/brag /tmp/brag
cp /tmp/brag/skills/brag/assets/music/happy-beats-business-moves-vol-12-by-ende-dot-app.mp3 \
   brag-output/composition/assets/music/

cd brag-output/composition
npx hyperframes check                       # must pass before rendering
npx hyperframes render --quality high --output ../brag.mp4
```

The render comes out at about -19.4 LUFS. The delivered `brag.mp4` is mastered
afterwards, which is also where the poster is baked in as frame 0:

```bash
cd brag-output
ffmpeg -ss 18.3 -i brag.mp4 -frames:v 1 -q:v 2 brag.jpg -y
ffmpeg -y -i brag.mp4 -i brag.jpg \
  -filter_complex "[0:v][1:v]overlay=0:0:enable='eq(n,0)'[v]" \
  -map "[v]" -map 0:a \
  -c:v libx264 -crf 18 -preset slow -pix_fmt yuv420p \
  -af "volume=6dB,aresample=192000,alimiter=limit=0.84:level=false:attack=5:release=80,aresample=48000" \
  -c:a aac -b:a 192k -movflags +faststart brag.out.mp4 && mv brag.out.mp4 brag.mp4
```

That lands at -13.7 LUFS / -1.2 dBTP with LRA 4.8 — the social delivery target
with the mix's dynamics unchanged.
