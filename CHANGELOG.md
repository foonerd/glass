# Changelog

All notable changes to Glass are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.9.10] - 2026-10-08

- **A remote says what is blocked, with no ambiguity.** A remote's page has a **Connection** panel: each path to the player tried from the remote's side (the Manager, the channel and the player's own web over TCP, each open, refused, timed out or unreachable with the words), and the frames path over UDP judged from both ends: Check now asks the player, over the channel, to send three probe datagrams to the remote's frames port, and the verdict says arrived (open) or blocked (the player sent, nothing came), naming the side and the fix. A player older than this release is told apart from a block. The remote now hears frames on one fixed port, 5585 unless its configuration says another (`frames_port`; 0 for one the system chooses, as before), so a firewall rule can name it; the panel prints the rule table for this remote: the program's path, direction, protocol and port for each path. The player's channel answers a remote's probe request (`probe` → `probed`), and the Manager keeps each remote's frames port.

## [0.9.9] - 2026-10-08

- **A remote takes test releases too.** A remote's Version panel offered the latest release only, where the player's System tab has had "Offer test releases" since 0.8.5. The remote's settings page now has the same switch under its Version panel, worded as the player's and off unless said: with it on, the remote is offered the newest of the repository's last ten releases, a test release among them and said so beside its version, as soon as it is published; off, the latest release as before. The switch is applied with the other settings and kept in the remote's configuration. The upgrade's checks stay as they were: the archive against the release's checksum, the new display tried, the one before kept. The bundle built with glass-evo carries the switch from the glass-evo release built on this Glass.

## [0.9.8] - 2026-10-08

- **Make a report, whole.** The report carried the sheet as it was before glass-evo and could only be copied. Now it carries the sheet as it stands, the glass-evo section with it, the addresses and the forecast's place hidden unless revealed; its window line says the log level Glass wrote at in those minutes and the level it goes back to; **Download the report** saves the whole as a text file named by the player and the minute, beside Copy for the forum and the GitHub issue; a report longer than a forum post takes is cut at a line with a word that the file has the whole. The report's forms live in one script the page and the tests share.

## [0.9.7] - 2026-10-08

- **Find a problem knows glass-evo.** The guided diagnosis had no symptom for the face, the look panel or the forecast, and no check on the component. Three symptoms more: the clock, the date, the forecast or the picture wrong or missing on the screen; the look panel's likeness and the screen disagreeing; the forecast missing, stale or for the wrong place. Thirteen checks, each pure with a test, over the status sheet's glass-evo section: the component not installed, older than this Glass works with, or without a binary for the player; the kiosk owning the screen; the display started before the component's install, or running Glass's own binary where glass-evo owns the screen; a component without a browser module, the views at the theme alone, or this page holding an older module than the one installed; a look named but not installed; the theme on show bringing a look of its own; a picture named but missing; files under skies that are no sky's name; a theme's own font file that is not there; no place set; no reading, a reading older than three hours, or a request that failed; the clock waiting for the persist period, and that period at 0 s; the skies moving on a small board. The place is never in a finding. The Troubleshooting page has the checks in words; the status sheet's "When the player stops" row says the persist period's length.

## [0.9.6] - 2026-10-08

- **The status sheet brought up to glass-evo.** Everything the product gained since 0.8.60 was absent from the sheet. A **glass-evo** section after Screen now says: the component's version with the least this Glass works with, whether it is outdated, what Glass it needs and the version kept for going back; who owns the screen and how glass-evo holds it (the screen itself or an X server of its own); which binary the display runs, since when, and when the component was installed, with a word when the display started before the install and runs the old one; what the Face tab and Anymote show and whose module they bring, with the pages open; the look chosen and where it comes from (your own, shipped, or built in, and named but not installed), how many skies it brings, and whether the theme on show brings a look and skies of its own; the picture when nothing plays, found or missing; When the player stops; the face size; the forecast's place (hidden with the addresses until revealed, as a place is personal), unit, span, the reading's age and the last request's error. Network and remotes gains the remotes connected, each with what it is, its version and whether a later release is out; Themes gains the theme on show's own fonts, each found or missing where the display looks; Housekeeping's problems say in words when the display is older than the component's install, the component is older than this Glass works with, the look or the picture named is missing, or a place is set with no reading. Copy as text carries all of it.

## [0.9.5] - 2026-10-08

- **No person named in the plugin's own words.** The help for a settings backup's name gave a forum user's handle as an example; the examples are plain words now, in English, German and French.

## [0.9.4] - 2026-10-07

For a face theme's own skies (glass-evo 0.2.4): the likeness draws a look's skies when the look is chosen, not only when it is saved.

- **The chosen look's name goes to the module with the keys.** The look panel handed the module the look's name only for the saved look; a look chosen on its card went without it, so the likeness could not find the look's skies the page had put. It names the chosen look now, as the saved one, and the built-in look none.
- **Glass names glass-evo 0.2.4 as the least it works with.**

## [0.9.3] - 2026-10-07

For a face theme's own skies (glass-evo 0.2.3): they reach the browser views, the remote displays and the look panel's likeness.

- **A face theme's files, listed and served.** `GET /api/face/theme-files?name=` lists a face theme's own files, its skies under `skies/`, each with its path, its route and its checksum; `GET /api/face/theme-file?name=&file=` serves one. The module route's `theme` carries the list beside the theme's text.
- **The Face tab and Anymote** put the chosen look's skies into the module's table beside its `face.txt` before the first frame; **the look panel** puts the look's skies into its likeness module's table when the look is chosen, and paints the likeness again, so it draws the look's skies as the screen does.
- **A remote display** that carries the face brings the look's skies beside its `face.txt`, each checked against its checksum, from the files the player's configuration lists.
- **Glass names glass-evo 0.2.3 as the least it works with.**

## [0.9.2] - 2026-10-07

For a face theme's own skies (glass-evo 0.2.2), asked for on the forum: groundwork on the display's side, nothing to see on its own.

- **A picture file as the frames of a sky.** `expose::read_frames(path, side)`, for a face as `overlay::face::read_frames`: an animated GIF's frames each with the milliseconds it stands (its own delays), or one frame for a still picture, each fitted inside a transparent square of the side, centred.
- **Glass names glass-evo 0.2.2 as the least it works with**, which brings the browser-view font fix of 0.2.1 to every player with it.

## [0.9.1] - 2026-10-07

A theme whose time fields name `fonts/font.ttf` beside its `meters.txt` was set in the digital font in the browser views. With glass-evo 0.2.1.

- **A field's own font reaches the Face tab and Anymote.** A text or time field's `.font` is looked for as an absolute path, in the theme folder and under `font.path`, and the display found it on a player and on a remote but never in a browser view, where the theme's files live in the page's table and not on any disk: the view set such a field in its style's font. The candidates are now tried where the display reads its files, the table included, so the Face tab and Anymote draw the field in the font the theme names, as the player's screen does. The player's screen and the remotes were right before.

## [0.9.0] - 2026-10-07

Glass 0.9.0 gathers the 0.8 series, 0.8.0 to 0.8.90, 1 to 8 October 2026, into one release with glass-evo 0.2.0. Every change below has its own entry under its number.

- **The Glass interface on the player's own screen, glass-evo, from a preview to the pair this release names.** Got, updated and removed on the System tab, chosen on the Screen tab, the screen handed over and back by itself where the face cannot hold it (0.8.0 to 0.8.1, 0.8.16 to 0.8.18, 0.8.27 to 0.8.30, 0.8.37, 0.8.40, 0.8.46, 0.8.49). Glass names glass-evo 0.2.0 as the least it works with.
- **The idle screen on a grid.** The clock, the date and the forecast each on nine cells above the bar, aligned inside them with a margin of the user's own, a piece larger than its cells running over on the side it is aligned to; sizes drawn as set, up to four times the look's, over the edges (0.8.8, 0.8.72 to 0.8.77).
- **The weather.** Today's forecast for a place of the user's choosing, from Open-Meteo, in the Forecast section and on the grid; the span, today as a line or the next 24 hours or the week as columns; the heatmap, every temperature in the colour of its degree, the week's days and the date by their switches; the skies in colour, moving at their own pace, thunder flashing by its switch (0.8.79 to 0.8.86).
- **The look panel drawn by glass-evo itself.** Every piece of the likeness, the clock in its faces, the date and a clock in type in the look's own font, the forecast, drawn by the component's module as the screen draws them, the module fetched by the installed version (0.8.12, 0.8.78, 0.8.89). Each piece's own colour, strength, background and background colour from the look's Colours and Backgrounds, the buttons' included; How solid moving every background at its distance (0.8.84, 0.8.90).
- **A picture of your own when nothing plays**, chosen on the Screen tab from the player's `glass/backgrounds` or uploaded, darkened as you say, on the player's screen and, from 0.8.87, on the Face tab, Anymote and the remote displays; the screen off after minutes with nothing playing, over a fade (0.8.63 to 0.8.64, 0.8.67 to 0.8.68, 0.8.87).
- **When the player stops**, the idle screen at once or after the plugin's persist period, chosen in the Backgrounds section (0.8.88).
- **The browser views carry the face.** The Face tab and Anymote show glass-evo's clock, date and controls over the theme, drawn by its module, and its buttons act on the player; the Screen tab says what the views show; they tell the truth when the player stands still (0.8.5 to 0.8.7, 0.8.9 to 0.8.10, 0.8.19, 0.8.36, 0.8.51).
- **Remote displays with the face, bringing themselves up to date.** A remote built with glass-evo shows the face and brings the look and the picture; a Linux, Windows or Android remote upgrades itself from its settings page, the archive checked, the one before kept; the Remotes tab says which are behind (0.8.20 to 0.8.24, 0.8.53 to 0.8.54, 0.8.58 to 0.8.59, 0.8.61).
- **The System tab.** Test releases, offered to this player as soon as they are published; Back to the stable release, part by part, with a backup first, and from a shell with `get-glass.sh --stable`; downloads that break tried again (0.8.5, 0.8.13, 0.8.52, 0.8.55, 0.8.62).
- **Themes.** Text fields with a font and size of their own, radio and format icons by the most telling name, a theme's own stand-in for a folder layer, rotation quality pacing the turning as before, cue tracks, the sample rate aligned as the theme says, a negative position, fanart carrying on across meters (0.8.11, 0.8.26, 0.8.33, 0.8.47, 0.8.56 to 0.8.57, 0.8.60, 0.8.65 to 0.8.66, 0.8.69 to 0.8.71).
- **The player.** Covers from a Lyrion server through Squeezelite, recognised by content and held across address changes; the caches of covers and fanart bounded; a two-sided spectrum kept through a mono recording; a volume fader dragged as drawn; the display doing less where nothing changes (0.8.2 to 0.8.3, 0.8.15, 0.8.31 to 0.8.32, 0.8.34, 0.8.48).
- **Finding what is wrong.** The reasons a display cannot start, named from the libraries' own words; the Graphics row and the kiosk left stopped, not failed; settings backups of your own never removed; the downloads folder cleared at start (0.8.28, 0.8.35, 0.8.38 to 0.8.39, 0.8.41 to 0.8.45).

## [0.8.90] - 2026-10-07

The Buttons section had no background of its own where every other piece has one. With glass-evo 0.1.54.

- **The buttons' own background, behind More in the Buttons section.** Background (how solid the bar's glass is; as the Backgrounds section's How solid unless moved) and Background colour (the bar's and the More sheet's; the look's unless said), as the clock, the date and the forecast have theirs; the likeness draws the bar with them. Keys `buttons.glass`, `buttons.tint`.
- **How solid moves the forecast's background too.** It had moved the sheet's, the clock's and the date's, each at its distance, and left the forecast's where it was; it moves the forecast's and a buttons' own background the same way now.
- **Glass names glass-evo 0.1.54 as the least it works with.**

