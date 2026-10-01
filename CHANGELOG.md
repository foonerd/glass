# Changelog

All notable changes to Glass are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.7.99] - 2026-10-01

An X server of the display's own. Where the kernel does not drive the screen, the picture comes through an X server; `GLASS_SCREEN_OURS=1` from the launcher tells the display that the X server it draws on is there for it alone, with no kiosk on it. The display then treats that screen as it does one it draws on itself: it never leaves it, shows black after the countdown rather than what lies under it, a touch outside a control does not end it, and it turns the picture itself. Nothing changes under a kiosk's X server.

## [0.7.98] - 2026-10-01

Three faults found on an x86 player with the image's own kiosk.

The one-line installer no longer leaves Glass installed and off. `get-glass.sh` restarted the player's backend as soon as the plugin manager said Glass was enabled, and the manager writes that to its registry a moment later; on a fast player the restart came first and the player came back with Glass installed but not enabled, its Manager unreachable. The installer now waits until the registry on disk says enabled before it restarts, and says so if it never does.

On a player whose image runs its own kiosk (x86, and the products with a built-in screen), the display shows without a restart of the kiosk. There the X server runs as root and admits only root; Glass's install adds the player's own user for every session to come, and the session already running was left out, so the display was refused ("Authorization required") and nothing showed until the kiosk started again or the player was restarted. The install now admits the user to the running session as well.

The display no longer runs with nothing to draw on. On a player with no X server, no Wayland and no KMS/DRM device, the graphics library fell back to a driver that draws to nowhere: the display ran, reported itself as running, and the screen stayed empty. It now stops with "no screen to draw on", which the log and the Manager show; a driver of that kind is still taken when it is asked for by name (`SDL_VIDEODRIVER`).

## [0.7.97] - 2026-10-01

The plugin's side of glass-evo, the Glass interface for the player's own screen. Nothing shows and nothing changes on a player that does not have the glass-evo component; the kiosk stays the player's interface there, as before.

- **The screen's owner.** Where the component is installed, the Manager's Screen tab says who owns the screen, the kiosk or glass-evo, and hands it over and back. A take turns the kiosk's plugins off through the player's own plugin manager, stops the kiosk's units, and records each change with what it was before; the way back restores exactly what the take changed, and only where it is still as the take left it. While glass-evo owns the screen the launcher runs its binary in the display's place.
- **The face size.** Normal, large for a hand at arm's length, or car for a glance while driving.
- **How the screen looks.** A row of looks to choose from, the one that follows the artwork and every face theme, the user's own and those glass-evo ships; a likeness of the player's screen, when nothing plays and while music plays; and the adjustments in plain words, a section each for colours, backgrounds, the clock, the date and the buttons, each saying in a line how it stands. A click on a part of the likeness opens its section. Nothing reaches the player until the save, and only what differs from the look is kept.
- **The status sheet** names the screen's owner, the component and what the last take changed.
- **Routes.** `GET` and `POST /api/face` for the face's settings, `POST /api/screen/owner` for the take and the way back; `GET /api/screen` carries the owner and the face size.

The continuous build also covers the `evo` branch, where the next stages of this work are made.

## [0.7.96] - 2026-10-01

A letter no longer comes out as a grey smear. The outline rasteriser Glass sets its text with miscounts a glyph whose points land exactly on pixel edges, which a pen standing at a quarter or a half of a pixel can bring about: the bold face's w at 25 pixels with the pen three quarters of a pixel across, the w of "The Show Must Go On" in a theme's title, was drawn as a grey block with a faint letter in it, on the player's screen, on remotes and on the Face alike; the regular face's T and backslash did the same at 25 and 50 pixels. Six such cases in four million tried over the built-in faces. Every glyph is now set a 1024th of a pixel to the right of its pen, which nothing sees and which takes the points off the edges; none of the four million is miscounted with it.

For a face over the display: what a face drew is no longer left behind when it stops drawing. The display showed the whole picture while a face drew and went back to showing what the theme changed when it stopped, so the last frame of a bar fading out stayed, faintly, wherever the theme stood still; the frame after is shown whole now. `GLASS_GRAB_AFTER=N` makes the grab aid (`GLASS_GRAB`) wait N frames rather than thirty, to look at a screen later in a run.

## [0.7.95] - 2026-10-01

A face is handed its own settings. Every `face.<name>` key of the display's configuration reaches a face drawn over the display through its view, by name and as written, the writer's backslash before a `#` taken off so a colour arrives as `#rrggbb`; the display reads none of them but the size, and their meaning is the face's, so a new setting of glass-evo needs no change in Glass. The library surface a face is built on also carries the renderer's picture reading, scaling and blur, for a face that takes its colours from the cover and frosts what lies under its glass. Nothing changes for the display itself.

## [0.7.94] - 2026-10-01

Make a report. Whatever Find a problem's checks say, a report can follow, so a finding that does not settle the matter is where a report begins rather than where the page ends. Glass logs in full while the problem is reproduced, the display starting again so that its own lines are full too; when it has happened you say in a line what you did and what you saw, and the player's system log goes out through Volumio's own submitter, the call the player's dev page makes: the kernel's and the player's journal with Glass's lines of those minutes among them, the configuration and the network details, passwords left out, filed under the Glass version, the symptom and your line. The report holds your words, the minutes it was reproduced in by the player's clock, the log's link, what the checks found and the status sheet. Two ways to hand it over: Open a GitHub issue, for a defect, with the report in the issue where a link can carry it and on the clipboard either way; Copy for the forum, for a question or anything else. Nothing is sent before Start is pressed and the page says what goes where.

The log level is put back by the plugin, not the page: when the report is made, on Cancel, after ten minutes, and at the next start when the backend went down in between, which is how some problems are reproduced; a capture under way is picked up by a page opened later. A change of the log level is written to disk at once. Where the log cannot be sent, a player with no way out, the report says so and the log file is offered as a download to attach. `GET /api/diagnose/capture` says where a capture stands; `POST /api/diagnose/capture` with a symptom starts one, `.../finish` with the text makes the report, `.../cancel` ends it; `GET /api/diagnose/log` downloads a log that stayed on the player.

## [0.7.93] - 2026-10-01

