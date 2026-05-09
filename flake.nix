{
  description = "frostx - project lifecycle manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs";
  };

  outputs =
    { self, nixpkgs }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      pkgsFor = system: nixpkgs.legacyPackages.${system};
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "frostx";
            version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;

            src = ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            # git: unit tests; pandoc: section-5 man pages; installShellFiles: completion install hook.
            nativeBuildInputs = [ pkgs.git pkgs.pandoc pkgs.installShellFiles ];

            # Ensure git tests can run in the sandbox without a global git config.
            preCheck = ''
              export HOME=$(mktemp -d)
              git config --global user.email "test@example.com"
              git config --global user.name "Nix Test"
            '';

            postInstall = ''
              # Section 1: generate CLI man pages via the gen_man helper binary.
              mkdir -p $out/share/man/man1
              $out/bin/gen_man $out/share/man/man1
              rm $out/bin/gen_man

              # Section 5: convert user-facing reference docs to man format.
              mkdir -p $out/share/man/man5
              pandoc docs/user/frostx-toml.md -s -t man \
                --metadata title="FROSTX.TOML" --metadata section="5" \
                --metadata author="0xCCF4" \
                -o $out/share/man/man5/frostx.toml.5
              pandoc docs/user/actions.md -s -t man \
                --metadata title="FROSTX-ACTIONS" --metadata section="5" \
                --metadata author="0xCCF4" \
                -o $out/share/man/man5/frostx-actions.5
              pandoc docs/user/includes.md -s -t man \
                --metadata title="FROSTX-INCLUDES" --metadata section="5" \
                --metadata author="0xCCF4" \
                -o $out/share/man/man5/frostx-includes.5
              pandoc docs/user/state.md -s -t man \
                --metadata title="FROSTX-STATE" --metadata section="5" \
                --metadata author="0xCCF4" \
                -o $out/share/man/man5/frostx-state.5

              # Shell completions via gen_completions helper binary.
              compl=$(mktemp -d)
              $out/bin/gen_completions "$compl"
              installShellCompletion \
                --bash "$compl/frostx.bash" \
                --zsh "$compl/_frostx" \
                --fish "$compl/frostx.fish"
              rm $out/bin/gen_completions
            '';

            meta = with pkgs.lib; {
              description = "Project lifecycle manager - archives and cleans inactive project folders";
              license = licenses.gpl3;
              mainProgram = "frostx";
            };
          };
        }
      );

      checks = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          unit-tests = self.packages.${system}.default.overrideAttrs (_: {
            pname = "frostx-unit-tests";
            doCheck = true;
          });
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          default = pkgs.mkShell {
            inputsFrom = [ self.packages.${system}.default ];
            packages = with pkgs; [
              rustfmt
              clippy
              rust-analyzer
            ];
          };
        }
      );
    };
}