## [0.8.89] - 2026-10-07

A likeness that did not agree with the screen until the player was rebooted.

- **The likeness's module is the installed component's, always.** The look panel fetched glass-evo's module once, by a bare address a browser may keep from before an update. It now asks the Manager which component is installed and fetches the module by that version, and brings the module again when a component job finishes, so the likeness is drawn by the module the screen runs.

## [0.8.88] - 2026-10-07

The idle screen came at once on a pause or a stop, whatever the plugin's persist period said. With glass-evo 0.1.53.

- **When the player stops, in the Backgrounds section of the look panel.** "The clock at once" (as before, unless the look says) or "After the persist period": with the second, the screen glass-evo holds keeps the theme, with its countdown where the plugin's Keep Display Active After Pause/Stop says so, and shows the clock, the date, the forecast and the picture when that period ends. Kept with the look as every setting is; key `idle.wait`.
- **Glass names glass-evo 0.1.53 as the least it works with.**

## [0.8.87] - 2026-10-07

The picture when nothing plays (0.8.63) had been built for the player's own screen alone. With glass-evo 0.1.52.

- **The picture when nothing plays reaches the browser views.** A face in the Face tab or on Anymote asks the page for the picture the look names as it asks for the album art: wanted under the home at `backgrounds/<name>`, fetched by the page from `/api/backgrounds/<name>/file` and put into its table, marked missing when the player has none. The theme shows until it is in, as on the player. `intake::host_picture`, for a face as `overlay::face::host_picture`; the table keeps two such pictures at most.
- **And the remote displays.** `/api/remote/config` carries, in its `face` part, `picture: { name, url }` when the look names a picture the player has; a remote's sync brings it to `backgrounds/<name>` under its home beside the faces, the folder emptied first, and the remote names the folder in `GLASS_BACKGROUNDS`. The configuration's version already covers the face part, so a change of picture reaches the remotes at once.

## [0.8.86] - 2026-10-07

Step 4 of the weather: the skies move. With glass-evo 0.1.51.

- **The skies move, in the likeness as on the screen.** In the Forecast section, "Skies move" (on unless the look says) lets each sky keep its shape and gain one small motion at its own pace; the likeness draws the forecast again eight times a second while the panel is on show, so it moves with the screen. "Thunder flashes" (off unless the look says) lights the bolt and the cloud for a tenth of a second every eight to twenty seconds. Off, every sky stands still and nothing about the screen's refresh changes. Keys `weather.motion`, `weather.thunder`.
- **Skies in colour** (on unless the look says): a yellow sun, red when scorching, a pale moon, clouds and fog in greys, drizzle and rain in blues, white snow, the storm dark with a yellow bolt, heavy weather murkier with more falling. Off, every sky is in the forecast's colour as before. Key `weather.colour`.
- **Glass names glass-evo 0.1.51 as the least it works with.**

## [0.8.85] - 2026-10-07

Step 3 of the weather: a heatmap for the temperatures, numbers only, with switches for the week's days and the date. With glass-evo 0.1.50.

- **The forecast's heatmap.** In the Forecast section, "Heatmap" sets every temperature the forecast shows in the colour of its degree, numbers only: from "Cold" at −10 °C, through the forecast's own colour at 12 °C, to "Warm" at 30 °C, each a colour of your own (blue and red unless said). "Also the week's days" colours the week's lows and highs too; "Also the date" sets the date in the colour of the temperature now, with the date's own colour in the middle of the scale, the one link between pieces, by your switch. Every switch is off unless the look says. Keys `weather.heat`, `weather.cold`, `weather.warm`, `weather.heat.days`, `weather.heat.date`.
- **The likeness draws it through the module**, which now takes the player's reading for the date's line as well as the forecast's.
- **Glass names glass-evo 0.1.50 as the least it works with.**

## [0.8.84] - 2026-10-07

The forecast's own colour and background were linked to the date's; a piece's own settings modify the look's, never another piece's. With glass-evo 0.1.47.

- **The forecast's colour, strength, background and background colour are its own.** 0.8.83 had them follow the date's where the look left them so; nothing links one piece to another now. "Same as the rest" means the look's own ink and tint, as it does for the clock and the date, and the likeness draws the forecast's glass from its own settings alone.
- **Glass names glass-evo 0.1.47 as the least it works with.**

## [0.8.83] - 2026-10-07

The forecast's background did not show in the likeness, the date's Background controls were missing on the grid, and the forecast's own settings were wrongly named. With glass-evo 0.1.46.

- **The forecast's own background shows in the likeness.** The likeness drew a forecast's glass as the date's whatever the look said; it draws the forecast's own strength and colour now, the date's where the look leaves them so (`lookmodel.weatherGlassKey`, `weatherTint`, tested), as the screen does.
- **The date's Background and Background colour show on the grid.** They were shown only with the date at the top of the screen, where it first had a glass of its own; a date on the grid has one too.
- **The forecast's own colour, strength and background say "As the date's".** They are the date's unless given their own, which "Same as the rest" did not say: changing the date's colour changes a forecast that has none of its own, and a forecast given its own keeps it whatever the date's.

## [0.8.82] - 2026-10-07

The Forecast section's What it shows showed nothing of what was chosen. With glass-evo 0.1.46, which draws today's line and the span's columns as chosen from the mockups.

- **"What it shows" shows what is chosen.** The page painted a choice only for the clock's face and dial; the forecast's span is painted as one of its six words now, today unless said (`lookmodel.span`, tested).
- **Glass names glass-evo 0.1.46 as the least it works with.**

## [0.8.81] - 2026-10-07

Step 2 of the weather: the next day every few hours, or the week. With glass-evo 0.1.45.

- **The forecast's span.** The Forecast section's "What it shows" is today as a line, the next 24 hours every 2, 3, 4 or 6 hours, or the week; the face and the likeness draw the hours and the week as columns in the forecast's cells, each its time, its sky and its temperature (the week each day's low and high). The key is `weather.span`, `today` unless said.
- **The reading carries the hours and the week.** The plugin asks Open-Meteo for the hourly forecast beside the daily one, seven days in one request every half hour as before, and the `weather` line carries `hours` (24 from the next whole hour at the place: `hour`, `temp`, `code`, `day`) and `days` (7 from today: `weekday`, `code`, `low`, `high`) beside what it carried. The overlay crate names `Hour` and `Day`. A display that does not know the fields ignores them.
- **Glass names glass-evo 0.1.45 as the least it works with.**

## [0.8.80] - 2026-10-07

Found on the player's page before it was handed over, with glass-evo 0.1.44.

- **The Forecast section's size, grid and More controls show.** 0.8.79 never set the section's own visibility flag, as the Clock and Date sections have theirs, so "On the grid", "Inside its cells", "Margin", the size and the finer settings stayed hidden; they show now while the forecast is on, as the other sections' do.

## [0.8.79] - 2026-10-07

The weather on the idle screen's grid, as the clock and the date are. With glass-evo 0.1.44.

- **Today's forecast, for a place the user chooses.** The Forecast section on the Screen tab: on or off, a place found by name (a town or a city; the first eight matches to pick from), the degrees in °C or °F, and the reading as the player holds it. The plugin asks Open-Meteo once a place is set, every half hour, keeps the reading across restarts and drops one older than three hours; "Weather data by Open-Meteo" stands in the section, as its licence asks. The display hears a `weather` line on the channel (`place`, `unit`, `now`, `code`, `day`, `today`, `low`, `high`, `rain`, `at`; `off` when there is nothing), and `GET /api/weather`, `GET /api/weather/search?name=` and `POST /api/weather` serve the section.
- **On the grid and only, as the clock and the date are.** The section has "On the grid" (the bottom row across the three columns unless the look says), "Inside its cells", "Margin" and a size of its own, and behind More its own colour, strength and background, each the date's unless said. The forecast on the same cells as the clock or the date, aligned the same, shares their glass in the order clock, date, forecast.
- **The likeness, drawn by the module.** glass-evo's module draws the forecast line from the player's own reading, in the look's font, as it draws the date and the clock: a sky, the temperature now, a sky, today's low and high. Nothing shows until a place is chosen, on the likeness as on the screen.
- **The overlay crate names `Weather`, `Sky` and `sky_of`**, and `Input` carries the reading, so a face can draw it; Glass names glass-evo 0.1.44 as the least it works with.

## [0.8.78] - 2026-10-07

The date on the grid was set in the browser's type in the likeness and in the face's own on the glass, and the two did not agree. With glass-evo 0.1.43.

- **The module draws every piece of the likeness.** The date, and a clock in type, are set by glass-evo's own module in the look's bold font, brought from the player, as the face sets them on the screen: the same shapes, the same room, pixel for pixel at the likeness's scale. A drawn clock face was already the module's. Nothing stands in for the module: until it and the font are here, the likeness shows no type.
- **For the face's types,** the overlay crate names `FontFiles` beside `Input`, `Metadata` and `TextStyle`, so a page's module can load fonts the way the display does.
- **Glass names glass-evo 0.1.43 as the least it works with**, so a player takes the module that draws the lines before this page.

## [0.8.77] - 2026-10-07

The date on the grid, wired exactly as the clock is. With glass-evo 0.1.42.

- **The date on the grid, as the clock is.** The Date section has the same nine cells ("On the grid", with "As the look places it" to leave them), "Inside its cells" and "Margin" as the Clock section; `date.place` takes cells beside its three words, `date.align` and `date.margin` are the date's own. The Where words show only while the date is off the grid, and the section's summary says "On the grid" while it is on it.
- **The likeness as the screen.** The date on the grid stands in its cells as the face sets it: a size the look comes with fitted to the cells, a size the user set kept, the margin from every side, and a date larger than its cells running over the side it is aligned to. The clock and the date on the same cells with the same alignment stand one under the other on one glass, the clock first, each against the side the block is aligned to; the date is set first and the clock takes what it leaves of the cells' height. A date off the grid stands as before, and the clock with it.
- **Glass names glass-evo 0.1.42 as the least it works with**, so the two halves move together.