Find a problem. The Status tab has a guided diagnosis: it asks what is wrong, nine symptoms taken from the Troubleshooting page (the meters never appear, they show but do not move, the display dies or keeps restarting, touch or the controls, the picture's turn and place, pictures missing, remote displays and the Face, slowness, something else), and runs the checks the Manager can run itself over what it already knows: the status sheet, the screen's facts, the release state, and Glass's lines in the journal since the backend started. Each check stands for one known cause, among them a player that is not playing, a display set to no screen of its own, a kiosk that failed or is on its way, a display that died and why, the audio tap out of the sound path, a source playing below the tap, interactive controls off, no touch panel, a failed calibration, a portrait panel not turned, remotes not served or not receiving, a frame rate the display had to lower. A finding is a likely cause, something worth knowing, or a thing checked and found right, in that order, with a button to the tab where it is set. Copy as text carries the findings with the sheet. `GET /api/diagnose` lists the symptoms and `POST /api/diagnose` with one returns the findings.

## [0.7.92] - 2026-10-01

The status sheet carries the screen whole. The Status tab's Screen section now says what the display draws on and through which renderer, whose the screen is, the kiosk service, the X server and the kiosk's plugins with the Touch Display plugin's angle, the screen's rotation and pointer, the touch mapping and its calibration, the panels with their native sizes, the touch panels, mice and keyboards with their event nodes, and the backlights; Copy as text carries all of it into a report. These facts are read for the sheet on show only, so the page's start costs what it did. The Screen tab keeps what it acts on: what the display draws on, whose the screen is, and its controls with the suggestion beside them, with a button to the sheet for the rest; the list of what the player has and its Look again button are gone from there. `GET /api/screen` names the renderer in `now.display.renderer`.

For those who build Glass: `scripts/check.sh` lints the plugin's scripts, the browser module and the pages' inline scripts for a name nothing defines, with one pinned ESLint fetched for the run.

## [0.7.91] - 2026-10-01

Theme previews render again on a player whose log level is below Info. The Manager learned which pictures a preview render had made from the display's own lines on its output, and those lines are held back below Info, so such a player rendered every theme and was told there were no pictures, at every upgrade that made the previews stale; reported from the community after 0.7.90. The pictures the render wrote are read from their folder now, shown in the display's order where it says one, else the theme's.

## [0.7.90] - 2026-09-30

A screen save goes through again. 0.7.89 read the face size of an unset key as the word "undefined", and its save path, which keeps the size as it stands unless one is given, then refused every save of rotation or pointer on the Screen tab; the size is read in one place now, normal unless the key says large or car. Also the Save row of the screen settings stood alone while the kiosk held the screen, its layout rule outranking the hidden flag; the flag outranks every display rule now.

## [0.7.89] - 2026-09-30

The turn stays the screen's own. With a rotation set for a panel the display draws on itself, the display applied the same turn under X too, on top of the turn the X server already makes, and the theme came out turned twice; since 0.7.83 that was the state of any player with a rotation kept and the kiosk back on. The rotation applies only where the display draws on the screen itself now; under X it is 0, as the Screen page always said.

A face size, for glass-evo. One key in the display's configuration, `face.size`, normal, large or car, that the display hands to a face as a scale, 1, 1.4 or 2, so a face draws its controls and its clock for a hand at arm's length or for a glance while driving; `GET` and `POST /api/screen` carry it as `faceSize`. The display itself draws nothing differently.

## [0.7.88] - 2026-09-30

A track inside a cue sheet finds its folder's pictures. Volumio names such a track `cue://<path>@<track>`, and both the display and the Manager took anything with a scheme for a stream with no folder, so a theme's folder pictures, back.jpg among them, never showed for CUE albums; reported from the community with a CUE + FLAC test. The sheet's folder is the track's now, on the player and for remote displays alike. Also: the watcher restarts a kiosk unit only for a failure that happened after the display stepped aside, not for one left from before.

## [0.7.87] - 2026-09-30

The display can carry a face. This is the library surface glass-evo is built on, and nothing changes for the display itself: `glass::run_with` takes an `Overlay`, drawn over the picture after the theme, offered every touch before the theme's controls, and sending the commands it hands back the way the theme's buttons go; `expose::ui` gives a face its primitives: a fill with an alpha, a blit with an alpha, a line of text in the theme's fonts. With no face, which is every player today, the loop is as it was.

## [0.7.86] - 2026-09-30

The screen watcher, lighter. It read the kiosk unit's two facts with two processes at every tick; one `systemctl show` answers both, and answers for a unit in any state. It ticks every two seconds only while the display draws on the screen itself, where it must step aside fast, and every five seconds otherwise.

## [0.7.85] - 2026-09-30

No launch against an X server that is closing. When the kiosk went, the display could be started twice on X in the second before the fact said the screen was free, each try failing with "x11 not available" in the log. A display lost to an X server closing under it, and the moment the screen turns free, both hold launches for three seconds; the watcher then brings the display up where it belongs.

## [0.7.84] - 2026-09-30

The kiosk's plugin counts by its running state. Volumio keeps a plugin's enabled flag and its running status apart: the flag set on its own brings nothing until the next boot, and 0.7.83 read the flag, so a flag set without a start left the screen dark for a kiosk that was not coming. The fact now takes the Touch Display or Display Configuration plugin as bringing the kiosk when it is started or starting; the flag alone counts only in the first two minutes after Glass starts, when plugins come up one after another at boot.

## [0.7.83] - 2026-09-30

The screen by fact, and the kiosk always first. Since 0.7.74 the Screen tab offered "Drawn by: the screen itself" and told users to turn the Touch Display plugin off to get it. That could leave a player with a screensaver and no interface, and the kiosk is the interface, at times the only way into the player. The choice is gone. The plugin reads the fact at every start and every two seconds: an X server up, the kiosk unit running, starting or enabled at boot, the Touch Display or the Display Configuration plugin on, a panel connected. While the kiosk runs or is on its way, Glass draws on X as a screensaver and shows no screen controls. Only when no kiosk uses the screen at all does Glass draw on the screen itself, stay on it, and offer rotation, pointer and touch; and the moment the kiosk comes back, the display steps aside, starting the kiosk again if it failed against the display. The Screen tab says what the display draws on now and why. A driver key set through the old choice goes back to Auto.

## [0.7.82] - 2026-09-30

The calibration judges each touch against the map made from the others. A single touch that missed its target was spread over every sample by the fit from all five, so a miss of a tenth of the panel left a worst error under the tolerance and the map was kept. Each touch is now also held against the map fitted from the other four, where its own miss shows in full, and that miss must be inside the tolerance too; a refused calibration reports it.

## [0.7.81] - 2026-09-30

The touch calibration reads only the touches it asked for. A finger lifted before the targets appeared, a tap to see whether the display stays, say, was kept and paired with the first target, so every sample sat one target off, the map came out wrong with an error in the hundreds of pixels, and the display took it anyway. The display now keeps lifts only while the targets are on the screen, and a map whose worst miss is past a twentieth of the screen's diagonal is refused: the Screen tab says so with the error, and the mapping in force stays.

## [0.7.80] - 2026-09-30

Two reports from the community. The settings page's "Fit to screen" switch could turn the fit off but never on: the page did not send the switch with its section, so a save read it as off; it is sent now. A rotation of one meter, a theme with one, or one chosen from the list, with a change interval set, loaded that same meter again at every interval, a visible reload; the display now moves on only to a meter that is not the one on show, and a rotation of one stays put.

## [0.7.79] - 2026-09-30

Touch set right, and a screen that is never empty. A finger's share of the panel now goes through a matrix before it becomes a pixel, `touch.matrix`, so a panel whose touch frame is swapped, mirrored, offset or scaled against the picture is set right without X. The Screen tab's Touch section offers three ways: as the panel reports, swap or mirror switches, or calibrated: the display shows five targets on the screen, one touch each, computes the map with the least error, keeps it and uses it at once, and the tab says the worst error left or why it could not fit. When the display draws through KMS/DRM the screen is Glass's alone, so it never lands on the console: the display is started with the plugin and again whenever it is found gone, it stays on the screen with the player stopped, black after the countdown, and a touch outside a control does nothing there.

## [0.7.78] - 2026-09-30

The Screen tab. The Manager reads what the player has for a screen from the kernel and the system when the tab opens: the panels and their native sizes, with a portrait panel named as such; the touch panels, mice and keyboards, an HDMI remote told apart from a mouse; the backlights; and who holds the screen, the kiosk, the Touch Display or Display Configuration plugin, an X server. From that it suggests the rotation and the pointer. The settings moved here from the System tab and gained the pointer: shown, hidden, or Auto, which shows it with a mouse and hides it with a touch panel alone, resolved by the plugin from what the player has, at every save and at every start. `screen.pointer` and `screen.pointer.shown` are the keys. The probe's parsers are tested on texts captured from a player.

## [0.7.77] - 2026-09-30

Touch on a screen with no X server. The display took its taps from the mouse events SDL makes of a finger, and on KMS/DRM none reach it, so every touch counted as a bare touch and the display left. A finger now arrives as a finger, its place a share of the window turned back the same way as the picture, and the mouse SDL makes of a touch is dropped once a finger has been seen as one, so a touch counts once under X and once on KMS/DRM. A real mouse counts as before.

## [0.7.76] - 2026-09-30

The browser face tells the truth when the player stops. The persist countdown the player's screen shows after a stop or a pause now shows on the Face and on Anymote too: the plugin pushes the period as a line beside the file the display reads, and the engine counts it down on the page's own wall clock, which the page now hands it, so a theme's clocks are true there as well. When the period ends and the player's display has left the screen, the page says so in a banner over the picture, stopped or paused, until the player plays again. The red last ten seconds of a track, drawn by the engine from the position, reach the browser face now that its position is right.

## [0.7.75] - 2026-09-30

A Face, an Anymote page or a display that attached while a track played showed it as just begun, until a pause or a play brought it in line: the plugin replayed the player's state as Volumio had last pushed it, with the position of that moment. The kept state is now stamped when it arrives, and a replay hands out the position moved on by the time since, no further than the track's end.

## [0.7.74] - 2026-09-30

The screen without X. Two settings the display reads, `screen.driver` (`auto`, `x11`, `wayland`, `kmsdrm`) and `screen.rotation` (0, 90, 180, 270), set on a Screen panel of the Manager's System tab: with `kmsdrm` the display draws through the kernel's KMS/DRM directly, with no X server and no kiosk browser on the device, and turns the picture and touch by the rotation, the same angle the Touch Display plugin uses, for panels that are portrait by nature. The panel refuses KMS/DRM while the kiosk holds the screen. Both settings default to what the display did before, so nothing changes for a player that does not choose them. `GLASS_GRAB=PATH` writes the window's pixels, as shown, to a PNG after the first frames, so a screen with no X server can be checked from a terminal. Measured on a Pi 5 with the same theme and music: 903 MB used with the kiosk browser and X, 609 MB with the display alone on KMS/DRM.

## [0.7.73] - 2026-09-30

The theme collection and the fonts repository carry Glass's name: the Manager's Catalog reads `glass_templates`, and the package fetches its fonts from `glass_fonts`. Players on earlier releases keep working, since GitHub serves the old names as well.

## [0.7.72] - 2026-09-29

A cut theme's disc and reels turn about the right point: `vinyl.center` and the reels' centres are points on the screen and take the letterbox offset like the pictures' positions, where the cutter had scaled them alone, so a disc could be drawn twice, once at its place and once about the wrong centre. A theme's `screen.bgr`, which the engine puts at the screen's top left whatever the meter's place, is set on a canvas of the new size at the offset instead of scaled alone, so it stays under the theme.

## [0.7.71] - 2026-09-29

Package says what it is doing. The job row counts the meters shown as the display reports each snapshot, with a bar and the seconds to go, the theme's card shows "Packaging…" or "Cutting…" in place of the button while its job runs, and a finished package keeps a Download link in its row beside the download that starts on its own.

## [0.7.70] - 2026-09-29

Tailor and Package on the Manager's Themes tab. Tailor cuts a copy of a theme to another screen size on the player, the theme on show's size offered first, with the choice to stretch to the screen's shape instead of keeping the theme's, and installs the copy beside the original with its previews drawn. Package makes the theme's catalogue zip with a preview of every meter and hands it to the browser when it is ready. Both run as jobs the tab follows.

## [0.7.69] - 2026-09-29

A theme named by its whole path keeps its size. The screen size came from the folder setting as written, so a theme given as a path, as `--package` and `--tailor` give one, fell back to 800x480 and the package's preview showed the meters cropped. The size now comes from the last part of the path.

## [0.7.68] - 2026-09-29

The package. `glass --package --theme FOLDER|NAME --out DIR` writes a theme as the catalogue takes it: the display snapshots every meter headless, for `--settle` seconds each, tiles them into one `preview.png` in the theme's own width, and writes `DIR/<name>.zip` with the theme, its spectrum twin and the preview in the collection's layout, ready for a pull request or the Manager's upload. `--tailor` with `--package` cuts first and packages the cut. A theme given by its own path brings its spectrum twin from beside it, as a cut lays one out.

## [0.7.67] - 2026-09-29

The cutter. `glass --tailor WIDTHxHEIGHT --theme FOLDER --out DIR` writes a copy of a theme and its spectrum twin at another size: every position, size, length and font size in the two text files scaled by a table that classifies each key the parser reads, with a test that holds the parser to it; every picture resampled; comments, order and unknown keys kept and the unknown named. One scale factor keeps the theme's shape, centred, with `--stretch` for the screen's shape instead; `--from WxH` gives the size of a theme whose folder name does not. A creator shrinks a 4K theme to check it on a smaller screen, or lifts a small one, and works on the differences.

## [0.7.66] - 2026-09-29

A theme removed stays removed. The install brought PeppyMeter Screensaver's themes and the four bundled ones into Glass's folder at every install and upgrade, without overwriting, so a theme removed from Glass's folder came back at the next upgrade while the old plugin's folder still held it. Both are copied once now, when Glass's theme folder is first made. The Manager's System tab shows, while the old plugin's folder still holds themes, how many and how much space, and offers to wipe its two theme trees behind a confirmation; nothing else of that folder is touched.

## [0.7.65] - 2026-09-29

The fitted theme can be placed. Fit and position are two settings now: `position.fit = True` scales the theme to the screen with its shape kept, and `position.type` says where it goes, centred or with its top left at `position.x`, `position.y`, fitted or not; `position.type = fit` from 0.7.64 still reads as fitted and centred. The plugin's settings page has a Fit to screen switch above Meter Position, and the Manager's Appearance tab gains a Display section at its top with the same controls, applied at once. The window draws the fitted frame into a rectangle of its own and maps touches through it.

## [0.7.64] - 2026-09-29

A theme of another size fits the player's screen. `position.type = fit`, the third choice of the settings page's Meter Position, scales the theme to the screen with its shape kept and centres it, so every size in the catalogue can show on any player; touches land on the theme's controls as before. The scale is the window's own, as a remote display fits a theme, and costs the player nothing on the software path beyond what a remote pays.

## [0.7.63] - 2026-09-29

Two analyser keys close the last of the evo looks. `blend = add` makes the box's own drawing add its colour to what is under it within the box instead of covering it, so overlapping channels, a trail's wake or a glow's halo bloom and saturate as in evo's prism; the box still sits on the theme as its alpha says. `dot.hold = True` sits each dot of the dots style at the band's held peak and lets it fall with it, evo's peak constellation, with no separate peak mark. The fanart interval's help on the plugin's settings page says the seconds count from the last change and that a new track moves the picture on as well.

## [0.7.62] - 2026-09-29

The Catalog installs a theme whose zip moved on since the index was fetched. A download is checked against the index as kept on the player; when the catalogue had been updated since, the install failed with "response larger than allowed" or a size mismatch. The Manager now fetches the index again and downloads once more against the entry as it stands, and the Catalog tab refreshes an index older than ten minutes on its own.

The radial look costs less: each channel's ring is walked from its base circle to the frame's furthest tip instead of every pixel of the circle, and an angle's band comes from a table built once per layout.

## [0.7.61] - 2026-09-29

The fanart slideshow keeps its interval. The check that moves the picture on ran only when the player pushed a state, so a picture changed at a track change, a pause or a volume step and not every interval; the display now gives the slideshow the player's artist and track once a second between states, and the interval, the order and the transition run as set.

## [0.7.60] - 2026-09-29

The Face shows the next track. The plugin pushes the player's queue down its channel, `{"kind":"queue","items":[...]}` on connect and on every change, and a display takes the track after the playing one and the queue's length from it; the Face, which cannot ask the player itself, had nothing to draw in a theme's next-track rows. The player's own display and remotes read the same line, so a reordered queue moves their next line at once, and a display asks the player for the queue only while it has not been given one.

## [0.7.59] - 2026-09-29

A meter shows several spectrum boxes. `spectrum.name` takes a comma list of sections, one box each in that order, `spectrum.size` for all of them or `spectrum.2.size`, `spectrum.3.size` and so on for a box of its own size. A spectrum section says which channel it draws with `channel = left`, `right` or `mean` when the analyser's layout is `single`, so a classic layout with a face per channel takes a spectrum per face, and the Manager measures two channels when a section asks for one.

## [0.7.58] - 2026-09-29

The Manager removes any theme its Themes tab lists. A folder whose name has no underscore, `1280x400` for one, was listed but refused with "The selected theme cannot be removed"; the removal now accepts every folder the tab shows, hidden ones aside, and counts them the same way when it keeps the last theme.

## [0.7.57] - 2026-09-29

A graph's line, its ribbon width and its glow keep their width across the line on a slope: the column's span grows by the slope's secant, so a steep run reads as wide as a flat one instead of thinning to a thread.

## [0.7.56] - 2026-09-29

Effects that work across the analyser's styles, the organic and atmosphere looks of the evo framework as keys: `trail` keeps a fading wake of the last frame under the new one; `bar.glow` puts a soft halo behind each bar and `bar.fade` dims a bar at its base and brightens it at its tip; `sparkle` throws specks above loud bars; `line.width.max` makes a graph's line a ribbon whose width follows the level and `line.glow` a soft band around it; `echo` draws a ghost of the levels that follows them slowly, a mark per bar or a line of its own on a graph.

## [0.7.55] - 2026-09-29

Two more analyser styles. `style = dots` draws a disc per band at its level, `dot.size` pixels across, the peaks as smaller discs. `style = waterfall` draws a spectrogram: each frame's levels become a row of colour at the base of the box, the palette by level, or the band's colour at the level's opacity by index, and the rows move away `waterfall.speed` rows a frame towards the far edge, or from the far edge towards the base with `waterfall.reverse`; the box keeps as many frames as it has rows. Both take the layouts, the mirror and the onset flash.

## [0.7.54] - 2026-09-29

The analyser answers onsets, a band group rising after quiet as the bank hears it: `onset = flash` brightens that group's bars towards `onset.color` and fades them back, `pulse` grows every bar by up to three tenths and settles it back, `ring` sends a line from the base to the top of the box, a ring out from the base circle in the radial look, fading as it goes. `onset.decay` says how long, `onset.strength` how much, `onset.groups` which of sub-bass, bass, mid and high fire it. This look is Glass's own.

## [0.7.53] - 2026-09-29

The analyser's graph style: `style = graph` joins the band levels into a line across the box, the area under it filled at `fill.alpha` in the palette and the line drawn `line.width` pixels thick; `peaks.line` joins the peaks into a line of their own. Scales: `scale.x` labels the frequencies in a strip under the bars (`note.labels` names the notes instead), `scale.y` labels the decibels at the left with faint lines across, both in the theme's regular font at `scale.size` in `scale.color`. The radial look costs half of what it did: a pixel outside a channel's ring is passed over before its band is worked out.

## [0.7.52] - 2026-09-29

The analyser's radial look: `radial = True` draws the bands round a circle instead of along a baseline, the bars growing out from a base circle of `radius` (a share of the box's radius, 0.3 unless said) to the rim, or, with `radial.invert`, in from the rim towards the centre. `spin` turns the whole picture, in revolutions a minute, clockwise for positive values. The layouts follow: `dual-vertical` puts the left channel outside the base circle and the right inside it, `dual-horizontal` gives each channel half the circle, `dual-combined` lays them over each other; `mirror` runs the bands over half the turn and back. The palettes, colour modes, peaks, `bar.space` and `alpha` apply as before; LEDs, luminance, outlines, rounding and the reflection do not, as in audioMotion. A frame of 256 bands on two channels costs what the other looks do.

