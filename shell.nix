{
  pkgs ? import <nixpkgs> { },
}:
pkgs.mkShell {
  nativeBuildInputs = with pkgs.buildPackages; [
    cargo
    rustc
    rustfmt
    clippy
    # Used only to render report/report.md to .docx for submission:
    #   pandoc report/report.md -o report/report.docx
    pandoc
  ];
}
