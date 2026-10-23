//! Warcraft III: Reign of Chaos and The Frozen Throne.

use clap::Parser;
use war3::cli::{Args, normalize_args};

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn")).init();
    let args = Args::parse_from(normalize_args(std::env::args_os()));
    war3::app::run(args)
}