## [0.8.76] - 2026-10-07

A clock larger than its row on the grid aligned the wrong way about. With glass-evo 0.1.41.

- **A clock larger than its cells runs over on the side it is aligned to, in the likeness as on the screen.** Before, the likeness kept the margin only on the sides the clock was aligned to and set the clock's edge there, so a clock larger than its cells hung off the far side and "top" moved it down. Now the margin is kept from every side of the cells, as the face keeps it, and a clock whose glass is larger than the cells less the margin stands the margin from the far side instead, its excess over the side named: "top" always moves it up and "left" always left. A clock that fits its cells stands exactly as before. `lookmodel.turned` and `lookmodel.flexed` name the rule, tested.

## [0.8.75] - 2026-10-07

0.8.74's release failed at the signing's own check, as 0.8.73's had: the runner's `osslsigncode`, installed fresh from Ubuntu at every run, is 2.8 now and reports a timestamp as "Timestamp time" where the version before listed the timestamp authority's certificates. The binary was signed and timestamped all along.

- **The signing's check recognises both outputs.** The time-stamp request is as it was for every release before; the second time-stamp service added in 0.8.74 is taken out again, since it was not the cause.

## [0.8.74] - 2026-10-07

The clock on the idle screen's grid, the first step of the grid, with a margin of the user's own. The face draws it from glass-evo 0.1.40; this is the Manager's part, for the clock alone, as 0.8.72 and 0.8.73 carried it before the night's rollback.

- **The clock on the grid, in the look panel.** With glass-evo 0.1.40 the Clock section gets "On the grid": nine cells to press, one for a cell and then another for the block between them, with "As the look places it" to take the clock off the grid again; "Inside its cells", nine squares for where the clock stands inside what it occupies; and "Margin", units of a 720th of the height between the clock's glass and the sides of its cells it is aligned to, 20 as before and 0 for the corner. The likeness draws the clock in its cells by the face's rules: the margin kept from the side it is aligned to, the glass giving way as the clock takes more, a size the look comes with fitted to the cells and a size the user set kept.
- Keys for a look or a face theme: `clock.place`, `clock.align`, `clock.margin`. Shown only where the glass-evo installed knows them. Nothing else changes.
- **The Windows signing asks a second time-stamp service** when Microsoft's does not answer (DigiCert's), with more retries; 0.8.73's release failed three times for want of a timestamp. A failed verification now says what it saw.

## [0.8.73] - 2026-10-07

The clock placed in a corner of the grid wanted a margin of the user's own. The face has it from glass-evo 0.1.40.

- **Margin** in the Clock section, shown while the clock is on the grid: units of a 720th of the height between the clock's glass and the sides of its cells it is aligned to; 20 as before, 0 for the corner. The likeness keeps the same margin (`clock.margin`).

## [0.8.72] - 2026-10-07

The first step of the idle screen's grid: the clock on it. The face draws it from glass-evo 0.1.39; this is the Manager's part, for the clock alone.

- **The clock on the grid, in the look panel.** With glass-evo 0.1.39 the Clock section gets "On the grid": nine cells to press, one for a cell and then another for the block between them, with "As the look places it" to take the clock off the grid again; and "Inside its cells", nine squares for where the clock stands inside what it occupies. The likeness draws the clock in its cells by the face's rules: the margin kept from the side it is aligned to, the glass giving way as the clock takes more, a size the look comes with fitted to the cells and a size the user set kept.
- Keys for a look or a face theme: `clock.place`, `clock.align`. Shown only where the glass-evo installed knows them. Nothing else changes.

## [0.8.71] - 2026-10-06

Reported on the forum: a theme's button with `button.nextmeter.action = meter.next` "changes the skin on the computer (FACE) but does not simultaneously change the skin on the RPi."

- **A meter stepped to in the Manager's Face tab is shown on the player's screen too.** A `meter.next` or `meter.previous` button pressed on the screen already moved the Face tab and every remote that follows the player; pressed in the Face tab it moved that page alone. The page now hands the meter to the plugin (`POST /api/face/meter`), the plugin asks the player's own display for it over the channel (`{"kind":"show","meter":"..."}`), and the display's answer brings the other views along. The display shows it when the theme on show has a meter of that name, and its own next step goes on from there. With no display of the player's own running, the page keeps its meter to itself as before.

## [0.8.70] - 2026-10-06

Asked on the forum: "an option to add custom fonts for: artist, title and album. In the same manner as we can do for total and elapsed time."

- **A text field takes a font of its own.** `playinfo.title.font = fonts/MyTitle.ttf` sets the title in that file, found as a time field's font is: an absolute path, a file in the theme folder, or a file under `font.path`; not found, the font of the field's style as before. `playinfo.title.fontsize` gives it a size of its own, the style's size without it. The same two keys for `playinfo.artist`, `playinfo.album`, `playinfo.samplerate`, `playinfo.ticker` and `playinfo.next.title`, `.artist`, `.album`. A theme that names none of them draws as it did.

## [0.8.69] - 2026-10-06

The radio icons of 0.8.66 looked squashed on the screen, DAB and FM with the indicator starting below the line. Chosen from three mockups, then adjusted twice (MONO dead centred, the bars clear of the M and the same distance from the letters in every set), and with the plugin's coming STEREO word taken on.

- **The radio icons redrawn.** The FM and DAB letters at their full height, the five signal bars standing on the letters' baseline with their tops on a parabola, rounded, the unlit ones outlined; MONO dead centred under smaller FM letters. The names and the lookup are unchanged, so a theme's own icons stay as they are.
- **FM in stereo.** The radio plugin will name a stereo station `FM STEREO ◦◦●●●`; the icon is looked for under `fm_stereo_3`, `fm_stereo`, `fm_3`, `fm`, as a mono one is under its own word, and the plugin brings `fm_stereo_0` to `fm_stereo_5` and `fm_stereo` beside the mono set.
- The type shown as text reads `FM MONO` and `FM STEREO`, not `FM_MONO`.

## [0.8.68] - 2026-10-06

The screen-off of 0.8.67 was abrupt; asked for gradual.

- **The screen goes black and comes back over a fade.** "Over, milliseconds" beside "Screen off after" in the look panel: how long the screen takes to go black and to come back, 500 (half a second) unless said, 0 for at once, up to 120000 (two minutes). The face's setting `face.idle.fade`, in a face theme `[idle] fade`. Drawn by glass-evo from 0.1.36.

## [0.8.67] - 2026-10-06

Asked by a user with an AMOLED screen, which must not show the same picture for hours: a screen timeout for glass-evo, since it turns off the kiosk's own. Glass's half; glass-evo does it from its 0.1.35.

- **"Screen off after, minutes" in the look panel.** In the Backgrounds section of "How the screen looks": the minutes with nothing playing and no touch after which the screen glass-evo holds goes black; 0, the default, never. A tap wakes it and is the wake alone, and music starting wakes it too. It is the face's setting `face.idle.off`, kept with the look, in a face theme `[idle] off`.
- The screen's pixels go dark; a panel's backlight is not touched. On an OLED that is the screen off; on an LCD it is a black picture.
- Not yet: a slideshow when idle. The picture when nothing plays (0.8.63) stands still; a show of several is a later stage.

## [0.8.66] - 2026-10-06

Found while the FM/DAB radio plugin was being developed: a station received in mono had no icon.

- **An icon for FM in mono, and icons by signal level for FM and DAB.** The radio plugin names what plays as `FM` or `DAB` with its signal after it, five dots of which the filled ones count (`FM ◦◦●●●` is 3 of 5), and says a mono station with one channel. The icon for what plays is now looked for under the most telling name first and the plainest last: `fm_mono_3`, `fm_mono`, `fm_3`, `fm`, and the type's own name; the first found is drawn. The plugin brings the set: `fm_0` to `fm_5` and `dab_0` to `dab_5` with the signal as bars beside the letters, `fm_mono_0` to `fm_mono_5` with MONO under the letters, and `fm_mono` without bars. A theme brings its own under the same names in its `format-icons` folder, as before, and may bring only some: a theme with `fm_mono.svg` and no levels shows it at every level.
- A type the player reports with one channel is looked for under its `_mono` name first, whatever the source; where no such icon is there the plain one shows, so nothing changes for a mono file.
- The display reads the state's `channels`.

## [0.8.65] - 2026-10-04

Asked for after a theme maker put the volume on a meter as a number: the number as a full text of the theme's own.

- **The volume number takes an alignment, the italic style, words around it and a word for muted.** `volume.value.pos = x,y[,style]` has written the volume as a number since the first themes of Glass, with `fontsize`, `color`, `maxwidth` and `font`. Four things are new, each optional, and a theme that sets none draws as before:
  - `volume.value.align = left | center | right`: where the number stands in its `maxwidth`; absent, as the meter aligns its texts.
  - `italic` as a style in `pos`, beside `light`, `regular`, `bold` and `digi`.
  - `volume.value.format`: words around the number, `{}` where it goes: `VOL {} %` shows "VOL 45 %". A pattern with no `{}` is none.
  - `volume.value.mute`: what stands in the number's place while the player is muted, such as `MUTE`; absent, the number shows as ever.
- The cutter keeps the three new keys as written when it cuts a theme to another size.

## [0.8.64] - 2026-10-04

A small flaw of 0.8.63's look panel.

- **The picture's tile is marked while its chooser is open.** "Picture when nothing plays" opened its chooser without showing that it was the tile in hand: the mark was taken off again with the looks' own. It keeps it now.

## [0.8.63] - 2026-10-04

Asked on the forum: a photograph of one's own behind the clock and the date when nothing plays, in the theme's place. Glass's half; glass-evo draws it from its 0.1.32.

