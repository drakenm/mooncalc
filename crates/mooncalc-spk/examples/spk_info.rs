//! Print the file record and segment table of an SPK file.
//!
//! `cargo run -p mooncalc-spk --example spk_info -- path/to/de440s.bsp`

use mooncalc_spk::{Spk, time};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: spk_info <file.bsp>")?;
    let spk = Spk::open(&path)?;
    let fr = spk.file_record();
    println!(
        "{} {:?} ND={} NI={} {:?}",
        fr.id_word, fr.internal_name, fr.nd, fr.ni, fr.byte_order
    );
    println!("target center frame type      start JD TDB        end JD TDB   name");
    for s in spk.segments() {
        println!(
            "{:>6} {:>6} {:>5} {:>4} {:>17.6} {:>17.6}   {}",
            s.target,
            s.center,
            s.frame,
            s.data_type,
            time::et_to_jd_tdb(s.start_et),
            time::et_to_jd_tdb(s.end_et),
            s.name
        );
    }
    Ok(())
}
