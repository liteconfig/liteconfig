# Homebrew formula template for liteconfig.
#
# Public install:
#   brew tap liteconfig/tap
#   brew install liteconfig
#
# Release automation renders this template into
# liteconfig/homebrew-tap/Formula/liteconfig.rb using the GitHub release assets
# and SHA256SUMS manifest from the tagged build.

class Liteconfig < Formula
  desc "Fast TUI for syncing AI coding agent configs, skills, MCP, and rules"
  homepage "https://github.com/liteconfig/liteconfig"
  version "__VERSION__"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/liteconfig/liteconfig/releases/download/v#{version}/liteconfig-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "__SHA256_AARCH64_APPLE_DARWIN__"
    end
    on_intel do
      url "https://github.com/liteconfig/liteconfig/releases/download/v#{version}/liteconfig-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "__SHA256_X86_64_APPLE_DARWIN__"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/liteconfig/liteconfig/releases/download/v#{version}/liteconfig-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "__SHA256_AARCH64_LINUX_GNU__"
    end
    on_intel do
      url "https://github.com/liteconfig/liteconfig/releases/download/v#{version}/liteconfig-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "__SHA256_X86_64_LINUX_GNU__"
    end
  end

  def install
    bin.install "liteconfig"
  end

  test do
    assert_match "liteconfig", shell_output("#{bin}/liteconfig --version")
  end
end
