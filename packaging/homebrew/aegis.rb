class Aegis < Formula
  desc "Terminal-native agentic AI pair programmer for CUI environments"
  homepage "https://github.com/rtmx-ai/aegis-cli"
  version "VERSION_PLACEHOLDER"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/rtmx-ai/aegis-cli/releases/download/vVERSION_PLACEHOLDER/aegis-VERSION_PLACEHOLDER-macos-aarch64.tar.gz"
      sha256 "SHA256_PLACEHOLDER_ARM"
    end
    on_intel do
      url "https://github.com/rtmx-ai/aegis-cli/releases/download/vVERSION_PLACEHOLDER/aegis-VERSION_PLACEHOLDER-macos-x86_64.tar.gz"
      sha256 "SHA256_PLACEHOLDER_INTEL"
    end
  end

  def install
    bin.install "aegis"
  end

  test do
    system "#{bin}/aegis", "--version"
  end
end
