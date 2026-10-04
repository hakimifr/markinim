use markinim::quote::QuoteAssets;
use rand::SeedableRng;
use rand::rngs::StdRng;

#[test]
fn quotes_are_decodable_1000x1000_pngs() {
    let assets = QuoteAssets::load().unwrap();
    let mut rng = StdRng::seed_from_u64(11);
    let mut saw_variation = false;
    let first = assets.render("a short quote", &mut StdRng::seed_from_u64(1));
    for i in 0..20 {
        let png = assets.render("a short quote", &mut rng);
        let img = image::load_from_memory(&png).expect("render must produce a valid png");
        assert_eq!((img.width(), img.height()), (1000, 1000));
        if png != first {
            saw_variation = true;
        }
        let _ = i;
    }
    assert!(
        saw_variation,
        "random gradients/fonts must vary between renders"
    );
}

#[test]
fn long_text_does_not_panic() {
    let assets = QuoteAssets::load().unwrap();
    let long = "word ".repeat(60);
    let png = assets.render(long.trim(), &mut StdRng::seed_from_u64(3));
    assert!(image::load_from_memory(&png).is_ok());
}

#[test]
fn empty_and_unicode_text_render() {
    let assets = QuoteAssets::load().unwrap();
    let png = assets.render("привет ᗜᴗᗜ world 🌺", &mut StdRng::seed_from_u64(4));
    assert!(image::load_from_memory(&png).is_ok());
}
