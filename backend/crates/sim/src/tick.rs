#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    InputDrain,
    Movement,
    Belts,
    Inserters,
    Machines,
    Fluids,
    Power,
    Combat,
    TickEvents,
}

pub const PHASES: [Phase; 9] = [
    Phase::InputDrain,
    Phase::Movement,
    Phase::Belts,
    Phase::Inserters,
    Phase::Machines,
    Phase::Fluids,
    Phase::Power,
    Phase::Combat,
    Phase::TickEvents,
];

impl Phase {
    pub fn index(self) -> u8 {
        match self {
            Phase::InputDrain => 0,
            Phase::Movement => 1,
            Phase::Belts => 2,
            Phase::Inserters => 3,
            Phase::Machines => 4,
            Phase::Fluids => 5,
            Phase::Power => 6,
            Phase::Combat => 7,
            Phase::TickEvents => 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_order_is_fixed() {
        assert_eq!(PHASES[0], Phase::InputDrain);
        assert_eq!(PHASES[6], Phase::Power);
        assert_eq!(PHASES.len(), 9);
        for (i, phase) in PHASES.iter().enumerate() {
            assert_eq!(phase.index() as usize, i);
        }
    }
}