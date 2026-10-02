use std::path::PathBuf;

use factorio_server::{GameServer, ServerConfig};

fn parse_u64(arg: Option<&String>, fallback: u64) -> u64 {
    arg.and_then(|value| value.parse::<u64>().ok()).unwrap_or(fallback)
}

fn parse_i32(arg: Option<&String>, fallback: i32) -> i32 {
    arg.and_then(|value| value.parse::<i32>().ok()).unwrap_or(fallback)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--serve") {
        serve_mode(&args);
        return;
    }

    let config = ServerConfig {
        seed: parse_u64(args.get(1), 1),
        view_radius: parse_i32(args.get(2), 2),
        autosave_every: 600,
    };
    let ticks = parse_u64(args.get(3), 1200);
    let mut server = GameServer::new(config.clone());
    println!(
        "factorio-server seed={} view_radius={} ticks={}",
        config.seed, config.view_radius, ticks
    );

    factorio_server::apply_demo_script(&mut server);

    let mut done = 0u64;
    while done < ticks {
        let outcome = server.advance();
        done += 1;
        if done % 600 == 0 {
            println!(
                "tick={} hash={:016x} power={}/{} chunks={} events={}",
                outcome.tick,
                outcome.state_hash,
                server.world.power_produced,
                server.world.power_consumed,
                server.streamer.resident(),
                outcome.events.len()
            );
        }
    }

    println!(
        "final tick={} hash={:016x} belts={} machines={} entities={}",
        server.world.tick,
        server.world.state_hash,
        server.world.belts.len(),
        server.world.machines.len(),
        server.world.occupancy.len()
    );

    let path = PathBuf::from("save.fsv");
    match server.save().write_to(&path) {
        Ok(()) => println!("wrote save to {}", path.display()),
        Err(error) => eprintln!("save failed: {error}"),
    }
}

fn serve_mode(args: &[String]) {
    use factorio_server::serve::{self, ServeOptions};

    let options = ServeOptions {
        addr: args.get(2).cloned().unwrap_or_else(|| serve::DEFAULT_ADDR.to_string()),
        config: ServerConfig {
            seed: parse_u64(args.get(3), 7),
            view_radius: parse_i32(args.get(4), 2),
            autosave_every: 600,
        },
        ready: true,
        ..ServeOptions::default()
    };

    match serve::serve(options) {
        Ok(report) => println!(
            "serve stopped addr={} ticks={} clients={} intents={}",
            report.addr, report.ticks, report.clients, report.intents
        ),
        Err(error) => {
            eprintln!("serve failed: {error}");
            std::process::exit(1);
        }
    }
}
