//! Joins the first game found on the network and plays along without the game: it
//! says it has the map (or downloads it), greets the others, sends no actions, and
//! answers each turn with checksum 0, which the original's host finds out of step.
//! Prints what it hears.
//!
//! join <name> [seconds]

use std::time::{Duration, Instant};

use w3_net::Chat;
use w3_net::client::{Browser, Client, Event};
use w3_net::host::{TFT, VERSION};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let a: Vec<String> = std::env::args().collect();
    let secs: u64 = a.get(2).map_or(Ok(60), |s| s.parse())?;
    let mut browser = Browser::new(6112, TFT, VERSION)?;
    let end = Instant::now() + Duration::from_secs(secs);
    let (from, game) = loop {
        browser.poll();
        if let Some((from, g, _)) = browser.games.first() {
            break (*from, g.clone());
        }
        if Instant::now() > end {
            return Err("no game found".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    println!("joining {:?} at {from} (map {})", game.name, game.stat.map_path);
    let mut client = Client::join(from, &game, &a[1], 6112)?;
    let mut greeted = false;
    let mut turned = false;
    while Instant::now() < end && client.alive() {
        for e in client.poll() {
            match &e {
                Event::Turn { actions, interval } => {
                    if !actions.is_empty() || !std::mem::replace(&mut turned, true) {
                        println!("turn of {interval} ms");
                        println!("{e:?}");
                    }
                    client.keepalive(0);
                }
                Event::MapDownloaded(map) => println!("MapDownloaded({} bytes)", map.len()),
                _ => println!("{e:?}"),
            }
            match e {
                Event::MapCheck { .. } => client.map_status(std::env::var("NOMAP").is_err()),
                Event::Slots if !greeted && client.slots.slots.iter().any(|s| s.pid == client.pid && s.download == 100) => {
                    greeted = true;
                    client.chat(Chat::Lobby("hello from w3-net".into()));
                }
                Event::CountdownEnd => client.loaded(),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