- **A picture when nothing plays, chosen in the look panel.** After the row of looks on the Screen tab stands one more tile, "Picture when nothing plays". It opens the pictures to choose from: none (the theme shows, as before), each picture on the player, and an upload. The picture is apart from the looks and goes with whichever is chosen. The likeness under "When nothing plays" shows it behind the clock and the date, and the Backgrounds section gains "Darken the picture" while one is chosen. Nothing reaches the screen until Save, as with the rest of the panel.
- **Where the pictures are kept.** In `glass/backgrounds` on the player's Internal Storage share (`/data/INTERNAL/glass/backgrounds`): put them there over the network, or upload them from the tile. JPEG, PNG and WebP, told by their first bytes, at most 32 MB; an uploaded one is kept under a name of letters, digits, spaces and `. _ ( ) -` with the ending of what it is, and only files so named are listed. Removing the picture that is on show takes the choice with it.
- The choice and the darkening are the face's settings `face.idle.picture` and `face.idle.dim`, kept with the look: a settings backup holds them, and "Back to the stable release" asks about them with the look.
- Routes: `GET /api/backgrounds`, `GET /api/backgrounds/<name>/file`, `POST /api/backgrounds/upload?name=`, `DELETE /api/backgrounds/<name>`; `GET /api/face` lists `backgrounds`.
- For a face: `face::read_covering(path, w, h)` reads a picture scaled to cover a screen and cut from its middle, and the launcher names the folder in `GLASS_BACKGROUNDS`.
- Not yet: the picture on remote displays and in the browser views that carry the Glass interface; they show the theme.

## [0.8.62] - 2026-10-04

The way back to the stable release for a player whose Manager does not come up.

- **`get-glass.sh --stable`.** The installer run with `--stable` on a player that has Glass does from a shell what the System tab's "Back to the stable release" does, for the case where the Manager cannot be reached to press it: `curl -fsSL https://raw.githubusercontent.com/foonerd/glass/main/get-glass.sh | sh -s -- --stable`, as the volumio user. It fetches the latest release that is no test release, writes a settings backup named `before-stable` that the Backups tab restores, hands the release to the player's plugin manager as an update where another version is installed, lays the settings as that release's own plan makes them (its defaults, with the screen's and the network's set-up kept and test releases off), and restarts the backend. It asks nothing: the answers are the suggested ones.
- It leaves glass-evo and whose the screen is as they are; the Manager, once it is back, steps glass-evo and offers the screen's question.
- The plan is not the script's: it runs `manager/stable.js` out of the release's own zip, so the shell and the Manager cannot come to differ. A release older than 0.8.52 has none, and the script says so.

## [0.8.61] - 2026-10-04

The last stage of upgrades on a remote display: Android.

- **An Android remote says when a later release is out, and links it.** The Version panel is shown in the Android app's settings page too: it looks at the latest release of what the remote is, and where a later one is out it links that release's package. Opening the link hands it to Android's own installer, which asks, checks the package's signature against the installed app's, and installs it over the app with the settings kept. The app does not install anything by itself, and `POST /api/upgrade/install` answers 400 `by-hand` there. `GET /api/state` says `upgradeInPlace`, false on Android.
- The Remotes tab's line on a remote that is behind says how each system is brought up to date.
- Not tried on an Android device or emulator: the look at the releases is the one Linux and Windows use.

## [0.8.60] - 2026-10-04

Asked by a theme maker: a universal back cover for the albums that have none.

- **A picture the theme brings stands in for a folder layer.** A folder layer (`folderlayer.files`) shows the first of its names found in the playing track's folder, a back cover, a logo. Where the track's folder has none of them, the same names are now looked for in the theme's own folder, in the layer's order, and the first found there is shown: a skin that ships `CoverSamp.jpg` and lists it after `back.png, Back.png, back.jpg` shows the album's own back cover where there is one and the sample where there is none. What the track's folder has always stands before the theme's, and a layer whose names the theme does not bring stays empty as before. It travels with the theme's folder, so remote displays and the browser views show the same; on those the stand-in appears once the look at the track's folder has come back with nothing.

## [0.8.59] - 2026-10-04

The second stage of upgrades on a remote display: Windows.

- **A remote on Windows brings itself up to date.** The Version panel on its settings page works there as on Linux: it looks at the latest release of what the remote is and upgrades to it. The release's zip is fetched and checked against its size and checksum, `glass.exe` (or the bundle's display) is taken out of it and tried, the one that runs is renamed to `glass.prev.exe`, the new one takes its name, and it is started while the old one leaves; the new one waits a moment for the old one's page port. A new version that cannot hold on is given up for the one before at its third start, and kept as `glass.failed.exe`. `SDL2.dll` stays as installed.
- From 0.8.54 to 0.8.58 the panel showed on Windows and Android and answered that no upgrade is offered there. It is not shown on a machine it cannot upgrade: Android, until its own stage.
- Tried under Wine, not yet on a Windows machine.

## [0.8.58] - 2026-10-04

The Manager's part of upgrades on a remote display.

- **The Remotes tab says which remotes are behind.** Each connected remote is listed as what it is, the standalone at its Glass or the bundle by its face's name and version with the Glass it is built on, and a remote for which a later release is out is marked "<product> <version> is out", with a line on how it is brought up to date: its own settings page does it on Linux, a new install over it on Windows and Android. The Manager only says so; it upgrades nothing on a remote. A remote is compared with the latest release the Manager last saw of exactly its product; where that is a test release, which no remote is offered, nothing is said.
- `GET /api/remote/status` gives each remote its `face` and `upgrade` (`product`, `version`, `latest`, `behind`: true, false, or null where it cannot be said).

## [0.8.57] - 2026-10-04

A setting carried over from PeppyMeter Screensaver that Glass read and never applied.

