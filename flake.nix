{
  description = "CultureAndArts: a TUI that finds the ideal Google Arts & Culture wallpaper for your desktop";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f (import nixpkgs { inherit system; }));

      nativeDeps = pkgs: [ pkgs.pkg-config ];
      buildDeps = pkgs:
        [ pkgs.openssl ]
        ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
          pkgs.darwin.apple_sdk.frameworks.Security
          pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
          pkgs.darwin.apple_sdk.frameworks.CoreFoundation
        ];
    in
    {
      packages = forAllSystems (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "culture-and-arts";
          version = "0.1.0";
          src = self;

          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = nativeDeps pkgs;
          buildInputs = buildDeps pkgs;

          OPENSSL_NO_VENDOR = 1;

          meta = with pkgs.lib; {
            description = "TUI to find the ideal Google Arts & Culture wallpaper for your desktop";
            homepage = "https://github.com/grathwohl/culture-and-arts";
            license = licenses.gpl3Plus;
            mainProgram = "culture-and-arts";
            platforms = systems;
          };
        };
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          nativeBuildInputs = nativeDeps pkgs ++ [
            pkgs.rustc
            pkgs.cargo
            pkgs.clippy
            pkgs.rustfmt
            pkgs.rust-analyzer
          ];

          buildInputs = buildDeps pkgs ++ [ pkgs.fontconfig ];

          OPENSSL_NO_VENDOR = 1;
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };
      });

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.system}.default}/bin/culture-and-arts";
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixpkgs-fmt);
    };
}
