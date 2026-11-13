<img src="assets/icon.svg" width="64" height="64" alt="">

# MoonCalc

A desktop app for lunar crescent visibility, written in Rust. It draws world visibility maps under Yallop, Odeh and other criteria, and also computes ephemerides, rise and set times, phases, eclipses, Hijri calendars, sky charts and libration.

## Quickstart

### Download

Get the egui app (`.zip` / `.tar.gz`) or the Tauri installer for Windows or Linux from [Releases](https://github.com/drakenm/mooncalc/releases).

### Build from source

You need Rust 1.90 or newer.

```sh
git clone https://github.com/drakenm/mooncalc.git
cd mooncalc
cargo run --release          # egui app
```

To use JPL DE440 instead of the built-in Meeus/ELP theory, build with `--features de440`. Then download [`de440s.bsp`](https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/planets/) and choose it in the app:

```sh
cargo run --release --features de440
```

### Tauri app (optional)

You also need Node.js.

```sh
cd frontend/tauri
npm ci
npm run tauri -- dev
```

### Test

```sh
cargo test --workspace
```

## More

- [Architecture](docs/architecture.md)
- [Validation](docs/validation.md)
- [CI and releases](docs/ci.md)

## License

[MIT](LICENSE)
