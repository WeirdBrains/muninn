# Homebrew formula for Muninn (mnnn): installs prebuilt release binaries, no Rust needed.
# Published in the tap github.com/WeirdBrains/homebrew-tap as Formula/muninn.rb:
#   brew install weirdbrains/tap/muninn   (scripts/fill-sums.py fills in the sums)
class Muninn < Formula
  desc "Data goes in, your magic transforms it"
  homepage "https://github.com/WeirdBrains/muninn"
  version "2.25.0"
  license "BSD-3-Clause"

  base = "https://github.com/WeirdBrains/muninn/releases/download/v#{version}"
  on_macos do
    on_arm do
      url "#{base}/muninn-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "#{base}/muninn-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end
  on_linux do
    on_arm do
      url "#{base}/muninn-#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "#{base}/muninn-#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    bin.install "mnnn"
    man1.install Dir["man/*.1"]
    doc.install "README.md", "CHANGELOG.md", "docs"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mnnn --version")
    assert_match "Install", shell_output("#{bin}/mnnn help install")
  end
end
