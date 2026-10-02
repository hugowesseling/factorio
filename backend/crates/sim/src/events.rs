use crate::grid::EntityKind;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    PlayerMoved { player: u32, x: i32, y: i32 },
    EntityPlaced { kind: EntityKind, index: u32, x: i32, y: i32 },
    EntityRemoved { kind: EntityKind, index: u32 },
    TileMined { x: i32, y: i32, item: u16, count: u16 },
    MachineProduced { machine: u32, item: u16, count: u16 },
    ResearchProgress { tech: u16, progress: u32 },
    ResearchCompleted { tech: u16 },
    WaterExtracted { machine: u32 },
    TickCompleted { tick: u64 },
}
