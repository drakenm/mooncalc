# Continuous integration

GitHub Actions runs the required checks from [AGENTS.md](../AGENTS.md) in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml). A second workflow, [`.github/workflows/de440.yml`](../.github/workflows/de440.yml), runs the DE440 kernel tests on a schedule. A third, [`.github/workflows/release.yml`](../.github/workflows/release.yml), builds the desktop apps and publishes them on version tags (see [Release builds](#release-builds)). None uses secrets. All run with a read-only `GITHUB_TOKEN` (`permissions: contents: read`), except the release job that creates the GitHub release.

MoonCalc is a desktop application, so nothing is deployed. In the AGENTS.md release flow, the `-rcN` pre-release takes the place of staging: its downloads are what gets tested, and the final `vX.Y.Z` tag goes on the same commit.

## When CI runs

- Pushes to `master`, `feature/**` and `release/v*`.
- Pull requests into those branches (see the rules below).
- Manually, from the Actions tab (`workflow_dispatch`).

A `changes` job gates the checks. For pull requests it first applies the branch policy:

| Pull request | Result |
| --- | --- |
| `release/vX.Y.Z` into `master` | Allowed when `X.Y.Z` equals the workspace version in `Cargo.toml` at the PR head. CI runs. |
| `hotfix/<issue>-<name>` into `master` | Allowed when `<issue>` is an existing GitHub issue (not a pull request) in this repository. CI runs. |
| `chore/*` into `master` | Allowed. CI runs. |
| Any other branch into `master`, or a branch from a fork | `changes` fails, so `ci-required` fails and the PR can't merge. |
| `release/v*` into any other branch | CI runs. |
| Anything else into any other branch | CI is skipped. `ci-required` reports skipped, which branch protection accepts as passing. |

It then diffs the push (`before..after`) or the pull request (`base...head`). When every changed file is Markdown (`*.md`) or under `docs/`, the checks are skipped and `ci-required` passes. `LICENSE` counts as code because the GUI embeds it. Manual runs, new branches and force pushes whose old head is gone run every check.

The policy only blocks merges when branch protection on `master` requires `ci-required`. The `changes` job reads issues with the workflow's own `GITHUB_TOKEN` (`issues: read`).

A newer run for the same ref cancels the one in progress.

## Jobs

| Job | Runner | What it runs |
| --- | --- | --- |
| `fmt` | ubuntu-latest | `cargo fmt --all -- --check` |
| `clippy` | ubuntu-latest | `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo clippy -p mooncalc-gui -p mooncalc-app -p mooncalc-tauri --all-targets --features de440 -- -D warnings` |
| `test (ubuntu-latest)`, `test (windows-latest)` | both | `cargo test --workspace` |
| `wasm` | ubuntu-latest | `cargo build --target wasm32-unknown-unknown -p mooncalc-core -p mooncalc-visibility -p mooncalc-maps -p mooncalc-hijri -p mooncalc-atlas -p mooncalc-app` (the [portability rule](architecture.md)) |
| `frontend` | ubuntu-latest, Node 24 | `npm ci`, `npm run build`, `npm run check` in `frontend/tauri/` |
| `changes` | ubuntu-latest | Applies the pull request branch policy and decides whether the change touches code (see [When CI runs](#when-ci-runs)). |
| `ci-required` | ubuntu-latest | Needs every job above. Fails unless all of them succeeded, including when one failed, was cancelled or was skipped. The exception is a documentation-only change, where the checks are skipped on purpose and it passes. |

Branch protection should require only the `ci-required` check. It always reports a result, so the rule doesn't change when jobs are added. Add new jobs to its `needs` list.

### Toolchain and caching

`rust-toolchain.toml` pins the `stable` channel, and each job installs it with `rustup toolchain install stable --profile minimal` plus the components or targets it needs (rustfmt, clippy, `wasm32-unknown-unknown`). No third-party toolchain action is involved. CI therefore uses the latest stable Rust, like a local `rustup update`.

`Swatinem/rust-cache` caches `~/.cargo` and `target/` per job and OS. Caches are saved only from pushes, not from pull requests. Pull requests restore the cache of their base branch, so their own builds don't use up the repository's 10 GB cache quota. `actions/setup-node` caches the npm download cache, keyed on `frontend/tauri/package-lock.json`.

### Linux system packages

The Linux `clippy` and `test` jobs install the libraries that crates in `Cargo.lock` link through pkg-config:

| Package | Needed by |
| --- | --- |
| `libwebkit2gtk-4.1-dev` | `webkit2gtk-sys`, `soup3-sys`, `javascriptcore-rs-sys` (Tauri's webview). It depends on the GTK 3, GLib, Cairo, Pango, ATK and GdkPixbuf headers. |
| `libgtk-3-dev` | `gtk-sys`, `gdk-sys` (tao, wry, muda) |
| `libdbus-1-dev` | `libdbus-sys` (tao) |
| `libx11-dev` | `x11` (`gdkx11-sys`) |
| `libfontconfig-dev` | `yeslogic-fontconfig-sys` (egui system fonts) |

The list stays short on purpose. eframe's winit and glutin stack loads X11, Wayland, xkbcommon and EGL with `dlopen`, and `libappindicator-sys` loads its library at run time, so they need no build headers. No crate links OpenSSL or librsvg. `libssl-dev`, `librsvg2-dev` and `libayatana-appindicator3-dev` appear in Tauri's generic setup guide but aren't needed to build or test this workspace. Running or bundling the apps on Linux can still need the runtime libraries. When a dependency update adds a `-sys` crate, extend the `LINUX_BUILD_DEPS` list at the top of `ci.yml`.

### DE440 kernel tests

The tests in `crates/mooncalc-spk/tests/de440*.rs` need a JPL kernel. Without `MOONCALC_DE440` they print a notice and pass, so `ci.yml` builds them but doesn't exercise them against real data.

`de440.yml` runs them nightly (04:17 UTC) and on manual dispatch. It downloads `de440s.bsp` (32,726,016 bytes) from [NAIF](https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/planets/), checks its SHA-256 against the value recorded in the workflow, and caches it. It then runs `cargo test -p mooncalc-spk` with `MOONCALC_DE440` set, and fails if any test still prints the skip notice. The job is not part of `ci-required`, so a NAIF outage can't block merges. GitHub runs scheduled workflows only from the default branch, so the nightly run starts once this file is on that branch. Until then, start it manually.

## Release builds

`release.yml` builds both desktop apps, in a standard and a DE440 variant, for two platforms:

| Job | Runner | Output |
| --- | --- | --- |
| `version` | ubuntu-latest | The version label used in file names. On a tag, checks that the tag is `vX.Y.Z`, `vX.Y.Z-rcN` or `vX.Y.Z-hfN` and that `X.Y.Z` equals the workspace version in `Cargo.toml`. On a branch, the label is `X.Y.Z-dev.<short SHA>`. |
| `build (linux, standard\|de440)` | ubuntu-22.04 | egui `.tar.gz`, Tauri `.deb` and `.AppImage` |
| `build (windows, standard\|de440)` | windows-latest | egui `.zip`, Tauri NSIS `-setup.exe` and `.msi` |
| `publish` | ubuntu-latest | Tags only. Downloads the files from the four build jobs, writes `SHA256SUMS.txt` and creates the GitHub release with `gh release create`. `-rcN` tags become pre-releases; other tags are marked latest. |

### When it runs

- Tags `v*`: build and publish.
- Pushes to `release/v*` and manual runs: build only. The files stay on the run as workflow artifacts for 14 days, so a release branch can be tried before it is tagged.

Linux builds use Ubuntu 22.04 (glibc 2.35), so the binaries also run on distributions older than the newest runner image.

### Variants and file names

Each build job runs `cargo build --release --locked -p mooncalc-gui`, then the Tauri CLI (`npm run tauri -- build --ci --bundles ...`), then [`tools/release/package.sh`](../tools/release/package.sh), which names the files:

| File | Contents |
| --- | --- |
| `mooncalc-egui-<label>-<os>-x86_64.(zip\|tar.gz)` | egui app, `LICENSE.txt`, `README.txt` |
| `mooncalc-tauri-<label>-windows-x86_64-setup.exe`, `.msi` | Tauri app installers |
| `mooncalc-tauri-<label>-linux-x86_64.deb`, `.AppImage` | Tauri app packages |

The DE440 variant adds `-de440` after `egui` or `tauri`. It builds with `--features de440`. Its Tauri bundle also takes [`crates/mooncalc-tauri/tauri.de440.conf.json`](../crates/mooncalc-tauri/tauri.de440.conf.json), which changes the product name, binary name and identifier, so both Tauri variants can be installed side by side. No JPL kernel is bundled: users download `de440s.bsp` from NAIF and pick it in the app.

The Windows files are not code-signed. No secrets are involved.

### Cutting a release

1. Set `version` in `[workspace.package]` of `Cargo.toml` (and `frontend/tauri/package.json`) on the release branch.
2. Merge the release branch into `master` and wait for `ci-required`.
3. Tag the `master` merge commit and push the tag, e.g. `git tag -a v0.1.1-rc1 -m "MoonCalc 0.1.1-rc1"` then `git push origin v0.1.1-rc1`.

A tag that doesn't match the workspace version fails the `version` job before anything is built.

To reproduce one build locally (Windows, DE440 variant, from Git Bash):

```sh
(cd frontend/tauri && npm ci)
cargo build --release --locked -p mooncalc-gui --features de440
(cd frontend/tauri && npm run tauri -- build --ci --bundles nsis,msi --features de440 --config "$(pwd -W)/../../crates/mooncalc-tauri/tauri.de440.conf.json" -- --locked)
tools/release/package.sh windows de440 0.1.1-test dist
```

## Pinned actions

Every third-party action is pinned to a full commit SHA, with the release in a comment. Each release was at least 14 days old when adopted (the dependency rule in AGENTS.md).

| Action | Release | Commit |
| --- | --- | --- |
| `actions/checkout` | v7.0.1 | `3d3c42e5aac5ba805825da76410c181273ba90b1` |
| `actions/setup-node` | v7.1.0 | `949feb2413d6458794dcd2491c4babbbce0c15c1` |
| `actions/cache` | v6.1.0 | `55cc8345863c7cc4c66a329aec7e433d2d1c52a9` |
| `actions/upload-artifact` | v7.0.2 | `cf430e030ddbb5b0abf93d22962f4752f3646cd9` |
| `actions/download-artifact` | v8.0.2 | `9000827ccba6bdab643e8b6fd33ac0654aef8333` |
| `Swatinem/rust-cache` | v2.9.3 | `d1527c8f67ef9af357290475d4af4b0fab8f12e0` |

To update a pin, choose a release at least 14 days old and resolve its tag to a commit. An annotated tag needs a second lookup:

```sh
curl -s https://api.github.com/repos/actions/checkout/git/refs/tags/v7.0.1
# if "object.type" is "tag":
curl -s https://api.github.com/repos/OWNER/REPO/git/tags/<object.sha>
```

Then lint the workflows with [actionlint](https://github.com/rhysd/actionlint) and shellcheck:

```sh
actionlint .github/workflows/*.yml
```

## Reproducing CI locally

Run from the repository root. On Windows, put the toolchain on `PATH` first (`export PATH="$USERPROFILE/.cargo/bin:$PATH"` in Git Bash).

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p mooncalc-gui -p mooncalc-app -p mooncalc-tauri --all-targets --features de440 -- -D warnings
cargo test --workspace
rustup target add wasm32-unknown-unknown   # once
cargo build --target wasm32-unknown-unknown -p mooncalc-core -p mooncalc-visibility -p mooncalc-maps -p mooncalc-hijri -p mooncalc-atlas -p mooncalc-app
(cd frontend/tauri && npm ci && npm run build && npm run check)
```

On Debian or Ubuntu, install the packages first:

```sh
sudo apt-get install --no-install-recommends libwebkit2gtk-4.1-dev libgtk-3-dev libdbus-1-dev libx11-dev libfontconfig-dev
```

To run the DE440 tests, download `de440s.bsp` and point the variable at it:

```sh
MOONCALC_DE440=/path/to/de440s.bsp cargo test -p mooncalc-spk -- --nocapture
```

The `cargo` commands work without building the frontend first. `crates/mooncalc-tauri/build.rs` writes a placeholder `frontend/tauri/dist/index.html` when none exists.
