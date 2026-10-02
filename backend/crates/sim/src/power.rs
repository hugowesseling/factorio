pub const POWER_RATIO_FULL: u32 = 10_000;

pub const COMPONENT_POWER_PRODUCER: &str = "power_producer";
pub const COMPONENT_POWER_CONSUMER: &str = "power_consumer";

fn has_component(content: &factorio_content::Content, kind: u8, component: &str) -> bool {
    content
        .machine(kind)
        .map(|def| def.components.iter().any(|entry| entry == component))
        .unwrap_or(false)
}

pub fn machine_is_producer(content: &factorio_content::Content, kind: u8) -> bool {
    has_component(content, kind, COMPONENT_POWER_PRODUCER)
}

pub fn machine_is_consumer(content: &factorio_content::Content, kind: u8) -> bool {
    has_component(content, kind, COMPONENT_POWER_CONSUMER)
}

pub fn machine_power_required(content: &factorio_content::Content, kind: u8) -> u32 {
    if !machine_is_consumer(content, kind) {
        return 0;
    }
    content.machine(kind).map(|def| def.energy_usage).unwrap_or(0)
}

pub fn machine_produced(content: &factorio_content::Content, kind: u8) -> u32 {
    if !machine_is_producer(content, kind) {
        return 0;
    }
    content.machine(kind).map(|def| def.max_power).unwrap_or(0)
}

pub fn satisfaction(produced: u32, consumed: u32) -> u32 {
    if consumed == 0 {
        POWER_RATIO_FULL
    } else {
        produced.saturating_mul(POWER_RATIO_FULL) / consumed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::{
        MACHINE_ASSEMBLER, MACHINE_BURNER_GENERATOR, MACHINE_OFFSHORE_PUMP, MACHINE_STONE_FURNACE,
    };

    fn content() -> factorio_content::Content {
        factorio_content::Content::load()
    }

    #[test]
    fn components_classify_power_roles() {
        let content = content();
        assert!(machine_is_producer(&content, MACHINE_BURNER_GENERATOR));
        assert!(!machine_is_consumer(&content, MACHINE_BURNER_GENERATOR));
        assert!(machine_is_consumer(&content, MACHINE_ASSEMBLER));
        assert!(!machine_is_producer(&content, MACHINE_ASSEMBLER));
        assert!(!machine_is_producer(&content, MACHINE_STONE_FURNACE));
        assert!(!machine_is_consumer(&content, MACHINE_STONE_FURNACE));
        assert!(!machine_is_producer(&content, MACHINE_OFFSHORE_PUMP));
        assert!(!machine_is_consumer(&content, MACHINE_OFFSHORE_PUMP));
    }

    #[test]
    fn demand_and_output_come_from_the_right_fields() {
        let content = content();
        assert_eq!(machine_produced(&content, MACHINE_BURNER_GENERATOR), 5000);
        assert_eq!(machine_produced(&content, MACHINE_ASSEMBLER), 0);
        assert_eq!(machine_power_required(&content, MACHINE_ASSEMBLER), 90000);
        assert_eq!(machine_power_required(&content, MACHINE_STONE_FURNACE), 0);
    }

    #[test]
    fn satisfaction_is_clamped_to_full_when_idle() {
        assert_eq!(satisfaction(0, 0), POWER_RATIO_FULL);
        assert_eq!(satisfaction(5000, 5000), POWER_RATIO_FULL);
        assert_eq!(satisfaction(0, 5000), 0);
        assert_eq!(satisfaction(2500, 5000), POWER_RATIO_FULL / 2);
    }
}