- **Rotation quality paces the turning again.** PeppyMeter Screensaver drew a turning record, a tape reel and turning album art anew only so many times a second, in steps of so many degrees: Low 4 times in steps of 12, Medium 8 in steps of 6, High 15 in steps of 3, and Custom at `rotation.fps` (with a step of 45 over that number, between 1 and 12). Glass has read `rotation.quality` and `rotation.fps` since its first turntable and applied only the speed: the pictures turned at every frame, whatever the setting, the performance profiles and the wiki said. The pace is applied now. Between two of its moments a turning picture stands exactly as last drawn, and a picture that stands is not painted again, which is the saving the setting is for on a small board. How fast a record turns is unchanged.
- What this changes on a screen: with Custom at a number near the frame rate (the template's 25) nothing to the eye; with Low, Medium or High a record turns in visible steps, as it did under PeppyMeter Screensaver. The performance profiles set this quality, and their rotation column now does what it says.
- A scene carries the pace (`turn_pace`, times a second and degrees), so remote displays and the browser views turn alike.

## [0.8.56] - 2026-10-04

Asked by a theme maker: which icon name a track of a cue sheet takes.

- **A track of a cue sheet has a type, `cue`.** The player reports no type at all for such a track, so the type area stayed empty and no icon name could match. Where the player names no type and the track's address is a cue sheet's (`cue://`), the type is `cue`: a theme's `format-icons/cue.png` or `cue.svg` shows, and the label reads the same. A type the player does name stands as before.

## [0.8.55] - 2026-10-04

Found by running "Back to the stable release" on a player for the first time.

- **After the way back, the test release left behind is not offered again.** The Manager keeps the release it last saw for a day. A player that took test releases had a test release kept there, and after "Back to the stable release" turned test releases off, the System tab still offered that test release until the next daily look. The act now forgets what was last seen, of Glass and of glass-evo, when test releases are not kept, so the next look asks for the stable release.
- The act itself went as written on a Raspberry Pi 5 with glass-evo holding its screen: both stepped back to their stable releases, the screen stayed where it was kept, the kept parts of the settings stayed and the rest went to the defaults, and the backup named `before-stable` brought the settings back.

## [0.8.54] - 2026-10-04

0.8.53 was tagged and never released: its build was refused, rightly, and this release is that one made to load on a player.

- **The display loads on Volumio again with the remote's upgrade in it.** The upgrade of 0.8.53 tried and started the new binary through the standard library's process calls, which refer to two functions of glibc 2.39; a binary that refers to them, even weakly, is refused whole by the loader of Volumio's glibc 2.36. The release build checks for exactly that and stopped, so nothing of 0.8.53 was published. Both places go through `posix_spawn` and `execv` directly now, as the display's one other start of a program always did.
- The same check runs in the workshop pass (`scripts/check.sh`, "glibc") on the machine's own build, so a change that brings such a symbol in is stopped before a tag, wherever the machine's glibc is newer than the player's.
- Everything 0.8.53's notes say holds for this release: the Version panel on a Linux remote's settings page, the upgrade checked, tried and kept reversible, `Overlay::origin`, `glass --version`.

## [0.8.53] - 2026-10-04

A remote display brings itself up to date, on Linux; the first stage of upgrades offered on a remote.

- **The remote's settings page has a Version panel.** It says what the remote is (Glass, or the display built with a face and the Glass it is built on), looks at the latest release of exactly that on GitHub when asked, soon after every start and once a day, and where a later release is there offers **Upgrade to** it. The upgrade fetches the archive for the machine, checks it against the size and the checksum the release states, takes the display's binary out of it, tries it (it must start on this machine and say the release's version), puts it in place of the one that runs with the one before kept beside it as `glass.prev`, and starts the display again as the new version: by the user service where that keeps it running, by replacing itself where it was started any other way. Settings, players and the cache stay as they are.
- **A new version that cannot hold on does not stay.** Until a start has lived a minute the upgrade is on trial: the third start that finds it so puts the version before back in place and runs it, and the one given up is kept as `glass.failed`.
- **What is fetched is not the request's to say.** The page's two routes, `POST /api/upgrade/check` and `POST /api/upgrade/install`, take nothing: the repository, the archive's name and the file inside it are fixed by what the display is, the address must be the release's own on GitHub, and only the latest release that is no test release is ever taken. `GET /api/state` says `product` (name, repository, version) and `upgrade` (phase, the latest release, whether it is later, the last error).
- A display built with a face says where it is released through the contract (`Overlay::origin`, an `Origin` of repository, archive name, binary name and version); one that says nothing is offered no upgrade, since Glass's own release would take the face away. glass-evo says it from its 0.1.28.
- `glass --version` answers `glass <version>`.
- Not yet: Windows and Android remotes (the panel is not shown there), and the Manager's Remotes tab marking a remote that is behind.

## [0.8.52] - 2026-10-04

The way out that test releases need, and a known state to ask for when something is wrong.

- **Back to the stable release, on the System tab.** One act puts the latest release that is not a test release in place, of Glass and of glass-evo where it is installed, newer or older than what runs, and the settings back to what that release comes with. Before it does anything it asks, part by part, what is kept, yes or no, each with an answer suggested: the screen's set-up (rotation, touch, calibration, position, output) and the network set-up (remote displays, ports, the Manager's address, the share) are kept unless the answer is no; glass-evo on the player's screen, the theme on show with the fonts and the fanart show, glass-evo's look, when the display shows and what a touch does, performance and logging, and taking test releases go back to the default unless the answer is yes. A question is asked only where it means something on this player. Then the whole is said once more, with the versions it goes from and to, and nothing happens before "Go back to the stable release".
- What the user brought is not touched by any answer: installed themes, fonts, looks and backups, the fanart key, and whether an uninstall leaves the themes.
- **In an order that changes nothing before what it needs is here.** The stable Glass is downloaded and checked first; a settings backup named `before-stable` is written, and without it nothing is changed; glass-evo is stepped to its stable release, held to the Glass that will be here; the screen is given back to the kiosk unless kept; the settings are laid as answered over the defaults of the stable release itself, read from its own zip; Glass is replaced through the player's plugin manager and the backend restarts. On a player that already runs the stable release only the settings change and the backend restarts on them. A step that fails puts the settings back from the backup and glass-evo back to the version before. The backup is on the Backups tab: restoring it brings the settings back as they were.
- A setting a later release wrote and the stable one does not know goes with the reset. Every setting the plugin ships, reads or writes is named in one table with the part it belongs to (`plugin/manager/stable.js`), and a test fails when one is added without it.
- Routes: `GET /api/stable` says what the act would do here (`glass` and `evo`, each `{ action: none | back | forward, from, to }`, the `questions` with their suggested answers, the `facts` they follow from); `POST /api/stable` with `{ "keep": { "<part>": true | false } }` starts it as a job, an answer left out being the suggested one; 409 `busy` while an upgrade runs.

## [0.8.51] - 2026-10-04

A leftover removed: the black a player was to show after standing still past its countdown.

- **The theme stands behind a player that stands still, without a black frame at the countdown's end.** Where the screen is the display's own (glass-evo holding it, a remote or a browser view that carries the face), the display turned the picture black once a player that does not play had a persist period in `countdown` mode with no second left. The plugin clears that period at the moment it ends, so the black never stood: it could show for a frame or a few at the end of the countdown, longer in a browser view whose line from the plugin came late, and after a plugin that stopped in the middle of a countdown it stayed until the next play. What stands on a still screen is the theme under the clock, and that is now all there is: the black and what decided on it are gone from the display's loop and from the browser's pipeline (`overlay::stands_black` and `overlay::Black` are removed; no face used them).

## [0.8.50] - 2026-10-04

Glass's half of a meter theme that brings a look for the face; glass-evo draws it from its 0.1.26.

- **A face is told which theme is on show.** What a face sees each frame (`overlay::View`) now names the folder of the theme on show (`theme_dir`, empty where there is none), on the player's screen, on a remote and in a browser alike. A theme's whole folder already travels to remotes and to the pages, so what a theme brings for a face beside its `meters.txt` is there wherever the theme is drawn. A face written against the contract fills or ignores the new field; one built against an earlier Glass is unchanged until it is built against this one.
- **The look panel shows what the theme on show brings.** Where the theme on show has a `face.txt`, `GET /api/face` gives its keys as `themeLook` (`{ folder, keys }`, else null), and the panel under Screen lays them where the face does: over the look chosen there, under the user's own adjustments. The likeness shows the result, and a line under the looks says that the theme brings a look of its own and which stands over which.
- **A save leaves alone what is stored as it stands.** The panel named every key at a save and removed each that agreed with the look beneath it. With a theme's look in between, an adjustment of the user's that the theme on show happened to agree with would have been removed, and missed under the next theme. A key stored with the value it has now is no longer named; one moved back to what stands beneath it is removed, as before.

## [0.8.49] - 2026-10-04

A precaution for the graphics check of 0.8.37.

- **The probe never holds the screen's card.** The first process to open a graphics card that has no master becomes its master, and SDL passes over a card it cannot be master of. The probe opens the screen's card to look at it; run at a moment when nothing holds the screen, it was the card's master for the fraction of a second it runs, and a display starting in exactly that moment would have been refused the card. The probe gives the card up as soon as it has opened it. Not seen to happen; the two can now not meet.

## [0.8.48] - 2026-10-04

Found while a finding of the wiki's review was checked against PeppyMeter Screensaver and the collection.

- **A bar is dragged the way it is drawn.** A touch on a volume or progress bar was turned into a value by the shape of the bar's box alone: left to right in a box wider than tall, whatever the bar's own direction. A fader that moves up and down in a wide box, as ninety-four meters of the collection have their volume, was drawn bottom to top and read left to right: a finger moved along the fader changed nothing, a finger moved across it did. A bar is read now as it is drawn (`controls::gauge_fraction`): bottom to top where it is vertical, by `slider.orientation` or, where that says neither way, by its box; left to right otherwise. A knob, an arc and a number are read by their box as before.
- What a slider's orientation means is unchanged and is PeppyMeter Screensaver's rule: `vertical` or `horizontal` as written; a volume bar that says nothing is vertical and a progress bar horizontal; any other word goes by the box. No theme of the collection has a volume bar that says nothing in a wide box.

## [0.8.47] - 2026-10-04

Asked by a theme maker whose `webradio.png` did not show.

- **A theme's format icon may carry the type's own name.** The icon a theme brings for what plays is looked for in its `format-icons` folder under the type's key, and the key of web radio is `radio`: a file named `webradio.png`, as the player itself calls the type, was never looked for, and the player's own radio icon showed in its place. The same held for every type whose key is shorter than its name (`tidal_connect` for `tidal`, `dab_radio` for `dab`, and the like). A theme's folder is searched under the key first, as before, and then under the type's name as the player reports it (`lead::format_name`: lower case, spaces as underscores), `.png` before `.svg` for each. The player's and Volumio's own sets are found by the key, as before.

## [0.8.46] - 2026-10-04

Found on a Raspberry Pi 5 with a DSI panel: glass-evo could not take the screen, three starts ending in "kmsdrm not available", on a player where it had held the screen for days.

- **The display tells SDL which card has the screen.** On a screen with no X server SDL looks for the card to draw on by going through `/dev/dri` in the order the system lists it, and its search (SDL 2.26.5, `get_driindex`) does not stop at the card it can use: a card listed after it that has connectors and nothing connected makes it forget what it found, and it ends with "kmsdrm not available". A Raspberry Pi with its panel on DSI and nothing on HDMI has exactly that pair of cards, and which is listed first is decided by the order the kernel made them in at boot. So the same player drew on its screen after one boot and could not after another: glass-evo failed three times and the kiosk took the screen back, and a display on a player with no kiosk died at every start. SDL takes the card's number from `SDL_KMSDRM_DEVICE_INDEX` where it is set, and searches for nothing. The display sets it now before it opens the screen, to the card of the connected connector as the kernel lists the connectors (`pane::kms`), the choice the graphics check makes too; and the plugin sets the same in the display's environment, so a glass-evo built before this release is told as well. A number set from outside stands. With two screens connected the first by name is taken, the same at every start.

## [0.8.45] - 2026-10-04

Found while the wiki was read against the code.

- **The cutter leaves flags, shares and comments as they are.** A theme cut to another size has every position and length scaled, by what kind each key is. Three things were classed wrongly, each ending in a warning under the cut where nothing was amiss. `flip.left.x` and `flip.right.x` are switches, not positions: the cutter tried to scale "True". A marker's `pos` on a volume or progress gauge (`*.marker.*.pos`) is a share of the gauge from 0 to 100, one number and no pixels in it: the cutter took it for a pair of coordinates, one warning for every marker of every meter, sixteen hundred of them across the collection's themes. And a line that opens with a semicolon is a comment to the display and was a key to the cutter: a position commented out that way was scaled inside its comment, and any other such line drew a warning. All three are left as written now, without a word.

## [0.8.44] - 2026-10-04

Found while the wiki was read against the code.

- **The help of "Adaptive spool speeds" says what the spools do.** It said the left spool slows as the tape depletes and the right one speeds up, the reverse of what is drawn and of what a tape does. The spool the tape leaves turns faster as it empties and the one that takes it up slower as it fills, each from half to one and a half times its set speed over the track; with the reels turning counter-clockwise the left spool is the one that empties, clockwise the right. The text says so now, in English, German and French. Nothing changes in what is drawn.

## [0.8.43] - 2026-10-04

Found while the wiki was read against the code.

- **Find a problem recognises a window the display could not open.** For a display that dies or keeps restarting, the diagnosis looked in Glass's lines for "could not open the window", a sentence nothing has ever written, so that finding never came and the cause was left to the general "the display died". It looks now for what the display does write: SDL's own words for a driver or a window it could not have ("SDL error: ...", "x11 not available", "No available video device") and the display's for a player with nothing to draw on ("no screen to draw on"). The finding quotes the last of them and points to the status sheet's Kiosk and Graphics rows, in English, German and French.

## [0.8.42] - 2026-10-04

Found while the wiki was read against the code.

- **The clock digits can be set in one of the player's own fonts.** The Appearance tab offers the player's fonts for every text style, the clock digits among them, and writes the chosen font by its bare file name. The display looked for the four text styles' fonts under `font.path` and took the clock's (`font.digi`) as written, so a player font chosen for the digits was a name it could not open, and the clock was drawn in no face at all. `font.digi` is found under `font.path` now as the others are; `builtin` is still DSEG7, and an uploaded font, a whole path to a file that is there, is still taken as it is.

## [0.8.41] - 2026-10-04

Found while the wiki was read against the code.

- **A backup you make is never pruned, whatever you name it.** The backups an upgrade writes on its own are kept to the newest five, and one made before backups carried a mark was known by its name, `before-<version>`. A backup made by hand under such a name was taken for one of them and removed with the oldest. A backup now says in its manifest whether an upgrade wrote it or a user did (`automatic`, true or false), and only what the manifest leaves unsaid is judged by its name (`update.automaticBackup`). Backups made by hand before this release under such a name are still judged by it.

## [0.8.40] - 2026-10-04

Found while the wiki was read against the code.

- **A change of the screen's owner that does not finish says why.** When handing the screen to glass-evo, or giving it back, failed part of the way, the Screen tab said "The change did not finish:" and nothing after it: the reason was sent under `data.message`, where the page, and every other error of the Manager, has `message`. `POST /api/screen/owner` answers `GLASS.MANAGER_OWNER_FAILED` with the reason as `message` now, and the page shows it.

## [0.8.39] - 2026-10-04

Found while the wiki was read against the code: the Backups tab promised what the uninstaller did not keep.

- **An uninstall keeps the settings backups, always.** The uninstaller removed Glass's whole folder under Internal Storage, the backups in it, unless "Do not delete themes" was on, while the Backups tab's own words say a backup survives an uninstall and is what a player is put back from after one. Without that switch an uninstall now removes the themes, the catalog's files, the previews, glass-evo and the rest, and leaves `backups/`; where nothing was backed up the folder goes whole, as before. With the switch on everything stays, as before.

## [0.8.38] - 2026-10-04

Found while the wiki was read against the code.

- **The downloads folder is cleared at the start, folders and all.** What a run leaves under `catalog/downloads/`, the zip of an interrupted install, a package kept for its Download link, the folders a cut, a package or an uploaded backup was staged in, was meant to go when the plugin starts. The clearing removed files only and stopped at the first folder it met, leaving that folder and everything listed after it: on a player where themes had been packaged or cut, the leftovers piled up for good. Each entry is removed by itself now, a folder with what it holds, and one that will not go is named in the log and does not keep the rest.

## [0.8.37] - 2026-10-04

From a take that failed on a player whose graphics libraries did not work, where the kiosk's own X log had said so all along and nothing looked.

- **The Manager asks whether the screen can be drawn on before it is handed over.** On a screen with no X server the display draws through GBM and EGL, the system's graphics libraries; where they do not work, a take left the screen black until the kiosk had it back, with one sentence of SDL's for a reason. Three things say it beforehand now.
  - **A probe the display runs itself**, `glass --probe-graphics`: the steps SDL takes, one by one and without drawing, so it runs while a kiosk holds the screen. The screen's card with its kernel driver and its connector, a GBM device on it, every EGL vendor library the system registers loaded by itself, an EGL display for the device, its initialising, and who renders, by the renderer's own name. It prints one line of JSON: whether it works, the step that failed, and why in the loader's own words, which libglvnd keeps to itself when a vendor library is there and will not load. A screen Mesa has no renderer for is drawn on in software, by the processor: that passes and is said. A screen the kernel does not drive through KMS at all, a framebuffer-only one, is said to be drawn on through an X server only.
  - **The kiosk's X server's own word**: its log says whether it draws with the GPU ("glamor X acceleration enabled on ...") or fell back to software ("eglGetDisplay() failed"), and the Manager reads it.
  - **Mesa's parts**, which belong together, and whether they are of one version.
- **A take that could not hold is held back, with the reason.** "Use glass-evo on the screen" asks afresh first, where the screen would be drawn on itself: when the probe fails nothing is turned off, and the Screen tab says what the probe found. The take stays possible at the user's own word, **Try glass-evo on the screen anyway**, for a check that should ever be wrong; a failed try gives the screen back as before. Where glass-evo draws on an X server brought up for it, the probe has no say.
- **The status sheet has a Graphics row** in its Screen section: whether drawing on the screen itself works and who renders (the GPU, or in software), the screen's driver and connector, what the kiosk's X server says of itself, and Mesa's version or the parts that differ. Copy as text and a report carry it, and the journal has a line of it when it changes. Find a problem names graphics that cannot draw as the cause, for a display that dies and for a screen that stays the kiosk's.
- Routes: `GET /api/screen` answers `graphics` and `owner.blocked`; `POST /api/screen/owner` takes `force: true` and may answer `GLASS.MANAGER_OWNER_NO_GRAPHICS` with the reason as `message`.

## [0.8.36] - 2026-10-04

Asked on the forum: what is the difference between "what the player's screen shows" and "the theme alone"?

- **The first choice of "The Face tab and Anymote show" says what it does.** It read "what the player's screen shows", which reads as a copy of the screen, the kiosk's own pages included; the browser views show Glass's theme and never a copy of the kiosk's screens, and the choice only says when glass-evo's clock and controls lie over the theme. It reads "the Glass interface while glass-evo holds the player's screen" now, in English, German and French, and the same on a bundle remote's settings page ("This remote shows"). The line under the choice names "the first choice" and says that the pages are never a copy of the kiosk's screens. Nothing changes in what the choices do, or in the configuration (`follow`, `face`, `theme`; on a remote `follow`, `always`, `off`).

## [0.8.35] - 2026-10-04

From the first real failure the reason of 0.8.28 was read in.

- **Mesa's notes that it has no `drirc` are not quoted as the reason.** A display that died as it started is started again with the graphics libraries' own words on, and what reads as a fault among them joins the reason on the Screen tab. Asked to speak, Mesa notes at every start that it cannot open `/etc/drirc` and `~/.drirc`, a settings file hardly any system has; "No such file or directory" read as a fault, and in the one report that came, those two lines were all the reason added, pointing at a file that does not matter. They are left out (`relaunch.reason`).

## [0.8.34] - 2026-10-04

For a player that gives one cover many addresses: the Squeezelite plugin stamps the address of a Lyrion server's cover with the time of every state it pushes, so each play, pause and change of volume names the same cover anew.

- **The cover on show stays while the same track's next address is fetched.** A cover went dark from the moment its address changed until the picture at the new address had arrived, which was right for a new track and wrong for the same one: the cover blinked at every pause, every change of volume and three seconds into every song. The picture on show is kept now for as long as the track is the same (its place, title, artist and album) and replaced when the next one is there; a new track's cover is waited for with none on show, as before (`intake::cover_to_show`). On the player's screen, on a remote and in the browser views.
- **The Manager's route for the browser views takes the addresses the player reported of late.** It fetched only the one address the player's state names now, so a page asking for the address it was told a moment before, with another state pushed in between, was answered "not found" and showed no cover until the next state. The last eight addresses the player reported are known to the route (`picture.reported`); anything the player did not report is refused as before.

## [0.8.33] - 2026-10-04

Reported by theme makers: the sample rate would not sit under the format icon.

- **The sample rate line is aligned as the theme says.** The line (`playinfo.samplerate.pos`) was always drawn from the left of its box, whatever the theme asked of its texts. PeppyMeter Screensaver placed it in its box as the meter's other texts are placed, by `playinfo.align` or `playinfo.center`, and that is what themes are drawn for: of the 1,554 meters of the collection that show the line, 1,368 centre their texts and give the line a box (`playinfo.samplerate.maxwidth`), so in all of them it stood left of where its maker put it. It is centred or right aligned in its box now as the meter says. A line with no width of its own in a meter that gives its texts one (`playinfo.maxwidth`) is aligned in a box as wide as the widest line expected, "-44.1 kHz 24 bit-" set in its own type, as PeppyMeter Screensaver measured it, and is not cut by that box. New: `playinfo.samplerate.align = left`, `center` or `right` aligns the line by itself, over the meter's word. `playinfo.type.align` is the format icon's and label's own, as before. A line with no box at all starts at its position.
- For a face built against Glass: `plot::Text` and `lead::TextSpec` have one field more, `box_as`, a text whose width is the box where `max_width` is zero; empty for none.

## [0.8.32] - 2026-10-04

Found while reading for 0.8.31: what is fetched to be shown was never let go.

- **The pictures fetched for the display are kept to the newest 48.** A cover arrives with every track, a fanart set with every artist, and none was ever dropped: on a player and on a remote each address left a file in the temp dir's `glass-art` for as long as the system was up, and in the Face tab or Anymote each left its picture in the page's memory for as long as the page was open. A player up for weeks and a tablet on the wall for days grew without bound, and faster with a source that stamps its cover's address with the time, as the Squeezelite plugin does at every state it pushes. Now the art cache keeps its newest 48 files (`prune_art`; a picture taken up again counts as new), and the host's table keeps 48 files in each of the folders that fill track by track, covers with fanart and the track folders' pictures, dropping what was asked for longest ago (`lead::vfs::bound`); the notes of pictures a player does not have are bounded with them. Forty-eight is a whole fanart set, thirty at most, with the covers beside it. What is on show is never dropped, and what is wanted again is fetched again. A theme's own files, fonts and icons are kept as before.

## [0.8.31] - 2026-10-04

Reported from a player of a Lyrion server: the cover showed on Volumio's own page and not in Anymote.

- **A cover is known by its content.** Album art was taken only from a server that called it an image in its `Content-Type`. The Squeezelite plugin hands a Lyrion server's covers on through a proxy of its own on the player, and that proxy passes the bytes with no `Content-Type` at all: a browser's own picture element shows such a cover, and Glass refused it, on the player's screen, on a remote display, and in the Face tab and Anymote, while covers that came by a plain address, a radio station's for one, showed. A picture is told by its first bytes now, as the display has always decoded it: JPEG, PNG, GIF or WebP is taken whatever the server calls it, and what is none of them is refused whatever it is called. In the display (`intake::picture_kind`) and in the Manager's route for the browser views (`manager/picture.js`, which passes the picture on under its own type). On a player whose screen glass-evo holds, the screen follows with the glass-evo built on this release; the Face tab and Anymote need only this Glass.

## [0.8.30] - 2026-10-02

A line of the log that named the wrong file.

- **The launcher names the binary it tried.** When the display's binary does not answer, the launcher said "expected" and then Glass's own path, with the architecture left empty where glass-evo is the display: `.../glass/bin//glass`, a file nobody had tried. It says "tried" now and the binary it ran, glass-evo's or Glass's.

## [0.8.29] - 2026-10-02

Found on an x86 player, by making glass-evo fail there as it had on the Raspberry Pi of the report.

- **A second try is as quick as the first.** The count of starts that died, which sets the wait before the next one, went on across a change of the screen's owner. A first try of "Use glass-evo on the screen" that could not hold gave the screen back in fourteen seconds, as 0.8.27 meant; a second try straight after waited 8, 16 and 32 seconds between its starts, and a third a minute each. The count and the wait begin again now when the screen is handed to glass-evo and when the kiosk gets it back: another display on another screen, of which the deaths before say nothing.

## [0.8.28] - 2026-10-02

From the same report: "SDL error: Can't load EGL/GL library on window creation" does not say what would not load.

- **A display that cannot open the screen says why.** Of a screen it cannot open SDL keeps one sentence, whichever step failed: a library that is not on the player, or the graphics driver refusing the screen's device. Three things tell them apart now. A display that died as it started is started again with the graphics libraries' own words switched on (`EGL_LOG_LEVEL=debug`, `LIBGL_DEBUG=verbose`), so the journal has what Mesa tried and what failed; a first start is as before. The line that says a display did not run, and the reason the Screen tab gives when the screen went back from glass-evo, put the display's last word first and after it what was said before that reads as a fault, each once (`relaunch.reason`); the Screen tab's reason may be 600 characters, from 300. And where the words speak of the graphics libraries, the system's loader is asked for EGL, OpenGL (or OpenGL ES) and the buffer manager, and one it does not know is named with the line that installs it.

## [0.8.27] - 2026-10-02

Reported from a Raspberry Pi 5 with an HDMI panel, where glass-evo could not start.

- **A take that cannot hold gives the screen back in seconds.** A display that dies as it starts is started again after a wait that grows, and that wait was counted from the screensaver's delay everywhere. On a screen that is the display's own the screensaver's delay has no part: the screen's watcher starts the display as soon as it is found gone. So where glass-evo was handed the screen and could not start, its three tries stood the screensaver's delay apart and more, and the screen stayed black for a minute and a half at a delay of thirty seconds before the kiosk had it back. On a screen of the display's own the waits begin at a second now (`relaunch.base`): three failed starts and the screen's return take about ten seconds. A display on a player with no kiosk is tried again after 1, 2, 4 seconds and so on up to a minute; over a kiosk nothing changes.

## [0.8.26] - 2026-10-02

Reported from a player whose meters rotate at random.

- **The fanart slideshow carries on across a change of meter.** The show was made anew whenever the meter on show changed, so with meters rotating, by a timer or with the title, the artist's pictures started again at every change: from the first picture, or from a new random one, with the interval begun again and what was remembered per artist lost. The show is the artist's now, not the meter's: its place among the pictures and the time of its last advance are kept when the meter changes, a meter with no place for fanart rests the show, and the next meter that has one takes it up where it was. The same on a remote display, in the Face tab and in Anymote. On a player whose screen glass-evo holds, the screen follows with the glass-evo built on this release.

## [0.8.25] - 2026-10-02

Found on an x86 player with glass-evo on its screen.

- **A theme's previews are the theme alone.** Where glass-evo held the player's screen, the previews on the Manager's Themes tab were rendered with glass-evo's clock, date and bar of controls over them: the render ran in the display's own environment, which there names glass-evo's binary and the screen as its own. A preview is rendered by Glass's own display now, with no face (`GLASS_BIN` and `GLASS_SCREEN_OURS` are left out of its environment), and a `--snapshot` session takes no face whatever binary runs it. Each preview carries the version that drew it, so after the upgrade the old ones are out of date: **Render missing previews** on the Themes tab, or a theme's Details, renders them again. A package's pictures never carried a face.

## [0.8.24] - 2026-10-02

For a remote display on Android built with a face; nothing changes on the player, or in the app as Glass ships it.

- **The Android app's entry is the display's own.** What the app's `SDL_main` did (the app's storage for the configuration and the cache, landscape, the display run as a remote) lived in the shell crate `glass-android`, where an app built with a face could not reach it. It is `glass::android::enter` now, taking the face the app was built with, or none; Glass's own app calls it with none and is the app it was.
- `remote/README.md`: the bundle exists for Windows (glass-evo 0.1.19) and for Android (glass-evo 0.1.20); on Android both flavours are the one app under the one key, and Android refuses an app built on an older Glass than the one installed.

