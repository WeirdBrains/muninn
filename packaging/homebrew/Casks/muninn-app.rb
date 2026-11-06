# Homebrew cask for the Flutter Muninn.app (universal), from the
# muninn-app-<ver>-universal-apple-darwin.dmg that scripts/flutter-dist.sh writes.
# Published in the tap github.com/WeirdBrains/homebrew-tap as Casks/muninn-app.rb:
#   brew install --cask weirdbrains/tap/muninn-app
# scripts/fill-sums.py fills in the sha256 when that dmg is listed in SHA256SUMS.
# The GitHub Releases URL is the asset name after a release; this recipe does not
# cut a tag or invent a published download.
#
# Local eyeball (Homebrew 7+ rejects a raw .rb path; copy this file into a local
# tap and point url/sha256 at the built dmg):
#   brew tap-new --no-git "$USER/muninn-local"   # once
#   cask="$(brew --repo "$USER/muninn-local")/Casks/muninn-app.rb"
#   cp packaging/homebrew/Casks/muninn-app.rb "$cask"
#   dmg=/absolute/path/to/muninn-app-<ver>-universal-apple-darwin.dmg
#   sha=$(shasum -a 256 "$dmg" | awk '{print $1}')
#   # in the copy: url "file://$dmg" and sha256 "$sha" (do not commit that copy)
#   brew install --cask --appdir=... "$USER/muninn-local/muninn-app"
# Conflicts with the TUI cask (weirdbrains/tap/muninn), which also installs Muninn.app.
# Does not install the mnnn CLI (use the muninn formula for that).
cask "muninn-app" do
  version "2.22.0"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"

  url "https://github.com/WeirdBrains/muninn/releases/download/v#{version}/muninn-app-#{version}-universal-apple-darwin.dmg"
  name "Muninn"
  desc "Graphical Muninn workbench for market data"
  homepage "https://muninn.weirdbrains.com"

  conflicts_with cask: "muninn"

  app "Muninn.app"

  caveats <<~EOS
    This is the Flutter desktop app, not the Terminal TUI wrapper. It does not
    install the mnnn command (brew install weirdbrains/tap/muninn for that).

    Muninn.app is not notarized. If macOS blocks it, right-click Muninn.app in
    /Applications and choose Open, or run:
      xattr -dr com.apple.quarantine /Applications/Muninn.app
  EOS
end
