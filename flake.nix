{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, }:
    flake-utils.lib.eachDefaultSystem
      (system:
        let
	  pkgs = import nixpkgs { inherit system; };
        in 
        with pkgs;
	{
	  devShells.default = with pkgs; mkShell rec {
	    nativeBuildInputs = with pkgs; [
	      pkg-config
	    ];

	    buildInputs = with pkgs; [
	      pkg-config
	      xorg.libX11
	      xorg.libXcursor
	      xorg.libXrandr
	      xorg.libXi
	      xorg.libxcb
	      libxkbcommon
	      vulkan-loader
	      wayland
	      pulseaudio
	      linuxPackages.perf
	    ];

	    shellHook = ''
	      export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${builtins.toString (pkgs.lib.makeLibraryPath buildInputs)}";
	    '';
	  };
	}
      );
}
