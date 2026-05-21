class Aegis < Formula
  desc "Terminal-native agentic AI pair programmer for CUI environments"
  homepage "https://github.com/rtmx-ai/aegis-cli"
  version "VERSION_PLACEHOLDER"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/rtmx-ai/aegis-cli/releases/download/vVERSION_PLACEHOLDER/aegis-VERSION_PLACEHOLDER-macos-aarch64.tar.gz"
      sha256 "SHA256_PLACEHOLDER_MACOS_ARM"
    end
    on_intel do
      url "https://github.com/rtmx-ai/aegis-cli/releases/download/vVERSION_PLACEHOLDER/aegis-VERSION_PLACEHOLDER-macos-x86_64.tar.gz"
      sha256 "SHA256_PLACEHOLDER_MACOS_INTEL"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/rtmx-ai/aegis-cli/releases/download/vVERSION_PLACEHOLDER/aegis-VERSION_PLACEHOLDER-linux-aarch64.tar.gz"
      sha256 "SHA256_PLACEHOLDER_LINUX_ARM"
    end
    on_intel do
      url "https://github.com/rtmx-ai/aegis-cli/releases/download/vVERSION_PLACEHOLDER/aegis-VERSION_PLACEHOLDER-linux-x86_64.tar.gz"
      sha256 "SHA256_PLACEHOLDER_LINUX_INTEL"
    end
  end

  def install
    bin.install "aegis"
  end

  test do
    assert_match "aegis", shell_output("#{bin}/aegis --version")
  end
end
