//! `blp2png <in.blp> <out.png>`: decodes a BLP texture's first mip to a PNG, to look at.

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read");
    let images = w3_formats::blp::decode_texture(&a[1], &data).expect("decode");
    let img = &images[0];
    image::save_buffer(&a[2], &img.rgba, img.width, img.height, image::ExtendedColorType::Rgba8).expect("save");
}
