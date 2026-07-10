class Ws < Formula
  desc "Workspace memory for SSH, tmux, and AI coding agents"
  homepage "https://github.com/LeON-Nie-code/tmux-workbench"
  url "https://github.com/LeON-Nie-code/tmux-workbench.git",
      tag: "v0.1.2"
  license "MIT"
  head "https://github.com/LeON-Nie-code/tmux-workbench.git", branch: "master"

  depends_on "rust" => :build
  depends_on "git"
  depends_on "tmux"

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "Tmux Workbench", shell_output("#{bin}/ws --help")
  end
end
