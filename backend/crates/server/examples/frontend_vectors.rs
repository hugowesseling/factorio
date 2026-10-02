use factorio_content::Content;
use factorio_proto::{
    frame, ClientMessage, Intent, IntentKind, ServerMessage, PROTOCOL_VERSION,
};
use factorio_sim::{generate_chunk, ChunkPos, Pcg32};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    if mode == "content" {
        println!("{}", content_json(0));
        return;
    }
    let mut out = String::new();
    out.push_str("{\n");

    out.push_str("  \"pcg32\": {\n");
    let seeds = [0u64, 1, 7, 1234];
    for (i, seed) in seeds.iter().enumerate() {
        let mut rng = Pcg32::new(*seed);
        let values: Vec<String> = (0..8)
            .map(|_| format!("\"{:08x}\"", rng.next_u32()))
            .collect();
        out.push_str(&format!(
            "    \"{}\": [{}]{}\n",
            seed,
            values.join(", "),
            if i + 1 == seeds.len() { "" } else { "," }
        ));
    }
    out.push_str("  },\n");

    out.push_str("  \"chunks\": {\n");
    let cases: [(u64, i32, i32); 4] = [(7, 0, 0), (7, 1, -2), (3, -1, -1), (1, 40, 40)];
    for (index, (seed, cx, cy)) in cases.iter().enumerate() {
        let chunk = ChunkPos { x: *cx, y: *cy };
        let data = generate_chunk(*seed, chunk);
        let tiles: Vec<String> = (0..24)
            .map(|i| {
                format!(
                    "[{},{},{}]",
                    data.resource[i], data.ore_amount[i], data.flags[i]
                )
            })
            .collect();
        out.push_str(&format!(
            "    \"s{}:{}:{}\": [{}]{}",
            seed,
            cx,
            cy,
            tiles.join(","),
            if index + 1 == cases.len() { "\n" } else { ",\n" }
        ));
    }
    out.push_str("  },\n");

    out.push_str("  \"protocol\": {\n");
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    entries.push((
        "hello".to_string(),
        ClientMessage::Hello { name: "hugo".to_string(), version: PROTOCOL_VERSION }.encode(),
    ));
    entries.push((
        "intent_move".to_string(),
        ClientMessage::Intent { player: 3, intent: Intent::new(IntentKind::Move, 1, -1, 0) }
            .encode(),
    ));
    entries.push((
        "intent_place_machine".to_string(),
        ClientMessage::Intent {
            player: 0,
            intent: Intent::new(IntentKind::PlaceMachine, -412, 88, 2),
        }
        .encode(),
    ));
    entries.push((
        "ping".to_string(),
        ClientMessage::Ping { stamp: 987654321 }.encode(),
    ));
    entries.push((
        "server_hello".to_string(),
        ServerMessage::Hello { version: PROTOCOL_VERSION, seed: 42 }.encode(),
    ));
    entries.push((
        "server_snapshot".to_string(),
        ServerMessage::Snapshot { tick: 600, hash: 0xdead_beef }.encode(),
    ));
    entries.push((
        "server_event".to_string(),
        ServerMessage::Event { tick: 7, code: 3, a: -1, b: 0, c: 9 }.encode(),
    ));
    entries.push((
        "server_pong".to_string(),
        ServerMessage::Pong { stamp: 12 }.encode(),
    ));
    for (index, (name, bytes)) in entries.iter().enumerate() {
        out.push_str(&format!(
            "    \"{}\": \"{}\"{}\n",
            name,
            hex(bytes),
            if index + 1 == entries.len() { "\n" } else { "," }
        ));
    }
    out.push_str("  },\n");

    let ping = ClientMessage::Ping { stamp: 5 };
    let framed = frame(&ping.encode());
    out.push_str(&format!("  \"framedPing\": \"{}\"\n", hex(&framed)));

    let a = ClientMessage::Intent { player: 1, intent: Intent::new(IntentKind::PlaceBelt, 4, 4, 0) };
    let b = ClientMessage::Ping { stamp: 9 };
    let mut data = frame(&a.encode());
    data.extend(frame(&b.encode()));
    out.push_str(&format!(",\n  \"splitFrames\": \"{}\"\n", hex(&data)));

    out.push_str(",\n  \"content\": ");
    out.push_str(&content_json(2));
    out.push_str("\n}\n");
    print!("{out}");
}

fn content_json(indent: usize) -> String {
    let pad = " ".repeat(indent);
    let pad2 = " ".repeat(indent + 2);
    let content = Content::load();
    let mut out = String::new();
    out.push_str("{\n");
    let entries: Vec<String> = vec![
        content
            .items
            .iter()
            .map(|item| {
                format!(
                    "[{}, \"{}\", {}, \"{}\"]",
                    item_index(&content, &item.id),
                    item.id,
                    item.max_stack,
                    item.name
                )
            })
            .collect::<Vec<String>>()
            .join(", "),
        content
            .machines
            .iter()
            .enumerate()
            .map(|(index, machine)| {
                format!(
                    "[{}, \"{}\", {}, {}, {}]",
                    index, machine.id, machine.size.0, machine.size.1, machine.max_power
                )
            })
            .collect::<Vec<String>>()
            .join(", "),
        content
            .belts
            .iter()
            .enumerate()
            .map(|(index, belt)| format!("[{}, \"{}\", {}]", index, belt.id, belt.speed_ticks))
            .collect::<Vec<String>>()
            .join(", "),
        content
            .inserters
            .iter()
            .enumerate()
            .map(|(index, inserter)| {
                format!("[{}, \"{}\", {}]", index, inserter.id, inserter.swing_ticks)
            })
            .collect::<Vec<String>>()
            .join(", "),
        content
            .recipes
            .iter()
            .enumerate()
            .map(|(index, recipe)| format!("[{}, \"{}\"]", index, recipe.id))
            .collect::<Vec<String>>()
            .join(", "),
        content
            .technologies
            .iter()
            .enumerate()
            .map(|(index, tech)| format!("[{}, \"{}\"]", index, tech.id))
            .collect::<Vec<String>>()
            .join(", "),
        content
            .resource_items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<String>>()
            .join(", "),
    ];
    let names = [
        "items",
        "machines",
        "belts",
        "inserters",
        "recipes",
        "technologies",
        "resourceItems",
    ];
    for (index, value) in entries.iter().enumerate() {
        out.push_str(&format!(
            "{pad2}\"{}\": [{}]{}\n",
            names[index],
            value,
            if index + 1 == entries.len() { "" } else { "," }
        ));
    }
    out.push_str(&format!("{pad}}}"));
    out
}

fn item_index(content: &Content, id: &str) -> u16 {
    content.item_index.get(id).copied().unwrap_or(u16::MAX)
}