## [0.8.23] - 2026-10-02

For a remote display on Windows built with a face; nothing changes on the player or on the standalone remote.

- **A face on Windows is given the local time.** The time of day a face is handed (`Wall::now`) was universal time on Windows: the system's zone was read on a player only. Windows itself is asked now (`SystemTimeToTzSpecificLocalTime`, by the zone's own rules for the date), not the C runtime, which reads a `TZ` variable of another system's making in its own way. The zone's name is not read there; its distance from universal time is.
- **The Windows installer installs either flavour and says which**, as the Linux one does since 0.8.21: `glass-evo.exe` in the archive is the bundle and is installed in the standalone's place, under the same name, with the same Start menu entries; `-Check` names the flavour.

## [0.8.22] - 2026-10-02

The remote's settings page, for a remote built with a face; nothing changes on the player or on the standalone remote.

- **A remote says which flavour it is.** A face has a name (`Overlay::name`, none unless the face gives one). A display built with one names it under the page's title ("with glass-evo 0.1.18"), in `GET /api/state` (`face`, null on the standalone remote) and to the player in its `hello` line (`face`, left out by the standalone remote).
- **When the face shows is chosen on the page.** On a remote built with a face the page has "The Glass interface": what the player's screen shows, the Glass interface always, or the theme alone (`face` in the configuration: `follow`, `always`, `off`). The Status says whether it is shown now. The choice applies while the display runs, like every other setting. The standalone remote's page is as it was.

