# evotap

The audio tap of the [evo framework](https://evoframework.org) for Volumio, part of that ecosystem: an ALSA plugin loaded into the player's audio process, through which every stream the player plays passes byte for byte, PCM, DoP and native DSD alike. On the way it measures the stream for meters and, when asked, shares the stream itself with any process on the player that wants the audio. Nothing in it blocks the audio: it copies, hands over, and the player plays on; a reader that falls behind loses what it missed and the player never waits.

evotap is built and tested in the [Glass](https://github.com/foonerd/glass) repository, which is the one source, and released here as its own artefact so that any plugin can ship it without depending on Glass.

## Getting it

Every release carries one archive per architecture, `evotap-<version>-<arch>.tar.gz` for `arm` (armv6/armv7 32-bit), `armv7`, `armv8` (aarch64) and `x64`, holding:

| File | What |
| --- | --- |
| `lib/libevotap.so` | The ALSA plugin: the PCM `type evotap` and the scope of the same name. |
| `bin/evotap-cat` | Streams a live audio ring to standard output. |
| `bin/evotap-dump` | Prints what a live measurement ring says, a line per hop. |
| `README.md` | This contract. |
| `LICENSE` | MIT. |

Beside the archives, `SHA256SUMS` and `SHA256SUMS.sig`: every file's SHA-256, signed with the evo release key (Ed25519, the public key in `keys/release-signing.pub` here, the same key Glass's releases are signed with). Inside every archive, `MANIFEST` and `MANIFEST.sig` say the same of the archive's own files, for a check without the network. A consumer pins a version and verifies before it installs.

The library is built against Volumio's glibc 2.36 (bookworm) and needs no library beyond libasound, which the player has.

## Shipping it in a plugin

A plugin that wants the audio ships the library under its own folder and names it in its own ALSA contribution, with a tag of its own, so it stands in the chain beside Glass's tap or without it:

```
pcm_type.evotap {
  lib /data/plugins/<category>/<plugin>/lib/libevotap.so
}
pcm.<Plugin> {
  type evotap
  slave.pcm "<the next PCM in the chain>"
  ring "<tag>"
  audio 1
}
```

Several tap instances in one chain are fine; each costs one copy of the frames per transfer. The contribution goes in through Volumio's plugin contribution API (`volumio_info.has_alsa_contribution` and an `asound/<in>.<out>.<rank>.conf` file) before `softvolume`, so what the tap sees is the stream at full level, whatever the player's volume.

Keys in the PCM node beside `slave`:

| Key | Meaning |
| --- | --- |
| `ring` | The tag in the rings' file names. Letters, digits, `_` and `-`. Default `evotap`. |
| `hop` | Frames between measurements; 0 for the bank's own. |
| `slots` | Hops the measurement ring keeps, 2 to 4096, default 64. |
| `audio` | 0 or 1, default 0: whether the audio ring is written. Off, nothing of it runs. |
| `audio_seconds` | Seconds of the stream the audio ring holds, 1 to 10, default 2. |

## The rings

The tap publishes through files under `/dev/shm`, one per writing process and per stream. They are shared memory, not disk: any process on the player may read them, in any language, by the layouts below. Readers are many and anonymous; the writer never knows of them.

### The measurement ring

`/dev/shm/evometer.<tag>.<pid>.<n>`: the peak and RMS of each channel and the spectrum as a bank of bands per channel with its peak hold, one slot per hop, the newest overwriting the oldest. A 256-byte header, little-endian: magic `EVOMETER`, version 2, header size, sample rate, channels, FFT size, bands per channel, slots, slot size, writer pid, hop, then the sequence and time of the last publish (written last, with release ordering), the bank's scale, and how many times the writer made the file again after finding it gone. Each slot: sequence, time, frames so far, peak and RMS per channel, the bank per channel, the peak hold per channel, the sequence again (a reader whose two sequences disagree caught a write in flight and tries again), and flags. The exact offsets are in Glass's `crates/tap/src/ring.rs`. `evotap-dump [seconds]` prints a line per hop.

### The audio ring

`/dev/shm/evoaudio.<tag>.<pid>.<n>`, written only when the snippet says `audio 1`: the stream's own bytes, the frames interleaved as ALSA delivers them, at native width. A 256-byte header, little-endian:

| Offset | Field |
| --- | --- |
| 0 | magic `EVOAUDIO` |
| 8 | version, 1 |
| 12 | header size, 256 |
| 16 | sample rate |
| 20 | channels |
| 24 | the ALSA format number (`snd_pcm_format_t`) |
| 28 | bytes per frame, every channel |
| 32 | capacity in bytes, a power of two (64-bit) |
| 40 | writer pid |
| 44 | flags: bit 0 one-bit audio (DSD) |
| 48 | head: bytes written since the ring was made (64-bit, written last) |
| 56 | time of the last write, nanoseconds of the monotonic clock |
| 64 | bytes dropped by the writer (a transfer longer than the ring) |
| 72 | time the ring was made |
| 80 | how many times the writer made the file again after finding it gone |

The data follows the header: `capacity` bytes, the stream at `head` modulo `capacity`. A reader keeps a cursor, reads the head, copies from the cursor to the head, reads the head again: if the writer has moved more than a capacity past the cursor meanwhile, the copy may hold bytes written over and is discarded, the cursor moves to the head and the gap is counted.

One ring is one stream of one shape. When the player changes format or rate, or closes the stream, the ring goes and a new one is made for the next: a reader whose file is gone has reached the end of a segment and looks for the next ring of its tag. A stream that stands still, paused or waiting, keeps its ring; a reader tells a live ring by its writer's process being alive, the file present, and its inode unchanged.

### A ring removed

`/dev/shm` is cleared of a user's shared memory by systemd-logind when that user's last login session ends. The writer notices within half a second and makes the file again under the same name: the measurement ring with every slot it held, the audio ring with its header and an empty data region, the head going on. The header counts the heals. A reader tells the new file by its inode and opens it afresh.

## evotap-cat

```
evotap-cat [--tag TAG] [--wait S] [--follow] [--gap silence|skip] [--s32] [--shape]
```

Takes the newest live audio ring, of `TAG` when given, says the shape on standard error as the segment begins (`rate=… channels=… format=… frame=… one_bit=… ffmpeg=…`, the last being ffmpeg's name for the raw bytes where it has one), writes the bytes as they are, puts zeros in place of bytes it lost when it fell behind (`--gap skip` leaves them out), and exits 0 when the segment ends, so a supervisor starts it again with its encoder for the next shape. `--follow` goes on to the next segment instead. `--s32` turns every linear format into S32_LE with the valid bits left-justified, for an encoder that has no name for a 24-bit sample in a four-byte word; one-bit audio passes as it is. `--shape` prints the line and exits. `--wait S` is how long to wait for a ring, 5 seconds unless given, 0 for ever; none in time exits 2.

```
evotap-cat --tag pub --s32 | ffmpeg -f s32le -ar 48000 -ac 2 -i - -c:a libmp3lame -b:a 192k -f mp3 icecast://source:pass@host:8000/mount
evotap-cat --tag pub --follow > /data/INTERNAL/capture.raw
```

## The contract

The rings are a contract: the magic names the layout, the version counts the changes to it, and a reader refuses a version it does not know. Additions come as new versions; nothing within a version moves. The measurement ring is at version 2, the audio ring at version 1. The names from before the evotap rename (`glasstap`, `libglasstap.so`, `type glasstap`, `glasstap.<tag>` rings, `glasstap.demand`) are still read, loaded or answered for a release window, since an audio process keeps the library it loaded until it reopens its device.

## Licence

MIT, as Glass. evotap is designed, built and maintained by one developer in his own time; sponsorship through GitHub Sponsors or Ko-fi is a thank-you, not a purchase.