## [0.7.51] - 2026-09-28

The analyser's rounded and outlined bars cost what plain bars do: the straight body of a bar is drawn as rectangles and only the rows the corners and the end lines touch are drawn one by one, on the same whole-pixel edges, so a meter of outlined bars that took a whole core of a Raspberry Pi 5 at 60 frames a second takes a fraction of it. A button with three pictures or more shows one per state of its action, in the order its indicator uses: repeat off, all, single and infinity; mute off, muted and zero; random off and on; play, pause, stop and toggle by the play state, stop, pause, play. Two pictures keep their meaning, rest and active. The onset bits the bank works out reach the ring and the wire, and one-bit audio is flagged on the wire, as the contract says. `volume.value.font` is loaded and used. A `single` layout over a stereo bank draws the mean of the two channels. The manager serves a track's pictures only from the folder of the track the player reports.

## [0.7.50] - 2026-09-28

Two keys for a theme's strip: `playinfo.type.label = samplerate` puts the sample rate line beside the format's icon as one centred unit, and `fanart.scale = cover` (`folderlayer.N.scale` too) fills a box keeping the picture's shape, the middle cut out.

## [0.7.49] - 2026-09-28

The analyser draws its bars straight into the frame from a palette lookup, forty times cheaper than the shapes of 0.7.48: a dense look of 256 bands on two channels costs a couple of milliseconds a frame instead of a whole core. Its level range now defaults to -60 to 0 dB, the bank's own scale, where a full-scale sine reads 0 dB in its band. A theme may write the volume as a number beside its gauge with `volume.value.pos`.

## [0.7.48] - 2026-09-28

The analyser: a spectrum section with `style = bars` is drawn as anti-aliased bars from a palette, with peaks that hold and fall or fade, in one of four channel layouts (single, dual-vertical, dual-horizontal, dual-combined), mirrored, reflected, as LEDs, as full-height luminance bars, as outlines, rounded, with each bar's opacity its level, coloured along the bar, by its place or by its level, over a background of any alpha, weighted by the A, B, C, D or ITU-R 468 curve, on a decibel or a linear scale over any frequency range. The keys follow audioMotion-analyzer's controls, name for name with the same defaults, so a configuration ports across; ten palettes ship by name and a theme writes its own as stops. A development reference theme in the catalog, `1280x720_glass_analyser`, shows four looks with the cover, a fanart slot, the track texts and time, the progress and volume sliders and the buttons, for authors to start from.

## [0.7.47] - 2026-09-28

The spectrum measured as a bank, stereo from the tap to every display. The tap runs an overlapped FFT, a window of up to 16384 samples every 1024, and projects it onto the bands a theme asks for, up to 256 per channel over 20 Hz to 20 kHz on a log, mel or linear scale, with a peak hold per band and an onset per band group; the analyser is the `bank` crate, a port of the evo framework's terminus analyser under its own Apache-2.0 licence. A spectrum theme says what it wants with `bins`, `channels`, `scale` and `window` in its sections; the plugin writes the demand of the theme on show beside the rings and the tap follows it within a second. Themes of the previous engine keep their look: their bars are regrouped from the bank on the old logarithmic mapping. The ring is version 2 and the wire protocol 2, carrying the bank per channel and its hold in place of the raw spectrum of the channels' average; remotes upgrade together with their player. An audio process that still runs the previous tap keeps its meters: its ring is read and its raw spectrum projected onto the bank until the process restarts. The ALSA key `fft_size` is accepted and no longer read.

## [0.7.46] - 2026-09-28

The face draws the pictures: the album art, the artist fanart with its slideshow, and the pictures a theme takes from the track's folder, the record and the reels included. The module lists what it wants, the page fetches each picture through the manager and hands it in, or says the manager has none, and the manager fetches only what the player itself reports for the playing track. Underneath, every picture a scene names is decoded and kept by one type in the raster crate, shared by the display and the module, so the two cannot drift again. The SVG icons are drawn without the text engine, which the format icons never used: the display, the remotes and the browser module are smaller for it.

## [0.7.45] - 2026-09-28

The face draws the track's type icon. The browser module brought the icons and found the right one, then painted the label in its place: the display's icon decoding was in the display binary alone. It is now a type in the raster crate the display and the module share, so the Face tab and Anymote show the same icon the player's screen does, in the theme's colour.

## [0.7.44] - 2026-09-28

