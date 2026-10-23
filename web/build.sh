#!/usr/bin/env bash
# Builds every web app into target/site/, the folder GitHub Pages serves at
# play.contra-ethos.com. Each app lands in its own subfolder: /sandbox/, later /dice/, /blob/.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/target/site"
apps=(sandbox)

# Empty the folder rather than deleting it, so a local server running inside it survives.
mkdir -p "$out"
find "$out" -mindepth 1 -delete
for app in "${apps[@]}"; do
  # Trunk has to run from the app's own folder. MSYS_NO_PATHCONV stops Git Bash on
  # Windows from rewriting "/sandbox/" into "C:/Program Files/Git/sandbox/"; the
  # --dist path is relative, so it needs no conversion either.
  (cd "$root/apps/$app" && MSYS_NO_PATHCONV=1 trunk build --release --public-url "/$app/" --dist "../../target/site/$app")
done
cp "$root/web/index.html" "$root/web/CNAME" "$out/"
echo "site built in $out"
