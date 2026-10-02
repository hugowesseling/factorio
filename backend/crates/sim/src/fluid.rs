pub const FLUID_WATER: u8 = 0;
pub const TANK_CAPACITY: u32 = 1000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Fluid {
    pub kind: u8,
    pub amount: u32,
}

impl Fluid {
    pub fn new(kind: u8, amount: u32) -> Self {
        Self { kind, amount }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FluidTank {
    pub fluid: Option<Fluid>,
    pub capacity: u32,
}

impl FluidTank {
    pub fn new(capacity: u32) -> Self {
        Self {
            fluid: None,
            capacity,
        }
    }

    pub fn add(&mut self, fluid: Fluid) {
        match &mut self.fluid {
            None => self.fluid = Some(fluid),
            Some(current) if current.kind == fluid.kind => current.amount += fluid.amount,
            _ => {}
        }
    }

    pub fn take(&mut self, amount: u32) -> Option<Fluid> {
        let current = self.fluid?;
        let taken = current.amount.min(amount);
        if taken == 0 {
            return None;
        }
        self.fluid = if current.amount == taken {
            None
        } else {
            Some(Fluid {
                kind: current.kind,
                amount: current.amount - taken,
            })
        };
        Some(Fluid { kind: current.kind, amount: taken })
    }
}
