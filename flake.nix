{
  description = "Oma Bible - native Bible reader for Omarchy Linux";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustc
            cargo
            rustfmt
            clippy
            pkg-config
            openssl
            # GPUI may require additional platform libraries (graphics/display).
            # Add them here as needed for your system.
          ];

          shellHook = ''
            echo "Oma Bible development environment (TUI + GPUI)"
          '';
        };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "oma-bible";
          version = "0.1.1";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = with pkgs; [ openssl ];
        };
      }
    );
}
