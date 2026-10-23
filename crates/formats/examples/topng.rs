//! `topng <archive> <texture> <out.png>`: decodes a texture's top mip to PNG.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = w3_mpq::Archive::open(&a[1]).expect("open");
    let data = ar.read(&a[2]).expect("read");
    let img = &w3_formats::blp::decode_texture(&a[2], &data).expect("decode")[0];
    image::save_buffer(&a[3], &img.rgba, img.width, img.height, image::ExtendedColorType::Rgba8).unwrap();
}
