# Continuous integration

GitHub Actions runs the required checks from [AGENTS.md](../AGENTS.md) in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml). A second workflow, [`.github/workflows/de440.yml`](../.github/workflows/de440.yml), runs the DE440 kernel tests on a schedule. Neither uses secrets. Both run with a read-only `GITHUB_TOKEN` (`permissions: contents: read`).

There is no release or deployment workflow yet. The staging and production steps in the AGENTS.md release flow are still manual.

## When CI runs

- Pushes to `master`, `feature/**` and `release/v*`.
- Pull requests into those branches.
- Manually, from the Actions tab (`workflow_dispatch`).

A newer run for the same ref cancels the one in progress.

## Jobs

| Job | Runner | What it runs |
| --- | --- | --- |
| `fmt` | ubuntu-latest | `cargo fmt --all -- --check` |
| `clippy` | ubuntu-latest | `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo clippy -p mooncalc-gui -p mooncalc-app -p mooncalc-tauri --all-targets --features de440 -- -D warnings` |
| `test (ubuntu-latest)`, `test (windows-latest)` | both | `cargo test --workspace` |
| `wasm` | ubuntu-latest | `cargo build --target wasm32-unknown-unknown -p mooncalc-core -p mooncalc-visibility -p mooncalc-maps -p mooncalc-hijri -p mooncalc-atlas -p mooncalc-app` (the [portability rule](architecture.md)) |
| `frontend` | ubuntu-latest, Node 24 | `npm ci`, `npm run build`, `npm run check` in `frontend/tauri/` |
| `ci-required` | ubuntu-latest | Needs every job above. Fails unless all of them succeeded, including when one failed, was cancelled or was skipped. |

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

## Pinned actions

Every third-party action is pinned to a full commit SHA, with the release in a comment. Each release was at least 14 days old when adopted (the dependency rule in AGENTS.md).

| Action | Release | Commit |
| --- | --- | --- |
| `actions/checkout` | v7.0.1 | `3d3c42e5aac5ba805825da76410c181273ba90b1` |
| `actions/setup-node` | v7.1.0 | `949feb2413d6458794dcd2491c4babbbce0c15c1` |
| `actions/cache` | v6.1.0 | `55cc8345863c7cc4c66a329aec7e433d2d1c52a9` |
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