A button with an active look, asked for by a theme author: `button.<name>.image = rest.png, active.png` draws the second picture while the button is active, which its action decides (play, pause and stop by the player's state, toggle while playing, mute, random and repeat while on) or while a finger is on it (next, previous, meter.next, meter.previous, dismiss), on the player's screen, on remotes and in the browser alike. On Anymote the first tap anywhere takes the screen, and Escape is respected. Anymote introduces itself as any remote, a remote from anywhere.

## [0.7.43] - 2026-09-28

The Status tab as a system sheet: sections for the player, the screen, the audio path, the themes, the network and remotes, the face and Anymote, and the housekeeping, with Volumio's version, the board, the build the zip carries, the screen's size beside the theme's, whether the tap heads the ALSA chain, the last upgrade, the newest backup and the last three warnings or errors from the journal, so a screenshot or its text answers a support question in one go. "Copy as text" puts the sheet on the clipboard; the addresses, with the manager's and Anymote's links in their address form, stay hidden until "Reveal addresses". The face keeps the player's state across a page refresh: lines that arrive before the module is up are kept for it, and a new page gets what the plugin holds even before any push; the face starts on the meter the player's own display shows.

## [0.7.42] - 2026-09-28

The face takes a finger, and Anymote. A tap or a drag on the theme's controls in the browser acts on the player as on its own screen: the same regions, margins, taps and drags, moved out of the display binary into a crate the display, the remotes and the browser module share; the manager runs what the page sends. Anymote, any remote, a remote from anywhere, is the face on a page of its own, `/anymote` on the manager's port, filling the window with a full-screen button, and a web manifest so a phone or a tablet keeps it on the home screen as a full-screen app. A configuration that rotates puts its first meter on show until the player says which; the player's state carries across a change of meter.

## [0.7.41] - 2026-09-28

The face in a browser, the first cut. The Manager's Face tab draws the player's meters live: the display's pipeline compiled to WebAssembly, fed the same configuration, theme, fonts and icons a remote display brings and the same frames the remotes receive, so a phone or a tablet on the network watches what the player's screen shows, following the theme and the meter as they change, full screen on a button. The theme is kept in the browser by checksum and fetched again only when it changes. The frames daemon serves its datagrams to browser pages as an event stream on a local socket and runs whenever the plugin does, the port only while remote displays are served; the manager proxies the stream and adds the player's state. Underneath, the input crates read files and the clock through the host where a target has none, and the remote's hop conditioning and configuration rewriting are shared with the module.

## [0.7.40] - 2026-09-28

Controls under a finger. Every control's touch region is at least 48 pixels on each axis, centred on what is drawn, so a thin bar or a small light answers to a finger a little off it; a tap is tested against the drawn boxes first, then the nearest grown one, and a bar reaches half a margin past both ends so 0 and 100 can be hit; `touch.margin` in a meter tunes it, 0 keeps the drawn boxes. A finger down on the volume or progress bar drags it: the knob follows the finger, the volume goes to the player as it moves and the seek when the finger lifts.

## [0.7.39] - 2026-09-28

Interactive controls, the first step of the face. A tap on a theme's play state, mute, shuffle or repeat indicator acts on the player, a tap on its volume or progress bar sets the volume or seeks to where the tap lands, and a theme draws buttons of its own with `button.<name>.pos`, `size` or `image`, and `action` (toggle, play, pause, stop, next, previous, meter.next, meter.previous, mute, random, repeat, dismiss). A meter says it is meant for fingers with `interactive = True`, or has buttons; the display's setting, on the settings page and the Manager's System tab, takes the theme's word, or turns the controls on or off for every theme. A tap outside a control does what the touch settings say, as before. Remotes act the same way through the channel; their settings follow the player's.

## [0.7.38] - 2026-09-28

Car Dash by the sun. Beside the two times, the switch can follow sunrise and sunset, reckoned by the player itself for the place of its time zone, read from the system's zone table, or for a latitude and longitude of your own; minutes after sunset and before sunrise on top, and the two times standing in on a day without either. Nothing is fetched from the network.

## [0.7.37] - 2026-09-28

Car Dash: two themes by the clock. On the Manager's Themes tab, a day theme from one time and a night theme from another; the player puts the right one on show at those times and after a start, from its own clock, with no scheduler outside the plugin. Remotes that follow the player follow the switch. For a screen in a car, or a dark theme for the evening in a room.

## [0.7.36] - 2026-09-27

A full screen remote can be set up from the machine it covers. Escape now leaves full screen and keeps the remote showing in a window, so its settings page can be used in a browser beside it; F takes the screen again; Q quits, as Escape did before. The page has the same two buttons under Display, for a touch screen. The change is for the run; the Display setting says what the next start takes.

## [0.7.35] - 2026-09-27

The remote's page says when the remote is gone. A button pressed after the remote stopped (Escape or Q closes its window) or after its page moved used to show the browser's own words, "NetworkError when attempting to fetch resource"; the page now says the remote did not answer and asks for it to be started again and the page reloaded, in the banner and in the toast.

## [0.7.34] - 2026-09-27

The Linux archives of the display are small again. Since 0.7.20 they carried the Android build's leavings from the same checkout, 80 to 97 MB in place of 4 to 5; the archive now takes the display, the tap, the remote installer and its README by name, and the release fails on an archive above 16 MB.

## [0.7.33] - 2026-09-27

The remote's status screen, the one that says where the settings page is or what it waits for, is set in a face of the machine's own, Roboto on Android and the system's sans elsewhere, in place of the bitmap face. On Android the first-run screen names the device's real address for its page, read from the interface list, where it said localhost.

## [0.7.32] - 2026-09-27

The remote's page rounds out its settings. The configuration downloads as a file and a downloaded one uploads and applies at once, so a remote's settings move to another machine. The log level of the remote is set on the page and applies at once, over the environment's. A spectrum decay of the remote's own lets the bars fall by at most a share of their height per frame, as Peppy Remote's decay rate did; off, the bars show as the player sends them.

## [0.7.31] - 2026-09-27

Themes from the remote's own machine. A remote's page takes a themes folder on that machine, a disk of its own or a share mounted there (theme folders, or a `templates` folder with `templates_spectrum` beside it, as the player's data folder is laid out), reads it, and a player's theme choice can be one of those themes with the same meter selection as an own theme of the player's; nothing is brought from the player for it but the fonts and icons. The Display section chooses the screen the window opens on, on a machine with more than one.

## [0.7.30] - 2026-09-27

A fuller Status tab: the theme folders with how many themes and spectrum twins each holds, the face each text style is set in and how many fonts were uploaded, whether the themes are editable over the network share, the artist fanart settings with the artists cached, and the performance profile with its frame rate, the governor and the board.

## [0.7.29] - 2026-09-27

Glass keeps its themes in its own folder. A player that took its themes over from PeppyMeter Screensaver in place, and still read them from that plugin's folder under Internal Storage, has them copied into Glass's folder at the plugin's start, once, and its configuration pointed there; the old folder is left exactly as it was, for a return to that plugin. The installer copies too and no longer moves a preserved folder. The share panel and the guide name Glass's folders only.

## [0.7.28] - 2026-09-27

Text sits where the Python engine set it. A font size is the em in pixels, as FreeType sizes a face for pygame, and the baseline sits the face's ascent below the line's top, rounded up to a whole pixel; the line is ascent to descent high. The raster had taken the size as the face's ascent-to-descent height, which is the em for Lato and DSEG7 but 1.227 em for PeppyFont, so text set in the built-in faces was drawn smaller and higher than the same theme under PeppyMeter. A character taken from the fallback face is set at the same em as the text.

## [0.7.27] - 2026-09-27

Fonts of your own. The Manager's Appearance tab takes a font file, TrueType or OpenType, and keeps it on the player, and each text style, light, regular, bold, italic and the clock, is set in a face of your choice: one of the built-in multi-script faces, one of the player's own fonts (Volumio's Lato), or one you uploaded. A remote brings uploaded fonts from its player with the other fonts and sets its text the same way. In the meter configuration a style's `font.<style>` value is now `builtin`, a file name under `font.path`, or the path of an uploaded font; an older configuration's `use.system.fonts` is read once and folded into those values. The one switch of 0.7.25 became the per-style choice.

## [0.7.26] - 2026-09-27

Themes can be edited in place from a computer on the network again. Volumio shares its Internal Storage folder, where the theme folders live, and a switch on the Manager's System tab, "Let computers on the network edit the themes", makes every theme folder writable over that share, now and for themes installed later; off, the folders go back to the player's own permissions and can be read over the share but not changed. The old plugin had this switch and it had not come across; the setting a player took over from it applies.

## [0.7.25] - 2026-09-27

Text in every script again. The plugin carries the whole PeppyFont set once more, Light, Regular, Bold and Italic, multi-script faces that cover Latin, Cyrillic, Greek, Arabic, Hebrew, Thai, the Indic scripts and the Chinese, Japanese and Korean ones, and the display follows the rule the configuration always had: with `use.system.fonts` false, the default, the light, regular, bold and italic styles are set in PeppyFont; true keeps the fonts `font.path`, `font.light`, `font.regular` and `font.bold` name, Volumio's Lato on a player. A switch on the settings page's Theme section sets it. Since 0.5.0 only the italic face had shipped and the rule had gone unread, so a title in Chinese, Japanese or Korean lost its glyphs in three of the four styles. New with it: a fallback face consulted glyph by glyph, so a line that mixes scripts, a Latin artist with a Japanese title, or a theme's own font that lacks a character, is set whole, the missing glyphs taken from PeppyFont Regular. The three weights are fetched at packaging from the peppy_fonts repository at a pinned commit and checked by digest; a remote brings them from its player like the other fonts.

## [0.7.24] - 2026-09-27

Artist fanart shows on remotes. A remote asked the player for the artist's pictures and fetched them, but kept them under hashed names with no telling extension, and the loader the fanart slot uses chose its decoder by the file's name, so the pictures never appeared; album art took another loader and did. The loader now reads the format from the bytes, on every platform.

## [0.7.23] - 2026-09-27

The first signed release. 0.7.22 signed its binary and then refused its own signature: the check ran on a Linux runner without Microsoft's root certificates, so the chain could not be walked there, and the release was not published. The check now looks for what the runner can see, a signature issued by Microsoft's public code signing authority and a timestamp, and leaves the chain to Windows.

## [0.7.22] - 2026-09-27

The Windows binary and its installer scripts are signed. `glass.exe`, `install.ps1` and `uninstall.ps1` carry an Authenticode signature issued through Azure Artifact Signing under Microsoft's public root and timestamped, so Windows knows the publisher: SmartScreen does not ask, and Smart App Control lets the program run. The file's Properties show the signature on its Digital Signatures tab.

## [0.7.21] - 2026-09-27

The release workflow finds the Android NDK on the runner; 0.7.20's release did not build, so this is the first release that carries the Android app.

## [0.7.20] - 2026-09-27

Glass runs as a remote display on Android. The release carries `glass-<version>-android.apk`: the display as a library inside SDL's Android activity, run as a remote, landscape, the screen kept on, with a Wi-Fi multicast lock so players' announcements arrive. The first start shows the settings page's address on the screen, as on the other platforms; the page is reached from a browser on the phone at `http://127.0.0.1:5583/` or from any machine on the network. The configuration and what is brought from players live in the app's own storage, and the display's lines go to the system log under the tag `glass`. The display's entry point is now a library function, `glass::run`, that the `glass` binary and the Android shell (`bins/glass-android`, `libmain.so`) both call. `scripts/ship-android.sh` builds SDL2 for Android from the SDL source, pinned by checksum, the display for arm64, arm and x86_64 with cargo-ndk, and the app with Gradle; `scripts/android/Dockerfile` is the image with the Android SDK, NDK and emulator for a machine without them.

## [0.7.19] - 2026-09-27

On Windows the display is a windowed program: no console window opens behind the meters when a Start menu entry starts it. Started from a terminal, it attaches to that terminal's console first, so its lines still arrive there, and `glass.exe --help` still answers. The executable carries an icon, the remote's, which Explorer, the taskbar and the Start menu entries show, and a version block that the file's Properties show; both are compiled with windres from the cross build.

## [0.7.18] - 2026-09-27

The remote opens its window on Windows. The display opened a window only when the `DISPLAY` variable named an X11 display, which Windows does not have, so a remote there ran its session, synced the theme and followed the player, and showed nothing; a window now opens on Windows and macOS whenever `--headless` is not given, and on Linux when `DISPLAY` or `WAYLAND_DISPLAY` is set.

## [0.7.17] - 2026-09-27

A remote's settings page works again. Since 0.7.13 its script had not parsed (a parenthesis left open in the Frames row of the status), and a browser runs all of a script or none, so the page showed its form but read nothing, and finding players, adding one and Apply did nothing; the project's checks now parse the scripts of the remote's page and the manager's page. The page also says when it cannot read the remote's state, in a banner at the top with the reason, and tries again every five seconds. The remote says every request its page receives at the verbose level (`page: POST /api/player from 127.0.0.1:52140`), so a report from a machine where the page does nothing tells whether the requests arrive. The Windows installer removes the mark of the web from what it copies, so SmartScreen does not ask about the Start menu entries, and says that Windows Defender Firewall's question about private networks is to be answered yes, since players announce themselves with a broadcast; the wiki's Remotes page has the same, with what Smart App Control does to an unsigned program.

## [0.7.16] - 2026-09-27

The Windows installer finds the display in the release archive. `install.ps1` looked for `glass.exe` under `bin\windows-x64`, the layout of a checkout that ran the cross build, while the archive holds it under `bin`, so an install from the archive stopped with "glass.exe is missing"; it now takes either layout, and `-Check` only says where the files would come from, which the project's checks run against both layouts.

## [0.7.15] - 2026-09-27

Glass runs as a remote display on Windows. The release carries `glass-<version>-windows-x64.zip` with `glass.exe`, the `SDL2.dll` it loads and an installer (`remote\windows\install.ps1`) that puts both under the user's programs folder and two entries in the Start menu, Glass Remote and Glass Remote Settings, with `-Startup` for a display that starts with the session; `uninstall.ps1` takes them out. The configuration lives at `%APPDATA%\glass-remote\config.json` and what is brought from players under `%LOCALAPPDATA%\glass-remote`. The build is a cross-compilation with MinGW-w64 (`scripts/ship-windows.sh`, and `scripts/windows/Dockerfile` for a machine without MinGW) against the SDL project's MinGW package, pinned by checksum; the tap's FIFO, relay, ring and measuring thread, the player's side, are left out of the Windows binary, which shows a player's meters as a remote only.

## [0.7.14] - 2026-09-27

Text on a remote is set in the player's fonts again, and the format icon is drawn. The player's web fonts, the ones its configuration's `font.path` names, are served by the manager as assets with checksums (`/api/remote/asset/webfont/<name>`), and a remote brings them into its `webfonts` folder. Since 0.7.0 the remote had asked the player's web server for them by path and been given the web application's page instead, so the three files it kept were not fonts and its text fell back to the plugin's font. Volumio's own format icons (`mp3`, `flac`, `wav` and the rest, beside its web application) are listed with the plugin's own and served the same way, so a remote has every icon the player has; before, only the plugin's few were brought.

## [0.7.13] - 2026-09-27

The display governs its own frame rate. When frames keep overrunning their period with every painter at work, the rate steps down a ladder, 60, 45, 30, 20, 15, so a theme too heavy for the player runs smoothly at a lower rate instead of stuttering at the set one; the display says so in the journal and tells the plugin with the meter on show, the Status tab and the Performance panel say "lowered to", and a remote's page shows the rate it draws at. A step back up is tried after two minutes, and the wait doubles each time the theme proves too heavy again; every meter starts over at the set rate. A switch on the Performance panel turns it off, for a rate that must be exact; a rate asked for on the command line is exact too. `GLASS_BENCH_DELAY_MS` in the display's environment adds that many milliseconds to every frame's painting, to watch the painters grow and the governor step down on any machine.

## [0.7.12] - 2026-09-27

`glass --dev`, with or without `--remote`, opens a window at the theme's exact size, unfitted, with a title bar that names the theme and the meter on show and follows the meter as it changes, the pointer visible and Escape to close: a window to move about a desktop while a theme or a remote is reviewed. Nothing is written to the configuration; the remote's page keeps saying what the display would be without the switch.

## [0.7.11] - 2026-09-27

The manager has a System tab, before Remotes, for what acts on the player: Glass releases (upgrade and rollback), the performance profile and the logging settings with the log viewer. The Status tab is a picture of the player and nothing else. The header's release badge opens the System tab.

## [0.7.10] - 2026-09-27

Logging and performance profiles, on the Manager's Status tab. What Glass writes to the player's journal is set by a level, errors only, warnings (the default), info, verbose or trace, and at the two finest levels by targets: the audio path, the channel, the display, remotes, themes, artwork, the manager and upgrades, settings. The display's and the frames daemon's lines now reach the journal as they are written and follow the same level from their next start; before, the display's lines were kept until it left and then dropped. The panel shows the last three hundred Glass lines and downloads the last thousand as a file for a report. A performance profile sets the frame rate, the rotation quality and the transitions together: Full, Standard, Light, Minimal, or Auto, which reads the board (a Raspberry Pi 5, 4, 3 or Zero 2 W, or a PC) and picks the row for it; a value changed by hand on the settings page makes the profile Custom. An upgrade now waits until the player's plugin registry shows Glass enabled before it restarts the backend, and sets it when it does not, so an upgrade cannot leave the plugin installed and off.

## [0.7.9] - 2026-09-27

A remote shows the pictures a theme takes from the playing track's folder: folder layers, a record or reel picture from the album. The player's manager serves one picture from the track's folder on request, under the same rule the display applies on the player (the folder under `/mnt`, a plain picture name), and the remote brings the first candidate the player has into its cache home in the background, once per track folder, so a track change costs the remote no frame.

## [0.7.8] - 2026-09-27

One line installs Glass on a player: `get-glass.sh`, fetched with curl and run as the player's user, downloads the latest release (or the one named), hands it to the player's own plugin manager over its socket, enables the plugin and restarts the backend, so the command line tool's staging step, which fails when the zip sits in the folder, is not part of the way in. The damage boxes a frame paints are merged in the order of their geometry, so the same scene always paints the same boxes; they were merged in the order of their steps' keys, which carry picture identities and differ from run to run, and one order of a test scene painted a few pixels over the test's bound.

## [0.7.7] - 2026-09-27

A remote that follows the player shows the meter the player shows. The player's own display tells the plugin which meter it moves to, the plugin passes it to every remote and to a remote that connects, and a following remote switches to that meter instead of rolling its own; "Show the same meter as the player" on the remote's page, on by default, turns it off for a remote that should keep its own rotation. The Status tab names the meter on show.

## [0.7.6] - 2026-09-26

A remote keeps moving across a track change. The frames daemon numbered its datagrams with the ring's own hop count, which starts again with every stream, so after a track change that opened a new stream the remote took the new packets for old ones and dropped them until the count passed the old stream's; the player's own display, reading the ring directly, never saw it. The daemon numbers its datagrams itself now, and the remote also takes a packet whose stamp from the player is later than the last, whatever its number, so it moves with a daemon of an earlier release too.

## [0.7.5] - 2026-09-26

A remote that follows the player follows its whole configuration: a change of the meter selection, the rotation or any other value of the player's meter configuration starts the remote's session again with it, not only a change of theme; and the player sends its configuration to every display that connects to the channel, so a change made while a remote was away is taken up when it returns. The wiki is rewritten around a Quick Start, with Settings, Themes, Meters Reference, Spectrum, Catalog, Backups, Artwork, Troubleshooting and Logging pages, screenshots and diagrams. Names of the development machines, private addresses, dashes and other glyphs are gone from the code's fixtures, the plugin's strings and the pages.

## [0.7.4] - 2026-09-26

A remote's needles and bars move as the player's own do. The frames daemon put the ring's running frame count on the wire, where the field holds the frames in the hop and is sixteen bits wide, so every packet after the first second claimed 65535 frames and the remote's meter decay let the needles fall to each new peak at once. The daemon now sends the frames in the hop, and the remote times its decay by the player's stamps on the packets, which spans a dropped datagram and holds against a daemon of an earlier release. A frame of silence from the daemon is taken as a stop, so the needles fall the way they fall on the player.

## [0.7.3] - 2026-09-26

A remote display is set up on a page of its own, served by the same `glass` binary on port 5583 while it runs: the players it knows (found on the network or typed, one on show), the theme (the player's, followed as it changes, or one of the player's themes kept as the remote's own with its own meter rotation), the window (full screen with the theme fitted to the screen, a window, or a frameless one at a fixed place), the frame rate and a gain for this remote's needles and bars. A change applies while the display runs: the session starts again in the same process and the window stays up. `glass --remote` alone runs as the page says, and a first start shows the page's address on the screen; `--settings` opens the page in a browser, of a remote already running or of the one it starts. The manager's Remotes tab links to each remote's page. The release archives carry `remote/linux/`: an installer with the two desktop entries, "Glass Remote" and "Glass Remote Settings", an icon, and a user service that keeps the display running with the session. The ship script now refuses a binary that needs a glibc newer than Volumio's 2.36: the browser-opening switch had reached, through the standard library's process spawn, a symbol of glibc 2.39 that the player's loader refused, and the switch now spawns through `posix_spawnp` directly.

## [0.7.2] - 2026-09-26

The status tab says whether remote displays are served, on which ports, how many receive frames, and any problem the daemon or the channel reported.

## [0.7.1] - 2026-09-26

Serving remote displays is off until the manager's Remotes tab turns it on, and the three ports are set there, pre-filled with 5580, 5581 and 5579 and checked against each other and the manager's port; a change applies at once, and a port something else holds is reported on the tab. A player with no screen of its own says so in the display setting ("no screen of its own, remote displays only"): it opens no window and serves the remotes, PeppyMeter's server mode. A remote named by hand asks the player's manager for the ports first (`--manager-port` when the manager's port was moved), so moved ports work without a beacon.

## [0.7.0] - 2026-09-26

Remote displays: Glass on another machine shows a player's meters with the player's theme, fonts and icons, drawn by the same pipeline. `glass --remote <player>` (or `discover`) brings the player's configuration, the theme on show, the fonts and the format icons into a home of its own from the manager, each file kept by checksum; subscribes to the player's frames, which `glass-serve` on the player sends over UDP from the tap's ring, one datagram per hop with the peaks and RMS exact and the spectrum in quarter-decibel steps; hears the player over the channel on TCP and says who it is; starts itself again when the player's theme changes; and plays or pauses the player on a touch. The plugin starts the frames daemon, serves the channel over TCP, announces the player with a beacon, and shows the remotes on the manager's Remotes tab. The wire is documented on the wiki's Remotes page. The old peppy_remote is not fed: it ran the Python engine on the remote, and this is its replacement.

## [0.6.11] - 2026-09-26

The measured line reads a ring stamped a hair ahead of the manager's own clock reading as live, the display's own rule; 0.6.10 could call a playing source unmeasured for that.

## [0.6.10] - 2026-09-26

The status tab says whether what plays is measured: while the player reports play, a live ring under `/dev/shm` means the source's stream passes through the tap, and none means it plays below the tap and is heard but not measured. Each ring is listed with its stream and whether it is live or how long it has been quiet.

## [0.6.9] - 2026-09-26

Soloist Connect plays again with Glass running. 0.6.8 sent Soloist to `plug:spotify`, a `plug` put straight on the tap, and libasound's parameter negotiation there ends with an empty interval and aborts Soloist's daemon the moment it plays. Soloist stays on the player's own device, `plug:volumio`, where the tap sits and measures it. What 0.6.8 set out to fix stays fixed another way: Glass nudges Soloist after its own rewrite of the ALSA file and after every later rewrite the player makes as plugins start, and Soloist restarts its daemon only when the device it runs with differs from what the file now says, so a device sampled while the file was half written is corrected within seconds.

## [0.6.8] - 2026-09-26

Soloist Connect meters again after a backend start. Soloist chooses its ALSA device when its daemon starts, from the ALSA file as it stands; at a backend start that file is written in stages, and a device read too early (`softvolume`) sits below the tap, so Soloist's stream was heard but never measured until its plugin was restarted. With its metering flag on and a `pcm.spotify` in the file, Soloist opens `plug:spotify`, which is the tap. Glass now says metering is on after every rewrite of the ALSA file, and off when it is uninstalled; 0.5.10 had said off when it retired the old side outputs, which the tap made wrong. Soloist restarts its daemon only when the device it runs with differs.

## [0.6.7] - 2026-09-26

The settings backups an upgrade or rollback writes on its own are kept to the newest five, pruned after each one is written and when the manager starts; they are marked in their manifest, and the ones earlier releases named `before-<version>` count too. Named backups are never touched.

## [0.6.6] - 2026-09-26

"Open the manager here" opens the manager in this window, inside Volumio's own page with its title bar and way back, instead of a new tab; on the player's own display a new tab had no way to close. Volumio's interface opens every link button in a new tab, so the button now goes through the one button type the interface navigates in place. Inside Volumio's page the manager's inputs take the kiosk's on-screen keyboard, the way the radio plugin's page does.

## [0.6.5] - 2026-09-26

The settings page sets the manager's port and address. A new port is taken at once, the manager moves and the buttons follow; a port something else holds is refused with a message and the old one stays, and a manager that finds its port taken at start says so. The address is what the buttons and the status page name: empty is the player's name with `.local`, or an IP address or host name for networks where `.local` names do not resolve.

## [0.6.4] - 2026-09-26

A settings backup downloads from the manager as a zip, and a zip uploads as a backup: its manifest and files are checked the way a restore checks them before it lands, under its own name or a numbered one when taken. Settings move from one player to another this way.

## [0.6.3] - 2026-09-26

The restart after an upgrade is handed to systemd as one restart job, which it carries out on its own. The stop-then-start of 0.6.1 and 0.6.2 was issued from inside the player's service, and the stop took the process waiting to start it again with it, so the backend stayed down until started by hand. A player that upgraded to 0.6.2 that way needs one `volumio vstart`; from 0.6.3 on the backend comes back by itself. The kept-plugin zip writer no longer leaves an error listener behind per drained write.

## [0.6.2] - 2026-09-26

The manager looks for a new release on its own, a minute after it starts and once a day after, and says so in its header on every tab; the header's notice opens the status tab where the upgrade is. A job left in `restarting` for minutes no longer blocks another upgrade.

## [0.6.1] - 2026-09-26

Glass upgrades itself from the manager's status tab. The latest release is read from GitHub, at most once a day unless asked; when it is newer, one press downloads its plugin zip, checks it against the digest the release carries, writes a settings backup, keeps the installed plugin as a zip for going back, snapshots the meter and spectrum configurations, hands the zip to the player's own plugin manager (which replaces the plugin and runs its install script, keeping the settings), puts the configurations back, and restarts the player's backend so the new code loads. The page follows the job, waits for the new version to answer, and reloads. The kept version goes back the same way. `node --test` covers the version order, the release parsing and the zip writer.

## [0.6.0] - 2026-09-26

The Glass Manager: a web application the plugin serves on its own port (5582, `managerPort`) for everything that manages the display rather than operates it, opened from the settings page in the same window or a tab of its own. Installed themes are shown with previews the display draws itself, every meter of a theme rendered headless and kept until the theme's meters file or the plugin changes; a theme is put on show, its meter rotation set (all at random, a list, or one, on a timer or with the title), or removed. The theme catalog from peppy_templates is fetched with its ETag and browsed by size, kind and state; an install downloads the zip, checks its size and SHA-256 against the catalog, unpacks the folders the catalog names into the meter and spectrum trees through a staging directory that replaces a folder whole, and remembers the checksum so a changed zip shows as an update. A theme zip uploaded from the browser installs the same way, in any of the layouts the catalog holds. The artist fanart settings, the settings backups and a status page (display, player, channel, audio rings, catalog, storage) are the manager's too. The settings page keeps what operates the display and no longer holds the fanart controls, the remove control or the backup sections. The plugin reads zips itself, with no unzip in the path. Restoring a settings backup works again: since the share-access option left, the restore called a helper that no longer existed and every restore failed. `glass --thumb WIDTH` writes a thumbnail beside each snapshot.

## [0.5.13] - 2026-09-26

The tap asks its PCM how many frames stand between the last one written and the sound, from its measuring thread, and places every hop from that; the buffer-and-period estimate remains the fallback when the PCM cannot say. A player that keeps its buffer anywhere between a period and full, as one reading from a pipe does, now meters in step with the sound rather than within a period of it.

## [0.5.12] - 2026-09-26

Hops keep their cadence. When a player writes ahead of the sound, as one does while it fills its buffer or reads from a pipe, each hop now falls due no sooner than one hop after the last, so the ring fills evenly instead of in pairs; and a stream's first frames are timed from an empty buffer, so the meters start with the sound instead of a buffer's length behind it.

## [0.5.11] - 2026-09-26

The meters move as the audio does again. A player that writes a whole period at a time (MPD for local files, NAS and DLNA, the radio's aplay) handed the tap five or six hops in a burst every eighth of a second, and the display, reading the latest hop each frame, saw only one of them; with a half-second buffer the hops also ran ahead of what was heard. The tap's measuring thread now places every hop in time, from when its transfer arrived and the buffer and period the player asked for, and puts it into the ring when its audio plays. One-bit audio is measured a hop at a time too. The relay holds two thirds of a second of stereo at 384 kHz.

## [0.5.10] - 2026-09-26

The plugin sheds what the tap made redundant. The audio selection (modular alsa or DSD native) and the Spotify, Soloist, USB DAC, AirPlay and DSP switches are gone from the settings page and the configuration: every source meters through the tap on the main path, bit-perfect. The MPD side output and its include, the copies mounted over MPD's and AirPlay's configuration templates, Spotify's own PCM and Soloist's metering device are taken back once on the first start after the upgrade, so Spotify, AirPlay and Soloist play to `volumio` again, and on uninstall. The Dummy and Loopback cards are no longer loaded, and the display starts for every service that plays. The ALSA template no longer varies with the settings.

## [0.5.9] - 2026-09-26

Meters for every source, bit-perfect. The tap is an ALSA PCM plugin now (`type glasstap`), at the head of the Glass section for every source: the stream passes through it byte for byte in the format the player and the output agreed, so PCM, DoP and native DSD stay bit-perfect, and it is measured on the way, linear audio for peak, RMS and spectrum, one-bit audio by density. The audio thread only copies and hands the samples across a lock-free relay; a thread of the tap's own measures and publishes. FM and DAB, Tidal Connect, Bluetooth and anything else that plays through `volumio` meter as MPD, Spotify and AirPlay did, in both audio selections and on x64 as on the Pi; the MPD side output is off and the loopback card unused. `scripts/tap_exact.sh` plays raw frames of each format through the tap into a file and proves the bytes; CI runs it. Ring files carry a writer number after the process id.

## [0.5.8] - 2026-09-26

The guard holds. When Glass refuses to start because PeppyMeter Screensaver is enabled, it disables itself, so the two are never both enabled at the next start and the ALSA chain is built without Glass until the switch is made. Glass starts after PeppyMeter Screensaver at boot, so when both are found enabled the transition release steps aside and Glass runs. The guard's messages name PeppyMeter Screensaver where they said Glass, and the German and French files carry their own words where they carried English. PeppyMeter Screensaver 3.5.0, its transition release, mirrors the guard.

## [0.5.7] - 2026-09-26

The channel. The plugin serves a local socket with the player's state and the infinity playback flag, pushed as they change, and takes commands for the player back. The display reads its state from the channel when the plugin runs it, so a track change shows at once and the player is no longer asked once a second; without a channel it asks the player as before. Infinity playback shows on the repeat indicator's fourth state, or on the shuffle indicator's third. The wiki's Contracts page has the lines.

## [0.5.6] - 2026-09-26

The cross builds survive a restored CI cache, which keeps the links of the unpacked sysroot and drops the files behind them: the ship script looks for each target's SDL and ALSA library at its own path and unpacks it again when the file is gone, and the tap's build script runs again when the library it links appears, changes or goes.

## [0.5.5] - 2026-09-26

The chain names the tap. The ALSA contribution puts `glasstap` on the meter PCMs, the display reads the tap's ring instead of the two pipes, and nothing of peppyalsa is left: no library in the package, no pipes made or held by the plugin, no peppy names in the PCMs or the MPD side output, which is `mpd_glass` now and is rewritten on the first start after this release. The meter keeps its fall and the spectrum its bins and smoothing, made in the display from the raw measurements. `tapdump` stays as the way to look at the tap.

## [0.5.4] - 2026-09-26

The release and CI workflows check out the peppyalsa builds the plugin still carries, run the scripts as executables, and use the actions' Node 24 releases; the packaging refuses to make a zip without the ALSA scope for an architecture.

## [0.5.3] - 2026-09-26

The cross builds in CI get the C library headers of each target, which the TLS dependency's C sources need.

## [0.5.2] - 2026-09-26

The workshop. The toolchain is pinned in `rust-toolchain.toml`, the workspace shares one set of lints, the code is formatted by rustfmt and clean under clippy with warnings denied, and `scripts/check.sh` runs formatting, lints, tests and documentation the way CI does. GitHub Actions runs that check and the cross builds on every push, and publishes the plugin zip and per-architecture archives of the display and the tap on every version tag, with the changelog section as the notes. `scripts/package.sh` uses npm when the machine has it and a container otherwise.

## [0.5.1] - 2026-09-26

The audio tap. `glasstap` is an ALSA scope of Glass's own: loaded by the audio player's ALSA chain in place of peppyalsa, it measures the stream a hop at a time, the peak and RMS of each channel and the magnitude spectrum of each channel through a 2048-point window by default, and publishes them into a shared ring under `/dev/shm`, one file per writing process, which the display reads at its own frame rate without anything blocking the player. DSD over PCM is measured by bit density as before. When the configuration names them, the tap also writes the two FIFOs peppyalsa wrote, so both can run side by side; in this release the chain still names peppyalsa and the tap ships beside it. `tapdump` prints what the live ring says. The `tap` crate holds the ring contract, the measurements and their tests.

The display binaries, the tap library and `tapdump` are no longer committed; `scripts/ship.sh` builds them and the plugin zip carries them.

## [0.5.0] - 2026-09-26

Glass is a Volumio plugin. The `plugin` directory holds it and `scripts/package.sh` builds the zip Volumio installs, with the display binaries, the ALSA scope and the node modules. The plugin keeps the audio path up, starts the display after the screensaver timeout while music plays, keeps it up through a pause for the persist time, and serves the settings page, the artist fanart cascade and the settings backups. Glass replaces PeppyMeter Screensaver: the installer refuses while that plugin is enabled, the plugin refuses to start while it is enabled and offers to disable it, and themes and settings are taken over from an installed or a preserved PeppyMeter Screensaver. The bundled themes are the PeppyMeter and PeppySpectrum defaults.

The display reads its configuration from the plugin's home, `GLASS_HOME`, by default `/data/plugins/user_interface/glass`: `config/meter.txt` and `config/spectrum.txt`, with `fonts` and `format-icons` beside them. The run contract's files are `/tmp/glass_running`, `/tmp/glass_dismiss` and `/tmp/glass_persist`, the dismiss marker's variable `GLASS_DISMISS_FILE`, the fade lock `glass_fade_lock`, and the fanart endpoint `glass_artistfanart`.

## [0.4.28] - 2026-09-25

Less memory. Font files are mapped rather than read, so a face costs the pages its glyphs touch and a file named by several styles is mapped once; a turned picture not shown for ten seconds is let go and the turned pictures share 16 MB; the meter foreground keeps only the pixels of its opaque spans. On a Pi 5 the Thorens turntable's resident set went from 125 MB to 87 MB. With `GLASS_PROFILE=1` Glass says what each store holds.

## [0.4.27] - 2026-09-25

Only what changed is painted. A frame's drawing steps are keyed, the boxes of the steps that differ from the last frame's are the only part of the canvas repainted, and only those boxes are uploaded to the window's texture; a frame with nothing changed is not uploaded at all. The result is pixel for pixel the frame a whole repaint gives. A turning picture's box is measured from where it has pixels, so a round record or reel in a square picture claims a box its own size. The profile line says how much of the frame was painted and in how many boxes.

## [0.4.26] - 2026-09-25

A frame is planned, then painted. The drawing steps of a frame are laid out first as read-only operations, and the canvas is painted in row bands that several threads share. By default a frame takes from one thread up to one a core as it needs: painting that overruns its share of the period gets another thread, painting that would fit in half the period with one fewer gives it back. `--threads N` fixes the count. Lines set in type for the type area and the gauges are kept between frames. The turned pictures kept for needles and the tonearm share a byte budget. The screen and face pictures are dropped once the base is composed. Glass says which SDL renderer draws the window at start.

Fixed: the glow of a picture indicator was composed onto black and darkened its surroundings.

## [0.4.25] - 2026-09-25

Fixed: a picture kept turned (a needle or the tonearm) keeps the alpha of its soft edges. The kept copy was composed onto black, so a tonearm with a translucent glow or shadow drew a black surface around the arm since 0.4.23, and needles with soft edges carried a dark fringe since 0.4.20.

## [0.4.24] - 2026-09-25

Fixed: the spectrum theme is taken from the folder that carries the meter theme's own name, falling back to the spectrum configuration's `spectrum.folder`. A meter shown through `--theme`, or before the player has mirrored a theme change into the spectrum configuration, drew no spectrum.

## [0.4.23] - 2026-09-25

Turning pictures cost less. A turned picture visits only the span each frame row covers; records, reels, turning art and knobs take the nearest texel in fixed point, as the engine's plain rotation does, while needles and the tonearm keep bilinear sampling; the tonearm is kept turned per half degree like the needles. On the Pi 5 the Pioneer Gold turntable went from 55 to 25 percent of one core at 30 frames a second and holds 60 frames a second at 41 percent. `--fps N` stands in for `frame.rate` for one run.

## [0.4.22] - 2026-09-25

Theme review. `--list` prints every installed theme with its meters. `--theme FOLDER`, `--meter NAME|random|a,b,c` and `--interval SECONDS` stand in for the installed configuration's theme, meter and rotation without changing the player's files. `--snapshot DIR` shows each meter of the theme, or of the `--meter` list, for `--settle` seconds (default 4) and writes `DIR/<theme>/<meter>.png` before moving on, with or without a window. `--output` writes PNG when the name ends in `.png`.

## [0.4.21] - 2026-09-25

The plugin's run contract. When a window is open the player stands up the run flag `/tmp/peppyrunning` and leaves, fading out when it faded in, within half a second of the plugin removing it, taking the flag down as it goes. With `exit.on.touch` or `stop.display.on.touch`, a lifted finger or mouse button ends the player, writing the marker named in `PEPPY_USER_DISMISS_FILE` first when the run flag still stands, so the plugin re-arms its timeout instead of restarting. `position.type` other than `center` puts the frame's top left at `position.x`, `position.y`; the pointer is hidden. `plugin/run_glass.sh` names the dismiss marker as the engine's launcher does.

## [0.4.20] - 2026-09-25

Performance. The screen picture and meter face are composed once per meter and copied into a frame buffer that is kept between frames; rendered text lines are kept per position until the text, style, size, colour or font changes; needle sprites are kept turned per half degree, so a holding or slowly moving needle costs one plain blit; the meter foreground blends only the opaque span of each row; the window keeps one streaming texture instead of making one per frame. On a Raspberry Pi 5 with a 32-bit system at 1280 by 720 and 30 frames a second, the player's CPU share fell from about 36 percent to about 10 percent of one core. `GLASS_PROFILE=1` prints, every 60 frames, how long the poll, the step, each stage of the raster and the upload take.

## [0.4.19] - 2026-09-25

Start and stop. With `start.animation` the first frame fades in from `transition.color` over `transition.duration` seconds at `transition.opacity`, unless `transition.type` is `none`; every later meter of a rotation fades in the same way; a fade lock file shared with the player's engine keeps two starts within the fade time from fading twice. When the window closes after a fade in, the frame fades out before the player exits. After every start the levels and spectrum bars rise to their values over 0.7 seconds in ten steps, as the engine raises its full scale.

## [0.4.18] - 2026-09-25

Indicators, under `config.extend`. `mute.*`, `shuffle.*`, `repeat.*` and `playstate.*` show a state as an LED (`*.led` size, `*.led.shape` circle or rect, `*.led.color` one triple per state) or as one picture per state (`*.icon`), with an optional glow (`*.led.glow` or `*.icon.glow` radius, `.intensity`, `.color` per state) blurred behind them; a state past the look's last takes the last. Mute is off, muted, or zero volume; shuffle follows random; repeat is off, all, single, with a fourth state for infinity when the look has one; play state is stop, pause, play. `volume.*` and `progress.*` are gauges in the styles `numeric`, `slider` (a rounded bar, or a picture tip on a track with an optional fill tail), `knob` (a picture turned between two angles) and `arc` (a solid ring sector), with markers (`progress.marker.N.*`, picture or label at a percentage) and a head picture that moves with the value. The progress bar draws `progress.border` in `progress.border.color`. The player's controls (volume, mute, random, repeat, repeat single) come with the state.

## [0.4.17] - 2026-09-25

Cassettes. `reel.left.*` and `reel.right.*` (a theme picture or `album,theme` to prefer a file from the track's folder, scaled to the theme reel's size, with `center`) turn at `reel.rotation.speed` while the player plays or a stop is only a transition, in `reel.direction` (the meter's, else the player's, default counter-clockwise). Under `rotation.quality = custom` the player's `spool.left.speed` and `spool.right.speed` scale each reel. With `spool.adaptive` (the meter's or the player's) the supply reel speeds up and the take-up reel slows down as the tape runs, from half to one and a half times the speed. `queue.mode = queue` makes the progress that reels and the tonearm follow run over the whole queue, from the player's queue read every ten seconds, unless the source is a stream.

## [0.4.16] - 2026-09-25

Turntables. `vinyl.filename` (a theme picture, or `album,theme` to prefer a file from the track's folder), `vinyl.pos`, `vinyl.center`, `vinyl.dimension` and `vinyl.direction` put a record under the art that turns at `albumart.rotation.speed` while the player plays, keeps turning through a transitional stop and while the tonearm moves, and slows to a halt over the tonearm's lift after playback stops. `albumart.rotation` turns the art with the record on the record's centre, cut to a circle when it has no mask, with a spindle and ring in `font.color` and a round border. `tonearm.*` places an arm that drops onto the record over `tonearm.drop.duration`, follows the track from `tonearm.angle.start` to `tonearm.angle.end`, lifts over `tonearm.lift.duration` when playback stops, a second and a half before the end, or on a jump, and drops again where the track now is. `rotation.quality`, `rotation.fps` and `rotation.speed` from the player pace and scale the turning; a tonearm with a single reel and no record turns the reel as the record. Needles, records and arms all turn through one routine that maps a pivot in the picture onto a point on screen.

## [0.4.15] - 2026-09-25

Artist fanart. A meter with `fanart.pos` and `fanart.dimension` (plus `fanart.scale` and `fanart.zorder`, default `fit` and `background`) shows the pictures the player resolves for the playing artist through its `peppy_screensaver_artistfanart` endpoint, read from the player's own cache when it is there. The set is asked for when the artist changes and again on every track and every timed interval; the show moves one picture on per track and per interval, in order or at random as the player is set, remembering its place per artist and set. `fade` and `merge` transitions run over the player's duration; `none` cuts. Pictures for fanart and folder layers are now decoded off the frame loop, so a large picture never stalls a frame.

## [0.4.14] - 2026-09-25

Folder layers. `folderlayer.1.*` to `folderlayer.5.*`, and the legacy `folderlayer.*` under `folderlayer.enabled`, each with `pos` and `dimension`, show the first of their `files` found in the playing track's folder under `/mnt`, kept in proportion and centred (`scale = fit`) or stretched, with an optional `border` in `font.color`. `zorder = background` draws the picture over the screen picture and meter face and under the needles; `overlay` draws it over the texts and under the meter foreground. The layers follow the track folder, looked up once per folder. The meter face is now composed before the album art, and the meter foreground is drawn last of all, as the meter engine orders them. GIF and WebP pictures decode.

## [0.4.13] - 2026-09-25

Spectrum analyser. A meter with `config.extend` and `spectrum.visible` shows the spectrum `spectrum.name` names in the theme's `spectrum.txt`, in a `spectrum.size` box placed by the section's `spectrum.x/y` and clipped to it. Backgrounds, bars and reflections come from `color`, `gradient` (first colour at the bottom), `image` or `image.extended` fills; the background and `fgr.filename` are centred in the box. Bars rise from `origin.x/y` in `bar.height / steps` pixel steps, a raw bin rounded up to a whole step against `max.value`; reflections hang `reflection.gap` below the baseline; toppings stay a step above a dropping bar and fall `topping.step` pixels a frame. The pipe record length follows the spectrum configuration's `size`.

## [0.4.12] - 2026-09-25

Meter types. A circular meter may have one channel (`channels = 1`, needle at `mono.origin.x/y` for the mono level), per-channel angles (`left.start.angle`, `left.stop.angle`, `right.start.angle`, `right.stop.angle`, the left pair standing in when the shared `start.angle` and `stop.angle` are absent) and a mirrored needle per channel (`left.needle.flip`, `right.needle.flip`). Origins may lie off screen. A linear meter (`meter.type = linear`) shows the indicator picture up to the width its steps reach: `position.regular` steps of `step.width.regular` pixels, then `position.overload` steps of `step.width.overload`; `direction` is `left-right`, `right-left`, `bottom-top`, `top-bottom`, `edges-center` or `center-edges`; `indicator.type = single` moves the whole picture instead; `flip.left.x` and `flip.right.x` mirror a channel's picture; `mono.x/y` places a one-channel bar. `meter.visible = False` now hides bars as well as needles. A frame written with `--output` is renamed into place, never seen half written.

## [0.4.11] - 2026-09-25

Time fields and the data source. `time.remaining`, `time.elapsed` and `time.total` each take a position with an optional style word, a colour (elapsed and total default to the remaining colour), and for the clock style a font of their own through `time.*.font`, found as an absolute path, in the theme folder or under `font.path`, and `time.*.fontsize`. Elapsed and total show `mm:ss` of the position and the length. While the display persists after a pause, the plugin's persist file drives the remaining field: `countdown` counts the persist period down in orange, `freeze` keeps the track time. The pipe levels are conditioned as the meter engine conditions them, from `[data.source]`: full scales, `volume.gain.db` and the live `volume.gain.db.source` file, the stereo algorithm, the smoothing buffer and the mono algorithm.

Meter selection. `meter = random` walks every section of the theme's meters file, each once before starting over; a comma list cycles in order. The next meter comes after `random.meter.interval` seconds, or with the next title when `random.change.title` is set. `meter.visible = False` under `config.extend` hides the needles.

The remaining time counts whole seconds of position before subtracting, as the player does.

## [0.4.10] - 2026-09-25

Text moves as the player moves it. Every title, artist, album and next line has a box: its own `maxwidth`, else `playinfo.maxwidth`, else the width left on screen minus a margin, or six tenths of the screen when `playinfo.align` (or the legacy `playinfo.center`) centres. A text that fits is placed by the alignment; a wider one bounces between its ends with a pause, at the speed the player's `scrolling.mode` selects: 40 everywhere, the player's custom values, or the meter's `playinfo.scrolling.speed.*`. The ticker (`playinfo.ticker.*`) composes one looping line from artist, title, album and the next track, in its direction, with its separator, spacing and end gap, capped to the visible width, and can replace the separate lines. `playinfo.next.title/artist/album.*` show the track after the current one, read from the player's queue. The `italic` style uses `font.italic` with `font.size.italic`, falling back to regular.

## [0.4.9] - 2026-09-25

The type area: `playinfo.type.pos`, `dimension`, `mode` (`icon`, `text`, `both`, the meter's value before the player's `[current]` setting), `align`, `color` and `fontsize`, with the icon resolved from the theme's `format-icons`, then the player's own set, then Volumio's stock set, case-insensitive. SVG icons are rendered with resvg and tinted with the type colour; PNG icons keep their colours. Album art honours `albumart.mask` (white cut away) and `albumart.border` (drawn in `font.color`). The samplerate line falls back to the bitrate when samplerate and bit depth are empty, and takes `playinfo.type.color` before `font.color`.

## [0.4.8] - 2026-09-25

Album art. `intake` reads the art location from the player and fetches the picture in the background, three seconds at most, into a cache under the temp directory; a location that starts with a slash is served by the player itself. `plot` places the picture from `albumart.pos` and `albumart.dimension` once the file exists. `expose` decodes it by content and stretches it to the box, drawn over the screen picture and under the meter face.

## [0.4.7] - 2026-09-25

Theme text is set in the theme's fonts. `lead` reads `font.path` and the `font.*` files from the player configuration, and each meter's `playinfo.title.pos`, `playinfo.artist.pos`, `playinfo.album.pos`, `playinfo.samplerate.pos` and `time.remaining.pos` with their style word, colour, `font.size.*` and `playinfo.maxwidth`. `intake` carries samplerate, bitdepth, status, duration and a seek position that moves on between the once-a-second reads. `plot` composes the title, the artist with the album when the meter has no album line, the samplerate line and the remaining time as `Scene.texts`; the clock turns red in the last ten seconds. `expose` rasterises them with `ab_glyph`, clipping at `playinfo.maxwidth`.

## [0.4.6] - 2026-09-25

The needle is drawn by sampling its sprite bilinearly at every frame pixel inside its turned bounds, and its alpha channel blends over the face. The meter foreground blends the same way. Edges that were stepped are smooth.

## [0.4.5] - 2026-09-25

Now-playing text is read once a second inside `intake` and carried in `Input.metadata`; the player no longer contacts Volumio on every frame. `--record step.json` writes the skin, input and scene of each step, and `plot` replays every recorded frame under `testdata/frames/` as a test.

## [0.4.4] - 2026-09-24

The theme is drawn at its own pixel size on a fullscreen window. Layer order is the screen picture, the meter face, a needle rotated around the theme origin at `distance`, then the meter foreground. Title and artist stay at `playinfo.title.pos` and `playinfo.artist.pos`.

## [0.4.3] - 2026-09-24

The window covers the display. Needles rotate with the meter level. Title and artist come from Volumio while a track is playing.

## [0.4.2] - 2026-09-24

The window is borderless. The unrotated needle image is not drawn. That image was the thick black bar on each meter.

## [0.4.1] - 2026-09-24

A theme frame no longer paints the placeholder meters or the spectrum staircase. The theme JPEG is shown, and a needle image is placed at `left.origin` and `right.origin`.

## [0.4.0] - 2026-09-24

The selected meter's indicator image is drawn at `left.x`/`left.y` and `right.x`/`right.y`. The visible width follows the channel level. `testdata/show-theme.sh` leaves that frame on the display until Ctrl-C.

## [0.3.0] - 2026-09-24

The frame size follows the selected theme folder. `screen.width` and `screen.height` override that pair when both are set. The theme's meter background is copied at its own resolution.

## [0.2.0] - 2026-09-24

`pane` opens a window when `DISPLAY` is set and uploads one RGBA frame. `plugin/run_glass.sh` sets the X11 driver, and on x64 turns MIT-SHM off, before the process starts. The loop sleeps for `[current] frame.rate` from the plugin `config.txt` (10-60, default 30). `--headless` skips that window. `--output` still writes a PPM.

## [0.1.0] - 2026-09-24

The player reads `/tmp/myfifo` and `/tmp/myfifosa`, plots levels and bars, and rasters one RGBA frame. `--output` writes a PPM. No display library is linked.

## [0.0.1] - 2026-09-24

Initial scaffold. The station line compiles. It does not yet read a FIFO or draw a frame.
