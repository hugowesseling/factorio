#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    Move { player: u32, dx: i32, dy: i32 },
    PlaceBelt { player: u32, x: i32, y: i32, dir: u8 },
    PlaceInserter { player: u32, x: i32, y: i32, dir: u8 },
    PlaceMachine { player: u32, x: i32, y: i32, kind: u8 },
    RemoveEntity { player: u32, x: i32, y: i32 },
    MineTile { player: u32, x: i32, y: i32 },
    CollectMachine { player: u32, x: i32, y: i32 },
    ResearchTech { player: u32, tech: u16 },
}
