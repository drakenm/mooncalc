//! Renders a synthetic Yallop-style zone pattern in every projection and
//! sampling mode, writes PNG and SVG files to `target/render_demo/`, and
//! prints render timings.
//!
//! ```sh
//! cargo run --release -p mooncalc-maps --example render_demo
//! ```

use std::path::PathBuf;
use std::time::Instant;

use mooncalc_maps::basemap::{BaseMap, Scale};
use mooncalc_maps::grid::VisibilityGrid;
use mooncalc_maps::projection::{Equirectangular, MapProjection, Orthographic, Robinson};
use mooncalc_maps::render::{
    Legend, LegendPosition, MapData, Marker, RenderOptions, ZoneField, ZoneSmoothing, encode_png,
    render_raster, render_svg,
};

const YALLOP_Q: [f32; 5] = [0.216, -0.014, -0.160, -0.232, -0.293];
const PALETTE: [[u8; 3]; 6] = [
    [0, 160, 60],
    [120, 200, 70],
    [245, 215, 40],
    [245, 140, 30],
    [215, 50, 50],
    [215, 215, 222],
];
const LABELS: [&str; 6] = [
    "A: easily visible",
    "B: visible in perfect conditions",
    "C: optical aid may be needed",
    "D: optical aid needed",
    "E: not visible with a telescope",
    "F: not visible",
];

/// A parabola-shaped q field opening westwards, like a real crescent
/// visibility map, with no evaluation north of 62° N.
fn synthetic(rows: usize, cols: usize) -> (VisibilityGrid, Vec<f32>) {
    let mut g = VisibilityGrid::new(-90.0, 90.0, -180.0, 180.0, rows, cols);
    let mut q = vec![f32::NAN; rows * cols];
    let field = ZoneField {
        values: &[],
        thresholds: &YALLOP_Q,
    };
    for r in 0..rows {
        for c in 0..cols {
            let (lat, lon) = g.cell_center(r, c);
            if lat > 62.0 {
                continue;
            }
            let v = (-(lon - 20.0) / 250.0 - (lat + 5.0).powi(2) / 6000.0) as f32;
            q[r * cols + c] = v;
            g.set(r, c, Some(field.classify(v)));
        }
    }
    (g, q)
}

fn out_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/render_demo");
    std::fs::create_dir_all(&dir).expect("create output dir");
    dir
}

fn time_ms(n: usize, mut f: impl FnMut()) -> f64 {
    let mut v: Vec<f64> = (0..n)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let dir = out_dir();
    let t = Instant::now();
    let base = BaseMap::embedded(Scale::Ne50m);
    println!(
        "decode 50m base map: {:.1} ms",
        t.elapsed().as_secs_f64() * 1e3
    );
    let t = Instant::now();
    base.land_mask();
    println!(
        "build 50m land mask: {:.1} ms",
        t.elapsed().as_secs_f64() * 1e3
    );

    let (grid, q) = synthetic(180, 360);
    let field = ZoneField {
        values: &q,
        thresholds: &YALLOP_Q,
    };
    let with_field = MapData {
        grid: &grid,
        palette: &PALETTE,
        none_color: Some([150, 150, 160]),
        field: Some(field),
    };
    let cells_only = MapData {
        field: None,
        ..with_field
    };
    let legend = Legend {
        position: LegendPosition::BottomLeft,
        ..Legend::from_palette(Some("Yallop (synthetic)"), &PALETTE, &LABELS)
    };
    let markers = vec![
        Marker::new(21.42, 39.83, "Makkah"),
        Marker::new(51.51, -0.13, "London"),
        Marker::new(-6.2, 106.85, "Jakarta"),
        Marker::new(40.71, -74.0, "New York"),
    ];
    let base_opts = RenderOptions {
        markers,
        legend: Some(legend),
        ..RenderOptions::default()
    };

    let views: [(&str, MapProjection, u32); 4] = [
        ("equirect", Equirectangular::world(0.0).into(), 1440),
        ("equirect_150e", Equirectangular::world(150.0).into(), 1440),
        ("robinson", Robinson::new(0.0).into(), 1440),
        ("globe", Orthographic::new(20.0, -30.0).into(), 900),
    ];
    for (name, proj, width) in views {
        let opts = RenderOptions {
            projection: proj,
            width,
            ..base_opts.clone()
        }
        .fit_height();
        let img = render_raster(&with_field, &opts);
        std::fs::write(dir.join(format!("{name}.png")), encode_png(&img).unwrap()).unwrap();
        let svg = render_svg(&cells_only, &opts);
        std::fs::write(dir.join(format!("{name}.svg")), &svg).unwrap();
        let ms = time_ms(9, || {
            render_raster(&with_field, &opts);
        });
        let ms_svg = time_ms(5, || {
            render_svg(&cells_only, &opts);
        });
        println!(
            "{name:14} {}x{}: raster {ms:6.1} ms, svg {ms_svg:6.1} ms ({} KiB)",
            opts.width,
            opts.height,
            svg.len() / 1024
        );
    }

    // Sampling modes side by side over Europe/Africa on a coarse 5° grid.
    let (coarse, cq) = synthetic(36, 72);
    let region = Equirectangular {
        lat_min_deg: -20.0,
        lat_max_deg: 60.0,
        lon_min_deg: -60.0,
        lon_max_deg: 60.0,
    };
    for (name, field, smoothing) in [
        ("mode_nearest", None, ZoneSmoothing::Nearest),
        ("mode_smooth", None, ZoneSmoothing::Smooth),
        (
            "mode_field",
            Some(ZoneField {
                values: &cq,
                thresholds: &YALLOP_Q,
            }),
            ZoneSmoothing::Smooth,
        ),
    ] {
        let data = MapData {
            grid: &coarse,
            palette: &PALETTE,
            none_color: Some([150, 150, 160]),
            field,
        };
        let opts = RenderOptions {
            projection: region.into(),
            width: 600,
            smoothing,
            legend: None,
            markers: Vec::new(),
            graticule: Some(mooncalc_maps::render::Graticule {
                spacing_deg: 5.0,
                ..base_opts.graticule.unwrap()
            }),
            ..base_opts.clone()
        }
        .fit_height();
        let img = render_raster(&data, &opts);
        std::fs::write(dir.join(format!("{name}.png")), encode_png(&img).unwrap()).unwrap();
    }

    // Timing breakdown for the headline case.
    let opts = RenderOptions {
        legend: None,
        ..base_opts.clone()
    };
    let bare = RenderOptions {
        coastlines: None,
        borders: None,
        graticule: None,
        frame: None,
        markers: vec![],
        ..opts.clone()
    };
    let small = RenderOptions {
        basemap: Scale::Ne110m,
        ..opts.clone()
    };
    let nofield = time_ms(9, || {
        render_raster(&cells_only, &opts);
    });
    let surface = time_ms(9, || {
        render_raster(&with_field, &bare);
    });
    let s110 = time_ms(9, || {
        render_raster(&with_field, &small);
    });
    let img = render_raster(&with_field, &opts);
    let png_ms = time_ms(5, || {
        encode_png(&img).unwrap();
    });
    println!(
        "equirect 1440x720 breakdown: cells-only {nofield:.1} ms, surface only {surface:.1} ms, 110m {s110:.1} ms, PNG encode {png_ms:.1} ms"
    );
    println!("wrote {}", dir.display());
}
