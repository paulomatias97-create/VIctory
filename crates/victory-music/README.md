# Spotibuy: the phone's music app

A phone laid over the title's picture that drives the player's own Spotify
(Windows). Off unless asked for.

## Use

1. Open Spotify (desktop or Microsoft Store app), signed in, and play something.
2. Optional, for the intended look: put `Rubik-Black.ttf`, `Rubik-Bold.ttf`,
   `Inter-Regular.ttf` and `Inter-SemiBold.ttf` (SIL Open Font License, from
   Google Fonts) in a folder and point `VICTORY_FONTS_DIR` at it. Without
   them the phone uses Windows' own fonts.
3. Run the title with the phone on:

   ```powershell
   $env:VICTORY_PHONE = "1"
   $env:VICTORY_FONTS_DIR = "C:\fonts\spotibuy"
   target\release\victory.exe run <game dir>\<the title's exe> --execute
   ```

| Key (with the title's window focused) | Does |
|------|------|
| F7 | opens and closes the phone |
| F8 | play / pause |
| F9 | next song |
| F6 | previous song |

## Without the title

```powershell
cargo run -p victory-music --example music_probe -- now     # what Spotify plays
cargo run -p victory-music --example music_probe -- scene   # Spotify's volume following a scripted scene
cargo run -p victory-music --example music_probe -- around  # the music circling you (headphones)
cargo run -p victory-music --example phone_preview -- phone.png   # the phone, drawn to a PNG (any platform)
```

## How it works

* `media`: Windows' media controls give what Spotify plays and its cover,
  and take play, pause and skip. No account or key of ours.
* `mix` and `volume`: the scene sets Spotify's own volume in Windows' mixer
  (in a vehicle, under dialogue, walking away, paused). Spotify's audio is
  never captured or altered. The scene is not read from the title yet.
* `space`: outside the vehicle, its radio is heard from its side of the
  camera (and a little dimmer from behind), through Spotify's left/right
  balance in the same mixer. Only the balance a stereo pair gives: no
  front/back cues or muffling, which would need the audio itself.
* `ui`: the phone is drawn on the CPU (tiny-skia, fontdue); what stays still
  is kept, so a frame costs about 1 ms.
* `service`: the thread behind it; `victory-gpu::overlay` lays the picture
  over each presented frame as the compositor's last plane, uploading only
  the part that changed.
