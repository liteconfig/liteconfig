# Homebrew formula for liteconfig.
#
# Distributed via a custom tap:
#   brew tap liteconfig/tap
#   brew install liteconfig
#
# Release note: update `version` and the four `sha256` values after publishing
# GitHub release assets. `SHA256SUMS` contains the values for each target.

class Liteconfig < Formula
  desc "Fast TUI for syncing AI coding agent configs, skills, MCP, and rules"
  homepage "https://github.com/liteconfig/liteconfig"
  version "0.1.0"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/liteconfig/liteconfig/releases/download/v#{version}/liteconfig-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256_AARCH64_APPLE_DARWIN"
    end
    on_intel do
      url "https://github.com/liteconfig/liteconfig/releases/download/v#{version}/liteconfig-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256_X86_64_APPLE_DARWIN"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/liteconfig/liteconfig/releases/download/v#{version}/liteconfig-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_SHA256_AARCH64_LINUX_GNU"
    end
    on_intel do
      url "https://github.com/liteconfig/liteconfig/releases/download/v#{version}/liteconfig-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_SHA256_X86_64_LINUX_GNU"
    end
  end

  def install
    bin.install "liteconfig"
  end

  test do
    assert_match "liteconfig", shell_output("#{bin}/liteconfig --version")
  end
end
