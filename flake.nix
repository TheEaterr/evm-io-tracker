{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  };

  outputs = { self, nixpkgs }:
    let
      # Systems supported
      allSystems = [
        "x86_64-linux" # 64-bit Intel/AMD Linux
        "aarch64-linux" # 64-bit ARM Linux
        "x86_64-darwin" # 64-bit Intel macOS
        "aarch64-darwin" # 64-bit ARM macOS
      ];

      # Helper to provide system-specific attributes
      forAllSystems = f:
        nixpkgs.lib.genAttrs allSystems
        (system: f { 
            pkgs = import nixpkgs { inherit system; }; 
        });

      overrides = (builtins.fromTOML (builtins.readFile ./rust-toolchain.toml));
    in {
      # Development environment output
      devShells = forAllSystems ({ pkgs }: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            rustup
            rustPlatform.bindgenHook
          ];
        };
      });
    };
}