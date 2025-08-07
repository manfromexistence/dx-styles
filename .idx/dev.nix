{ pkgs, ... }: {
  channel = "stable-24.05";
  packages = [
    pkgs.gcc
    pkgs.rustup
    pkgs.flatbuffers
  ];
  env = { };
  idx = {
    extensions = [
      "pkief.material-icon-theme"
      "ziglang.vscode-zig"
      "tamasfe.even-better-toml"
      "rust-lang.rust-analyzer"
    ];
    workspace = {
      onCreate = {
        install = "rustup default stable && rustup update && cargo run";
        default.openFiles = [
          "README.md"
        ];
      };
    };
  };
}