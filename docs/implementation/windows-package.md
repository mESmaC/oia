# Local Windows development packages

The `#49` packaging slice creates an unsigned development ZIP from a clean Git
revision. It is a local build tool, not an installer or a public release gate.
ADR [0003](../adr/0003-d14-licensing-and-distribution.md) still defers signing,
installer formats and updates. Integrated A02/A17 acceptance and the rest of
`#49` remain open. This tool does not change UX-04/UX-05 or REPRO-02 runtime behavior.

From a clean checkout on Windows with the pinned Rust toolchain, C++ build tools,
and dependencies cached by `scripts/run.ps1 fetch`:

```powershell
.\scripts\package-windows.ps1
.\scripts\package-windows.ps1 -Configuration Debug -WithoutGlobe
.\scripts\package-windows.ps1 -OutputPath C:\local-builds\oia-dev.zip
```

The default is a release-profile Windows x64 MSVC build with CesiumJS included.
Missing/incomplete vendor files or a mismatched version fail before compilation.
`-WithoutGlobe` explicitly omits the renderer; the Globe reports its absence while
other tools remain usable. Vendor files are copied into the build snapshot and
SHA-256 inventoried. Their version marker alone does not authenticate their bytes;
the fetch scripts verify the upstream tarball and apply the documented patch.

The script builds the committed source snapshot with `--locked --offline`, pins
Rust from that snapshot, and reuses dependencies under `target/package-windows/build`.
It excludes other ignored/untracked content. Temporary snapshots remain under
`target/package-windows/` for inspection, including after errors. Failed builds
do not create the destination ZIP, and an existing destination is never replaced.
A process lock covers building and copying, so concurrent packagers cannot mix
one revision's executable with another revision's manifest; a competing run fails.
The ZIP defaults to ignored `target/packages/`. A custom destination inside the
checkout may become untracked and must be moved before packaging again.

The archive includes `oia.exe`, both repository licenses, third-party notices,
README, Cargo.lock, usage/limits in PACKAGE.txt, and build-manifest.json. The
manifest records revision, UTC build time, compiler, target, profile, OS, Cargo
arguments, optional vendor inventory, and hashes/lengths for payload files other
than itself. It is not a signed attestation or evidence of bit-reproducible builds.
The archive hash is printed separately; there is no self-referential hash.

Extract to a new directory and run `oia.exe`. Microsoft WebView2 Runtime must
already be installed; nothing is downloaded or installed by this packager.
The snapshot README and PACKAGE.txt explain session-only data and online Globe
imagery. No case data, credentials, fixtures or basemap tiles are copied as
separate files. The application still embeds its existing synthetic UI fixtures.

Packaging does not run tests or assert release readiness. Run the applicable
local Rust baseline and the proof gate separately, recording their actual outcomes.
A complete transitive dependency license audit, signing, installer/update policy,
fresh-machine usability checks, migrations/recovery and non-Windows support remain
required work under `#49` before public distribution.
