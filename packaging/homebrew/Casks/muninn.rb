# Homebrew cask for Muninn.app (universal), from the release .dmg. Published in the tap
# github.com/WeirdBrains/homebrew-tap as Casks/muninn.rb:
#   brew install --cask weirdbrains/tap/muninn   (scripts/fill-sums.py fills in the sum)
cask "muninn" do
  version "2.24.0"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"

  url "https://github.com/WeirdBrains/muninn/releases/download/v#{version}/muninn-#{version}-macos.dmg"
  name "Muninn"
  desc "Data goes in, your magic transforms it"
  homepage "https://muninn.weirdbrains.com"

  conflicts_with cask: "muninn-app"

  app "Muninn.app"
  binary "#{appdir}/Muninn.app/Contents/Resources/mnnn"

  caveats <<~EOS
    Muninn.app is not notarized. If macOS blocks it, right-click Muninn.app in
    /Applications and choose Open, or run:
      xattr -dr com.apple.quarantine /Applications/Muninn.app
  EOS
end
