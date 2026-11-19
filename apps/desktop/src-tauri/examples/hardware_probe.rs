//! Prints what Joona detects about this PC and the model it would suggest
//! for a few kinds of speaker.
//!
//! cargo run --example hardware_probe

use joona_lib::models::catalog::catalog;
use joona_lib::models::hardware;
use joona_lib::models::recommend::{self, Need};

fn main() {
    let started = std::time::Instant::now();
    let hw = hardware::detect();
    println!("detected in {} ms: {hw:#?}", started.elapsed().as_millis());
    for (languages, translate) in [
        (&["en"][..], false),
        (&["hi", "en"][..], false),
        (&["hi", "en"][..], true),
        (&["ta"][..], true),
        (&["de", "fr", "en"][..], false),
    ] {
        let need = Need {
            languages: languages.iter().map(|l| (*l).to_owned()).collect(),
            translate,
        };
        let pick = recommend::recommend(catalog(), &hw, &need).map(|m| m.id.as_str());
        let fits: Vec<String> = catalog()
            .models
            .iter()
            .map(|m| format!("{}={:?}", m.id, recommend::fit(m, &hw, &need)))
            .collect();
        println!(
            "{languages:?} translate={translate}: {pick:?}\n    {}",
            fits.join(", ")
        );
    }
}