## [0.8.21] - 2026-10-02

The remote's installer and its instructions; nothing changes on the player.

- **A remote comes in two flavours, and the installer says which it installed.** Standalone, from Glass's releases: the player's theme and nothing over it, as before. Bundle, from glass-evo's releases: the same display with the Glass interface in it. The Linux installer installs whichever its archive holds, in the same place under the same name, so one takes the other's place and the settings, the cache and the menu entries stay; it names the flavour, what it shows, that the controls need a touch screen or a mouse, and where the other flavour is. `remote/README.md` opens with the two flavours and a table of what each platform has; the Windows installer says that its archive is the standalone and that Anymote shows the Glass interface in a browser until the bundle is built for Windows.

## [0.8.20] - 2026-10-02

A remote display can carry a face. Nothing changes for the remote Glass ships, which carries none.

- **A remote's sessions take the face the display was built with.** A display built with a face over it (`glass::run_with`, as glass-evo is) lost it when started as a remote: a remote's session was given none. It is handed on now, and a face on a remote has the window as it has a screen of the display's own: it shows its clock while the player stands still, the picture behind it goes black past the countdown as on the player, and a touch beside it does nothing, where a remote without a face plays or pauses on a touch.
- **The remote knows whose the player's screen is, and brings the look.** `GET /api/remote/config` gains `face`: `owner` (`glass-evo` where its face holds the player's screen) and `theme`, the face theme the player's settings name as `name` and `text` (the user's before a shipped one of its name; null where none is named). The remote keeps the text at `faces/<name>/face.txt` in its home, where a face reads its themes, and nothing else there. The configuration's version covers both, so a remote starts its session again when the screen changes hands or the look changes. The face's own settings travelled already, in the meter configuration.
- **When a remote shows its face** is the remote's to say: `face` in its configuration, `follow` (the default: where the player's own screen shows it), `always` or `off`. A remote's log says which it is and why.

## [0.8.19] - 2026-10-02

No change in what is shown; one change in how it is made.

- **The screen and the browser views lay a face over the picture with the same code.** The display's loop and the browser's pipeline each had their own copy of it: when a player standing still goes black, the black picture kept between frames, the copy a face draws on, and when a standing face over a standing picture is not drawn again. It is one piece now, in the `overlay` crate (`stands_black`, `Black`, `Laid`), with its own tests, and both call it, so what a face shows on the player's screen and in the Face tab or Anymote cannot come to differ there. For a face built against Glass: `overlay::Laid` and its companions are new; nothing a face is written against has changed.

## [0.8.18] - 2026-10-02

One fix, for a player whose screen glass-evo took from a running kiosk.

- **The kiosk is left stopped, not failed, and Find a problem does not blame it.** On a player whose image runs its own kiosk (x86), handing the screen to glass-evo stopped the kiosk's unit, whose `startx` leaves with an error when its X server goes: the unit stood as failed for as long as glass-evo held the screen. Find a problem then named it as the cause for "the screen", "the meters never appear" and "the display keeps restarting", and advised turning the Touch Display plugin off and on, with nothing wrong. The take now clears the unit's state once it has stopped it, and the diagnosis takes a failed kiosk for a cause only where the kiosk is meant to run. A player already in that state needs nothing done: the diagnosis is right at once, and the unit's state goes at the next take or restart.

## [0.8.17] - 2026-10-02

One fix, to how a display that cannot start is started again.

- **A display that dies at launch is tried again after a growing wait.** It was started again at the screensaver's own delay for as long as music played: with a delay of one second, a process and two lines in the player's log every second, without end. Each death at launch in a row now doubles the wait (1, 2, 4, 8 seconds and on, with a one-second delay), a minute at most unless the screensaver's delay is longer, and the log line says how many times in a row and when the next attempt comes. A display that ran for ten seconds, left by itself, or was ended by the plugin starts the count again, and so does a glass-evo component installed or put back. The same wait holds for the starts the screen's watcher makes on a screen that is the display's own.

## [0.8.16] - 2026-10-02

One fix, to the installer.

- **The display starts on a player that never had a kiosk.** On a fresh Volumio without the Touch Display plugin the display died at once, every time it was started, with `SDL error: EGL not initialized`. To draw on a screen of its own, with no X server, SDL loads EGL and OpenGL by name; its package does not depend on them, and the installer asked for SDL alone. A player with a kiosk has both through its X server, which is why this went unseen. The installer now brings `libegl1` and `libgl1` with `libsdl2-2.0-0` (about 5 MB), on a new install and on an upgrade, and says so in the install dialog when one of them could not be installed. On an earlier version: `sudo apt-get install -y libegl1 libgl1`.

## [0.8.15] - 2026-10-02

One fix, to a spectrum look laid out for two channels.

- **What plays no longer changes a look's layout.** A look with a dual layout drew as `single` whenever its two channels read alike: through a mono recording its two sides became one area across the box, in the single palette where the theme names only `palette.left` and `palette.right`, and came apart again at the next stereo track. 0.8.3 took silence out of that rule; the rule itself is gone. How many channels a look draws is the theme's to say: a section that asks for a one-channel bank (`channels = 1`) draws one, the two channels' mean; any other draws what its `layout` asks, and a recording alike on both channels shows the same on both sides.

## [0.8.14] - 2026-10-02

One fix, to the strings of the Status sheet.

- **"none" agrees with its row.** Four rows of the Status sheet (the last upgrade, the newest backup, the other plugins, the audio rings) shared one string for "none", and the strings files carried that string twice, so the later one stood for all four: in German "keine" for an upgrade, in French "aucun" for an upgrade and a backup. Each row has its own string now. The check refuses a strings file, or any JSON the plugin ships, in which a key is written twice in one object.

## [0.8.13] - 2026-10-02

One fix, to the Manager's downloads.

- **A download whose connection breaks is made again.** A Glass update, the glass-evo component or a theme pack whose connection was reset part way failed at once with "network", and the button had to be pressed again. Such a download is now made again from its first byte, after 2, 4 and 8 seconds, before the failure is reported; each new attempt is a line in the player's log. Only a broken connection or a failing server (a 5xx answer) is tried again: a file that is not the one its release or the catalog names, or an answer that it is not there, fails as before. Underneath, a connection that broke inside the body was not classed as a network failure by the Manager's own fetch; it is now, like one that breaks before the body.

## [0.8.12] - 2026-10-02

Clock faces in the look panel, for glass-evo 0.1.15.

- **The clock's face is chosen on the Screen tab.** Under Clock: the time in type as before, in 7 segments, in 16 segments, as a flip clock, or as a dial with hands, and for a dial its style (station, numbers, Roman numerals, marks only). Behind "More": how much of an unlit segment shows; a dial's hands, its marks and numerals, its second hand and the disc behind them, each as the face comes or in a colour of the user's; a flip clock's cards. The rows show where the glass-evo installed knows clock faces (0.1.15 or later).
- **The likeness shows a drawn clock as glass-evo draws it.** The panel brings the module the glass-evo component carries for a browser and has it draw the clock alone (`clock_preview`), at the size and in the colours being chosen, before anything is saved: the same code that draws it on the screen. A clock in type is set by the page as before.

## [0.8.11] - 2026-10-02

One change, to where the picture may be placed.

