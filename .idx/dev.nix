{ pkgs, ... }: {
  channel = "stable-24.05";
  packages = [
    pkgs.gcc
    pkgs.rustup
    pkgs.flatbuffers
    pkgs.bun
    pkgs.pnpm
    pkgs.tree
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
        install = "rustup default stable && rustup update && rustup target add wasm32-wasip1-threads && cargo update -p ctor && cargo run";
        default.openFiles = [
          "README.md"
        ];
      };
    };
  };
}