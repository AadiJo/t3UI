// Rasterizes the web UI's grain overlay (`body::after` in apps/web/src/index.css:609) to
// crates/t3-ui/assets/noise@2x.png: 256x256 CSS px at 2x, as Chromium draws it on Retina.
//
// Regenerate (offline works, resvg is already in the registry cache):
//   mkdir -p /tmp/noisegen/src && cp crates/t3-ui/tools/noise.rs /tmp/noisegen/src/main.rs
//   printf '[package]\nname="noisegen"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nresvg="=0.46.0"\n' > /tmp/noisegen/Cargo.toml
//   cargo run --release --manifest-path /tmp/noisegen/Cargo.toml -- crates/t3-ui/assets/noise@2x.png
fn main() {
    let svg = r#"<svg viewBox='0 0 256 256' width='256' height='256' xmlns='http://www.w3.org/2000/svg'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='4' stitchTiles='stitch'/></filter><rect width='100%' height='100%' filter='url(#n)'/></svg>"#;
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).unwrap();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(512, 512).unwrap();
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(2.0, 2.0), &mut pixmap.as_mut());
    pixmap.save_png(std::env::args().nth(1).expect("output path")).unwrap();
}
