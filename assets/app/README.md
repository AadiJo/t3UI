# App icon

Copied verbatim from the reference app's production brand assets
(`t3code-again/assets/prod/`, see its `scripts/lib/brand-assets.ts`):

- `black-macos-1024.png`: source of `AppIcon.icns` (iconutil, same sizes as the reference packager).
- `t3-code.icon`: Icon Composer file compiled by `actool` into `Assets.car`. macOS 26+ shows this
  adaptive icon; older systems fall back to the `.icns`.

`script/bundle-macos.sh` builds both, exactly as the reference `build-desktop-artifact.ts` does.
