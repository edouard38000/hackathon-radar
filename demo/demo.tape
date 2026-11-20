# The README demo. Render it from the repo root, after a release build:
#
#   cargo build --release --locked
#   vhs demo/demo.tape
#
# It writes demo/demo.gif (embedded in the README) and demo/demo.mp4. It needs
# VHS 0.12.1 or later (brew install vhs brings ttyd and ffmpeg).
#
# Nothing here touches the network, a real Agent, your keychain or your own
# Profile: HOME and the XDG directories point at a fresh temporary directory,
# so `init` writes its Profile and database there, and the Hackathons are the
# made-up ones in demo/hackathons.csv. Their dates are in 2027; move them on
# before rendering once they've passed.

Output demo/demo.gif
Output demo/demo.mp4

Require bash

Set Shell "bash"
Set FontSize 14
Set Width 1500
Set Height 1120
Set Padding 24
Set Theme "Catppuccin Mocha"
Set TypingSpeed 25ms
Set PlaybackSpeed 1.0

Hide
Type `export DEMO_REPO="$PWD" PS1='$ ' HISTFILE=/dev/null` Enter
Type `cd "$(mktemp -d)" && mkdir bin && cp "$DEMO_REPO/target/release/hackathon-radar" bin/ && cp "$DEMO_REPO/demo/hackathons.csv" .` Enter
# init only checks that the Agent's program is on the PATH; this stand-in is never run.
Type `printf '#!/bin/sh\nexit 1\n' > bin/claude && chmod +x bin/claude` Enter
Type `export PATH="$PWD/bin:/usr/bin:/bin" HOME="$PWD" XDG_CONFIG_HOME="$PWD/.config" XDG_DATA_HOME="$PWD/.local/share" XDG_STATE_HOME="$PWD/.local/state"` Enter
Type "clear" Enter
Show

Type "hackathon-radar --help" Sleep 300ms Enter
Sleep 4s

Type "clear" Enter
Type "# Set up a Profile: Home in Brussels, Europe and the US West Coast." Enter
Type `hackathon-radar init --yes --home-city Brussels --countries "Schengen, US West Coast" --themes "AI agents, climate" --alert-channel none` Sleep 300ms Enter
Sleep 3s
Type "cat ~/.config/hackathon-radar/profile.toml" Sleep 300ms Enter
Sleep 4s

Type "clear" Enter
Type "# Bring in hackathons you already track, then see what fits." Enter
Type "hackathon-radar import-sheet hackathons.csv" Sleep 300ms Enter
Sleep 3s
Type "clear" Enter
Type "hackathon-radar report" Sleep 300ms Enter
Sleep 8s

Type "clear" Enter
Type "# Deadlines into your calendar." Enter
Type "hackathon-radar export ics --out deadlines.ics" Sleep 300ms Enter
Sleep 3s
