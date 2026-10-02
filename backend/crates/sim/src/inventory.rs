#[derive(Clone, Debug, Default)]
pub struct Inventory {
    stacks: Vec<(u16, u16)>,
}

impl Inventory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, item: u16, count: u16) {
        if count == 0 {
            return;
        }
        if let Some((_, existing)) = self.stacks.iter_mut().find(|(i, _)| *i == item) {
            *existing = existing.saturating_add(count);
        } else {
            self.stacks.push((item, count));
            self.stacks.sort_by_key(|(item, _)| *item);
        }
    }

    pub fn take(&mut self, item: u16, count: u16) -> u16 {
        if count == 0 {
            return 0;
        }
        let mut remaining = count;
        if let Some(entry) = self.stacks.iter_mut().find(|(i, _)| *i == item) {
            let available = entry.1;
            let taken = available.min(remaining);
            entry.1 -= taken;
            remaining = remaining.saturating_sub(taken);
            if entry.1 == 0 {
                self.stacks.retain(|(i, c)| *i != item || *c > 0);
            }
        }
        count - remaining
    }

    pub fn count(&self, item: u16) -> u16 {
        self.stacks.iter().find(|(i, _)| *i == item).map(|(_, c)| *c).unwrap_or(0)
    }

    pub fn has(&self, item: u16, count: u16) -> bool {
        self.count(item) >= count
    }

    pub fn len(&self) -> usize {
        self.stacks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stacks.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (u16, u16)> + '_ {
        self.stacks.iter().map(|(item, count)| (*item, *count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_tracks_sorted_items() {
        let mut inventory = Inventory::new();
        inventory.add(2, 3);
        inventory.add(1, 4);
        inventory.add(2, 1);
        assert_eq!(inventory.count(1), 4);
        assert_eq!(inventory.count(2), 4);
        assert_eq!(inventory.take(2, 5), 4);
        assert_eq!(inventory.count(2), 0);
    }
}