- **A position may be negative.** With the position set by hand, X and Y were taken from 0 up: a theme could be pushed right and down from the screen's corner, never left or up. The display always took a signed position; the Manager's Appearance tab, the plugin's check behind it and the settings page did not. X now runs from -7680 to 7680 and Y from -4320 to 4320 in the Manager (-3840 to 3840 and -2160 to 2160 on the settings page), so a theme larger than the screen, or a tailored one with a margin drawn into it, can be laid where it looks right.

## [0.8.10] - 2026-10-02

The Face tab and Anymote show glass-evo's face, with glass-evo 0.1.14 or later.

- **The views show what the player's screen shows.** Where glass-evo holds the screen, the Face tab and Anymote bring the module the glass-evo component carries, Glass's pipeline with the face over the theme: the clock and the date when the player stands still, the bar and its sheet at a touch, in the look and the sizes chosen for the player, drawn by the same code that draws them on the screen. A touch on the bar acts on the player. The page's own banner about a player standing still is left out there, the clock saying it.
- **What the views show is the user's to choose**, on the Screen tab where a glass-evo that carries its face for a browser is installed: what the player's screen shows (the face under glass-evo, the theme alone under the kiosk; as it comes), the Glass interface always (the face in the browser even while the player's own screen keeps the kiosk: a tablet on Anymote is then a Glass interface by itself), or the theme alone, as before. A page that is open brings the other module when the choice or the screen's owner changes.
- The System tab's installer carries the browser module out of the component, checked against the digest the manifest names; a component that names one and does not hold it, or holds another, is not installed.
- `GET /api/face/module` says which module a page brings and gives the face theme's text; `POST /api/face/views` with `{"mode": "follow" | "face" | "theme"}` sets the choice; `GET /api/screen` says `views`; the feed carries a `views` line when what the pages should carry changes.

## [0.8.9] - 2026-10-02

Groundwork for showing glass-evo's face in the Face tab and Anymote. Nothing changes on a player or in a page yet.

- **The contract of a face is a crate of its own, free of the window.** `Overlay`, `View`, `Cover` and the types a face is written against moved from the display's crate into `crates/overlay`, which builds for a browser as it does for a player; `glass` offers them as before, so a face written against `glass::` reads unchanged. `View` gains `wall`, the time of day where the player is, broken down (`Wall`): the display reads it from the system once a frame, and a face no longer has to ask the system itself.
- **The browser module is a library with room for a face.** `crates/page` is the pipeline the Face tab and Anymote run, as before, and now takes a face over the theme: asked what it covers, drawn on its own copy of the frame, offered every pointer event before the theme's controls, with black behind it once the player has stood still past the countdown, as on a screen of the display's own. `page::exports!` writes the module's raw exports into whichever crate is its root; `glass-face` is that one line, and a module that carries a face is the same line with the face named. Three exports are new: `zone` (the page's minutes east of universal time and the zone's name), `taken` (what a face asked of the player at a frame) and `overlaid`.
- The page tells the module its zone and takes what was asked after every frame; with no face in the module both do nothing.

## [0.8.8] - 2026-10-02

One change, to the look panel of the Manager's Screen tab, for glass-evo 0.1.13.

- **A clock as large as you set it.** glass-evo 0.1.13 draws a clock or a date at the size the user set, whatever it runs over, where it used to set it smaller to keep a margin and the glass's room. The size controls of the clock and the date now reach four times the look's own size (they stopped at two and a half), enough for any clock format to fill the screen; the likeness shows a size of the user's own as set, running over its edges as it will over the screen's, with the glass giving up its room as the words take more; and it fits a size the look comes with as the face now does, in the whole width. This Glass works with glass-evo 0.1.13 or later, and the System tab offers it.

## [0.8.7] - 2026-10-02

One fix, completing 0.8.6, which was published as a test release and not made the latest.

- **A page opened on a player that already stood still is told that the display stays.** 0.8.6 sent the word with the persist period, which starts at a stop or a pause: on a player that had stood still since before the plugin started, after an update for one, no period had ever been sent, and the Face tab and Anymote still said the display had left the screen. The plugin now says it once after its start, whatever the player does, and again when the screen changes hands.

## [0.8.6] - 2026-10-02

One fix, to what the Face tab and Anymote say of a player that stands still.

- **The pages no longer say the display has left the screen where it has not.** Under the kiosk the display leaves the screen when the persist period ends after a stop or a pause, and the Face tab and Anymote say so over the standing picture. On a screen that is the display's own, with glass-evo holding it or with no kiosk at all, the display stays, and the pages told the same story all the same. The line that carries the persist period to a page now says whether the display stays (`stays`), told again when the screen changes hands; the pages then say only that the player is stopped or paused and that the meters move again when it plays. The first of two steps: the pages do not yet show glass-evo's clock and bar.

## [0.8.5] - 2026-10-02

One addition, to how releases reach a player.

- **Test releases.** From the next release on, a release of Glass or of glass-evo is published as a pre-release, tried on the project's own players, and then made the latest; until that last step no player is offered it. The System tab has a switch, "Offer test releases", for a player that wants them at once: with it on, the Manager looks at the repository's last ten releases and offers the newest, a pre-release among them (marked "test release" on the tab), for Glass and for glass-evo alike; with it off, as on every player until someone turns it on, it asks for the latest as before. The choice is looked up the moment it is made. `POST /api/update/test` with `{"test": true}` sets it, `/api/update` and `/api/evo` say `test`, and the status sheet's housekeeping says which releases the player is offered.

## [0.8.4] - 2026-10-02

One fix, to the look panel of the Manager's Screen tab.

- **The likeness shows the clock as large as the screen will, and no larger.** glass-evo sets a line of the idle screen no wider than the picture leaves beside its glass and its margins, the date no taller than a quarter of the picture, and the clock in what the date and the bar leave, never taller than half. The likeness set them at the size asked for, however large: a clock pushed up to the widest ran from edge to edge in the Manager and stood inside its margins on the player. The likeness now makes the same fit (`fitted` and `idleMost` in the look model), again whenever the panel changes size, and its glass keeps the margins the screen's has.

## [0.8.3] - 2026-10-02

One fix, to the analyser while the player stands still.

- **A two-sided look keeps its two sides through silence.** The display takes two channels that read the same for a one-channel analyser and draws them as one, whatever layout the look asks for. Stopped or paused, both channels read nothing, which is the same too: the look fell back to one area in the one-channel palette. Bars showed nothing of it; a waterfall wrote its silent rows across both sides in that palette's lowest colour, the classic palette's green in a look that names only `palette.left` and `palette.right`. Silence now leaves the layout as the look asks. On the player's screen, in the Face tab, in Anymote and on remotes alike; a face built on this Glass has it, which for glass-evo is 0.1.12.

## [0.8.2] - 2026-10-02

The display does less where there is nothing new to show, and the status sheet says more of the player. The figures are a Raspberry Pi 5 at sixty frames a second with a spectrum across the whole screen, glass-evo on the screen; a face built on this Glass has them, which for glass-evo is 0.1.11.

- **A face costs nothing while it draws nothing.** The display made a new copy of the whole picture for a face on every frame, whether the face drew on it or not: megabytes asked of the system sixty times a second. The face is now asked first (`Overlay::covers`), and the copy it draws on is kept from frame to frame. Steady play: 52 percent of a core before, 46.5 after, the same as the display with no face.
- **A picture that stands is not sent again.** Black, a clock that says the same minute, a bar nobody touches: where the same things lie over a picture that did not move, the window is left as it is. A face says so with `Cover::Same`.
- **A player standing still is drawn fifteen times a second.** On a screen that is the display's own, three seconds after the player last played or the screen was last touched, the display slows to fifteen frames a second, and is back at its full rate with the next touch or the next note. Paused with a clock on the screen: 60 percent of a core before, 24 after.
- **Nothing is drawn behind a black screen.**
- **The status sheet's Player section** has the player's memory (all of it, in use, available, and swap in use) and the other plugins installed, each with its version, whether it is on, and whether it puts itself in the audio path. Both are in a report made from "Find a problem", where a board's name said nothing of its memory and a DSP plugin went unseen.

## [0.8.1] - 2026-10-01

On x86 a theme of another size than the screen fills the screen under glass-evo. The plain X server brought up for glass-evo ran no window manager, and it is the window manager that makes a full screen window the size of the screen: the face's window kept the theme's size, so a 1920x1080 theme on a 1280x720 screen was cut off, a smaller one stood small, and "fit the theme to the screen" did nothing. The session now runs the window manager the kiosk's own session runs (openbox). Give the screen back and hand it over again for it to take effect.

## [0.8.0] - 2026-10-01

glass-evo, the Glass interface for the player's own screen, can be got and chosen from the Manager. It is a preview: verified on a Raspberry Pi 5 with a DSI screen and the Touch Display plugin, and on an x86 player; other screens are untried. Nothing changes on a player that does not get it, where the kiosk stays the interface as before.

- **Getting it.** The System tab has glass-evo under Glass's own release: get it, update it, go back to the version before, remove it. The release is read from GitHub, its zip checked against the release's checksum, and this player's binary checked against the component's own list; the version in place is kept for going back. It is removed only while the kiosk owns the screen.
- **Versions that go together.** The two name the least of each other they work with: the component the least Glass, Glass the least glass-evo (0.1.10 for this release). A component outside either is not installed. An upgrade of Glass that needs a newer glass-evo installs that first, and puts it back if the upgrade does not go in.
- **The screen's owner.** On the Screen tab the screen is handed to glass-evo and back. A take turns the kiosk off, with every change recorded, and the way back restores exactly what was changed.
- **On x86 the X server stays.** There the picture may reach the screen through X alone, with no kernel driver for the graphics card. A take stops the kiosk and its browser and brings up a plain X server in their place, with glass-evo as its only client; the way back stops it before the kiosk starts its own. A Raspberry Pi keeps drawing on the screen itself, with no X server at all.
- **No take where there is nothing to draw on.** A player with neither a screen the kernel drives nor an X server is told so, and nothing is turned off.
- **A screen that goes back by itself.** While glass-evo owns the screen the kiosk is off, so the screen returns to the kiosk without being asked when the face cannot hold it: it failed to start three times in a row, its component is gone, or Glass was turned off or removed. An upgrade of Glass keeps the screen. The Screen tab says why it went back.
- **How the screen looks**, the face size and the owner on the status sheet, as 0.7.97 brought them, on a player that has glass-evo.
- **Routes.** `GET /api/evo`, `POST /api/evo/check`, `/api/evo/install`, `/api/evo/rollback`, `/api/evo/remove`; `GET /api/screen` carries how the screen would be held (`owner.mode`, `owner.holdable`) and whether an X server of Glass's own is up (`ownX`).

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
