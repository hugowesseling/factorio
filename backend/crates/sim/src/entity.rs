#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub struct BeltId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub struct MachineId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub struct InserterId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub struct PlayerId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AnyId {
    Belt(BeltId),
    Machine(MachineId),
    Inserter(InserterId),
    Player(PlayerId),
}

impl AnyId {
    pub fn kind(self) -> u8 {
        match self {
            AnyId::Belt(_) => 0,
            AnyId::Machine(_) => 1,
            AnyId::Inserter(_) => 2,
            AnyId::Player(_) => 3,
        }
    }

    pub fn index(self) -> u32 {
        match self {
            AnyId::Belt(id) => id.0,
            AnyId::Machine(id) => id.0,
            AnyId::Inserter(id) => id.0,
            AnyId::Player(id) => id.0,
        }
    }
}

pub const SENTINEL: u32 = u32::MAX;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Stack {
    pub item: u16,
    pub count: u16,
}

impl Stack {
    pub fn new(item: u16, count: u16) -> Self {
        Self { item, count }
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn add(&mut self, item: u16, count: u16) {
        if self.count == 0 {
            self.item = item;
        }
        self.count = self.count.saturating_add(count);
    }

    pub fn take(&mut self, count: u16) -> u16 {
        let taken = self.count.min(count);
        self.count -= taken;
        taken
    }
}

pub fn stack_add(buf: &mut [Stack; 4], item: u16, count: u16) -> u16 {
    if count == 0 {
        return 0;
    }
    let mut remaining = count;
    for slot in buf.iter_mut() {
        if remaining == 0 {
            break;
        }
        if slot.count == 0 {
            slot.item = item;
            slot.count = remaining.min(65535);
            remaining = remaining.saturating_sub(slot.count);
        } else if slot.item == item {
            let space = 65535u16.saturating_sub(slot.count);
            let put = remaining.min(space);
            slot.count += put;
            remaining = remaining.saturating_sub(put);
        }
    }
    count - remaining
}

pub fn stack_take(buf: &mut [Stack; 4], item: u16, count: u16) -> u16 {
    if count == 0 {
        return 0;
    }
    let mut remaining = count;
    for slot in buf.iter_mut() {
        if remaining == 0 {
            break;
        }
        if slot.item == item && slot.count > 0 {
            let take = remaining.min(slot.count);
            slot.count -= take;
            remaining = remaining.saturating_sub(take);
        }
    }
    count - remaining
}

pub fn stack_contains(buf: &[Stack; 4], item: u16) -> u16 {
    buf.iter().filter(|s| s.item == item).map(|s| s.count).sum()
}

pub fn stack_any(buf: &mut [Stack; 4]) -> Option<Stack> {
    for slot in buf.iter_mut() {
        if slot.count > 0 {
            let item = slot.item;
            let count = slot.take(1);
            return Some(Stack { item, count });
        }
    }
    None
}

pub struct Pool<T> {
    items: Vec<T>,
    free: Vec<u32>,
}

impl<T> Pool<T> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            free: Vec::new(),
        }
    }

    pub fn alloc(&mut self, value: T) -> u32 {
        if let Some(index) = self.free.pop() {
            self.items[index as usize] = value;
            index
        } else {
            let index = self.items.len() as u32;
            self.items.push(value);
            index
        }
    }

    pub fn dealloc(&mut self, index: u32) -> Option<T> {
        if index as usize >= self.items.len() {
            return None;
        }
        let value = self.items.swap_remove(index as usize);
        self.free.push(index);
        Some(value)
    }

    #[inline]
    pub fn get(&self, index: u32) -> Option<&T> {
        self.items.get(index as usize)
    }

    #[inline]
    pub fn get_mut(&mut self, index: u32) -> Option<&mut T> {
        self.items.get_mut(index as usize)
    }

    #[inline]
    pub fn len(&self) -> u32 {
        self.items.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (u32, &T)> {
        (0..self.items.len()).map(move |i| (i as u32, &self.items[i]))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (u32, &mut T)> {
        self.items.iter_mut().enumerate().map(|(i, item)| (i as u32, item))
    }
}

impl<T> Default for Pool<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_reuses_freed_index() {
        let mut pool = Pool::new();
        let a = pool.alloc(1);
        let b = pool.alloc(2);
        let c = pool.alloc(3);
        assert_eq!((a, b, c), (0, 1, 2));
        assert_eq!(pool.dealloc(a), Some(1));
        assert_eq!(pool.len(), 2);
        assert_eq!(pool.get(b).unwrap(), &2);
        let d = pool.alloc(4);
        assert_eq!(d, a);
        assert_eq!(pool.get(d).unwrap(), &4);
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn dealloc_swap_removes_into_the_hole() {
        let mut pool = Pool::new();
        pool.alloc(10);
        pool.alloc(20);
        pool.alloc(30);
        assert_eq!(pool.dealloc(0), Some(10));
        assert_eq!(pool.get(0).unwrap(), &30);
        assert_eq!(pool.get(1).unwrap(), &20);
        assert!(pool.get(2).is_none());
    }

    #[test]
    fn iter_visits_ascending_indices() {
        let mut pool = Pool::new();
        pool.alloc(5);
        pool.alloc(6);
        pool.alloc(7);
        let keys: Vec<u32> = pool.iter().map(|(index, _)| index).collect();
        assert_eq!(keys, vec![0, 1, 2]);
        let values: Vec<i32> = pool.iter().map(|(_, value)| *value).collect();
        assert_eq!(values, vec![5, 6, 7]);
        for (index, value) in pool.iter_mut() {
            *value += index as i32;
        }
        let values: Vec<i32> = pool.iter().map(|(_, value)| *value).collect();
        assert_eq!(values, vec![5, 7, 9]);
    }

    #[test]
    fn stack_helpers_work() {
        let mut buf = [Stack::default(); 4];
        assert_eq!(stack_add(&mut buf, 7, 5), 5);
        assert_eq!(stack_contains(&buf, 7), 5);
        assert_eq!(stack_take(&mut buf, 7, 3), 3);
        assert_eq!(stack_contains(&buf, 7), 2);
    }
}
