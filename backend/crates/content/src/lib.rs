use std::collections::HashMap;

pub const KIND_BELT: u8 = 0;
pub const KIND_MACHINE: u8 = 1;
pub const KIND_INSERTER: u8 = 2;
pub const KIND_PLAYER: u8 = 3;

#[derive(Clone, Debug)]
pub enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
    Array(Vec<Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }
}

pub fn parse_toml(text: &str) -> HashMap<String, HashMap<String, Value>> {
    let mut out: HashMap<String, HashMap<String, Value>> = HashMap::new();
    let mut current = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            if let Some(end) = line.find(']') {
                current = line[1..end].trim().to_string();
            }
            out.entry(current.clone()).or_default();
            continue;
        }
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim().to_string();
            if let Some(value) = parse_value(line[eq + 1..].trim()) {
                out.entry(current.clone()).or_default().insert(key, value);
            }
        }
    }
    out
}

fn parse_value(s: &str) -> Option<Value> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if t.starts_with('"') {
        if let Some(offset) = t[1..].find('"') {
            return Some(Value::Str(t[1..1 + offset].to_string()));
        }
    }
    if t == "true" {
        return Some(Value::Bool(true));
    }
    if t == "false" {
        return Some(Value::Bool(false));
    }
    if t.starts_with('[') {
        if let Some(end) = t.rfind(']') {
            let inner = &t[1..end];
            let items = split_top(inner)
                .into_iter()
                .filter(|v| !v.trim().is_empty())
                .map(|v| parse_value(&v).unwrap())
                .collect();
            return Some(Value::Array(items));
        }
    }
    if let Ok(i) = t.replace('_', "").parse::<i64>() {
        return Some(Value::Int(i));
    }
    None
}

fn split_top(s: &str) -> Vec<String> {
    if s.trim().is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(s[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(s[start..].trim().to_string());
    out
}

fn sorted_tables<'a>(
    map: &'a HashMap<String, HashMap<String, Value>>,
    prefix: &str,
) -> Vec<(String, &'a HashMap<String, Value>)> {
    let mut v: Vec<(String, &HashMap<String, Value>)> = map
        .iter()
        .filter(|(k, _)| k.starts_with(prefix))
        .map(|(k, v)| (k[prefix.len()..].to_string(), v))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn sorted_prototypes<'a>(
    map: &'a HashMap<String, HashMap<String, Value>>,
    prefix: &str,
) -> Vec<(String, &'a HashMap<String, Value>)> {
    let mut v: Vec<(String, &HashMap<String, Value>)> = map
        .iter()
        .filter(|(k, _)| k.starts_with(prefix))
        .map(|(k, v)| (k[prefix.len()..].to_string(), v))
        .collect();
    v.sort_by(|a, b| {
        int_value(a.1, "prototype")
            .cmp(&int_value(b.1, "prototype"))
            .then_with(|| a.0.cmp(&b.0))
    });
    v
}

fn str_value(table: &HashMap<String, Value>, key: &str) -> String {
    table
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn int_value(table: &HashMap<String, Value>, key: &str) -> i64 {
    table.get(key).and_then(Value::as_int).unwrap_or(0)
}

fn str_array(table: &HashMap<String, Value>, key: &str) -> Vec<String> {
    table
        .get(key)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|v| v.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn i64_array(table: &HashMap<String, Value>, key: &str) -> Vec<i64> {
    table
        .get(key)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|v| v.as_int().unwrap_or(0))
                .collect()
        })
        .unwrap_or_default()
}

fn size_value(table: &HashMap<String, Value>) -> (i32, i32) {
    let a = i64_array(table, "size");
    let w = a.first().copied().unwrap_or(1) as i32;
    let h = a.get(1).copied().unwrap_or(1) as i32;
    (w, h)
}

fn item_pairs(
    items: &HashMap<String, u16>,
    value: &Value,
) -> Vec<(u16, u16)> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|pair| {
                    let pair = pair.as_array()?;
                    let item = pair
                        .first()
                        .and_then(Value::as_str)
                        .and_then(|s| items.get(s).copied())?;
                    let count = pair.get(1).and_then(Value::as_int).unwrap_or(1) as u16;
                    Some((item, count))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Clone, Debug)]
pub struct ItemDef {
    pub id: String,
    pub name: String,
    pub max_stack: u16,
}

#[derive(Clone, Debug)]
pub struct MachineDef {
    pub id: String,
    pub name: String,
    pub size: (i32, i32),
    pub components: Vec<String>,
    pub categories: Vec<String>,
    pub energy_usage: u32,
    pub max_power: u32,
    pub fuel: u32,
    pub recipe: u16,
}

#[derive(Clone, Debug)]
pub struct BeltDef {
    pub id: String,
    pub name: String,
    pub speed_ticks: u32,
}

#[derive(Clone, Debug)]
pub struct InserterDef {
    pub id: String,
    pub name: String,
    pub swing_ticks: u32,
}

#[derive(Clone, Debug)]
pub struct EntityDef {
    pub id: String,
    pub name: String,
    pub kind: u8,
    pub prototype: u16,
    pub size: (i32, i32),
}

#[derive(Clone, Debug)]
pub struct RecipeDef {
    pub id: String,
    pub name: String,
    pub inputs: Vec<(u16, u16)>,
    pub outputs: Vec<(u16, u16)>,
    pub time_ticks: u32,
    pub categories: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TechDef {
    pub id: String,
    pub name: String,
    pub cost: u32,
    pub prerequisites: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Content {
    pub items: Vec<ItemDef>,
    pub machines: Vec<MachineDef>,
    pub belts: Vec<BeltDef>,
    pub inserters: Vec<InserterDef>,
    pub entities: Vec<EntityDef>,
    pub recipes: Vec<RecipeDef>,
    pub technologies: Vec<TechDef>,
    pub item_index: HashMap<String, u16>,
    pub entity_index: HashMap<String, u16>,
    pub recipe_index: HashMap<String, u16>,
    pub technology_index: HashMap<String, u16>,
    pub resource_items: [u16; 4],
}

impl Content {
    pub fn load() -> Self {
        let items_map = parse_toml(include_str!("../content/items.toml"));
        let machines_map = parse_toml(include_str!("../content/machines.toml"));
        let belts_map = parse_toml(include_str!("../content/belts.toml"));
        let inserters_map = parse_toml(include_str!("../content/inserters.toml"));
        let entities_map = parse_toml(include_str!("../content/entities.toml"));
        let recipes_map = parse_toml(include_str!("../content/recipes.toml"));
        let technologies_map = parse_toml(include_str!("../content/technology.toml"));

        let item_entries = sorted_tables(&items_map, "item.");
        let items: Vec<ItemDef> = item_entries
            .iter()
            .map(|(id, v)| ItemDef {
                id: id.clone(),
                name: str_value(v, "name"),
                max_stack: int_value(v, "max_stack") as u16,
            })
            .collect();
        let item_index: HashMap<String, u16> = items
            .iter()
            .enumerate()
            .map(|(i, item)| (item.id.clone(), i as u16))
            .collect();

        let recipe_entries = sorted_tables(&recipes_map, "recipe.");
        let recipes: Vec<RecipeDef> = recipe_entries
            .iter()
            .map(|(id, v)| RecipeDef {
                id: id.clone(),
                name: str_value(v, "name"),
                inputs: item_pairs(&item_index, v.get("inputs").unwrap_or(&Value::Array(Vec::new()))),
                outputs: item_pairs(&item_index, v.get("outputs").unwrap_or(&Value::Array(Vec::new()))),
                time_ticks: int_value(v, "time_ticks") as u32,
                categories: str_array(v, "categories"),
            })
            .collect();
        let recipe_index: HashMap<String, u16> = recipes
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id.clone(), i as u16))
            .collect();

        let machine_entries = sorted_prototypes(&machines_map, "machines.");
        let machines: Vec<MachineDef> = machine_entries
            .iter()
            .map(|(id, v)| MachineDef {
                id: id.clone(),
                name: str_value(v, "name"),
                size: size_value(v),
                components: str_array(v, "components"),
                categories: str_array(v, "categories"),
                energy_usage: int_value(v, "energy_usage") as u32,
                max_power: int_value(v, "max_power") as u32,
                fuel: int_value(v, "fuel") as u32,
                recipe: int_value(v, "recipe") as u16,
            })
            .collect();

        let belt_entries = sorted_prototypes(&belts_map, "belts.");
        let belts: Vec<BeltDef> = belt_entries
            .iter()
            .map(|(id, v)| BeltDef {
                id: id.clone(),
                name: str_value(v, "name"),
                speed_ticks: int_value(v, "speed_ticks") as u32,
            })
            .collect();

        let inserter_entries = sorted_prototypes(&inserters_map, "inserters.");
        let inserters: Vec<InserterDef> = inserter_entries
            .iter()
            .map(|(id, v)| InserterDef {
                id: id.clone(),
                name: str_value(v, "name"),
                swing_ticks: int_value(v, "swing_ticks") as u32,
            })
            .collect();

        let entity_entries = sorted_tables(&entities_map, "entities.");
        let entities: Vec<EntityDef> = entity_entries
            .iter()
            .map(|(id, v)| EntityDef {
                id: id.clone(),
                name: str_value(v, "name"),
                kind: kind_from_str(str_value(v, "kind").as_str()),
                prototype: int_value(v, "prototype") as u16,
                size: size_value(v),
            })
            .collect();
        let entity_index: HashMap<String, u16> = entities
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i as u16))
            .collect();

        let tech_entries = sorted_tables(&technologies_map, "technology.");
        let technologies: Vec<TechDef> = tech_entries
            .iter()
            .map(|(id, v)| TechDef {
                id: id.clone(),
                name: str_value(v, "name"),
                cost: int_value(v, "cost") as u32,
                prerequisites: str_array(v, "prerequisites"),
            })
            .collect();
        let technology_index: HashMap<String, u16> = technologies
            .iter()
            .enumerate()
            .map(|(i, tech)| (tech.id.clone(), i as u16))
            .collect();

        let resource_items = [
            item_index.get("metal_ore").copied().unwrap_or(0),
            item_index.get("copper_ore").copied().unwrap_or(0),
            item_index.get("stone_ore").copied().unwrap_or(0),
            item_index.get("coal").copied().unwrap_or(0),
        ];

        Content {
            items,
            machines,
            belts,
            inserters,
            entities,
            recipes,
            technologies,
            item_index,
            entity_index,
            recipe_index,
            technology_index,
            resource_items,
        }
    }

    pub fn item(&self, index: u16) -> Option<&ItemDef> {
        self.items.get(index as usize)
    }

    pub fn machine(&self, index: u8) -> Option<&MachineDef> {
        self.machines.get(index as usize)
    }

    pub fn belt(&self, index: u16) -> Option<&BeltDef> {
        self.belts.get(index as usize)
    }

    pub fn inserter(&self, index: u16) -> Option<&InserterDef> {
        self.inserters.get(index as usize)
    }

    pub fn entity(&self, index: u16) -> Option<&EntityDef> {
        self.entities.get(index as usize)
    }

    pub fn recipe(&self, index: u16) -> Option<&RecipeDef> {
        self.recipes.get(index as usize)
    }

    pub fn belt_speed(&self) -> u32 {
        self.belts.first().map(|b| b.speed_ticks).unwrap_or(30)
    }

    pub fn inserter_swing(&self, index: u16) -> u32 {
        self.inserters.get(index as usize).map(|i| i.swing_ticks).unwrap_or(30)
    }
}

fn kind_from_str(s: &str) -> u8 {
    match s {
        "belt" => KIND_BELT,
        "machine" => KIND_MACHINE,
        "inserter" => KIND_INSERTER,
        "player" => KIND_PLAYER,
        _ => KIND_BELT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_content() {
        let content = Content::load();
        assert!(content.items.len() >= 10);
        assert!(content.machines.len() >= 5);
        assert!(content.recipes.len() >= 5);
        assert!(content.entities.iter().any(|e| e.id == "transport_belt"));
        assert!(content.belts.iter().any(|b| b.id == "transport"));
        assert!(content.inserters.len() >= 2);
        assert_eq!(content.resource_items[0], content.item_index["metal_ore"]);
        assert_eq!(content.resource_items[3], content.item_index["coal"]);
    }

    #[test]
    fn machine_definitions_are_ordered_by_prototype() {
        let content = Content::load();
        assert_eq!(content.machines[0].id, "burner_generator");
        assert_eq!(content.machines[1].id, "stone_furnace");
        assert_eq!(content.machines[2].id, "assembler");
        assert_eq!(content.machines[3].id, "offshore_pump");
        assert_eq!(content.machines[4].id, "lab");
        assert_eq!(content.machines[0].max_power, 5000);
        assert_eq!(content.machines[2].size, (3, 3));
        assert_eq!(content.machines[2].energy_usage, 90000);
        let stone_furnace = content.machine(1).unwrap();
        assert_eq!(stone_furnace.size, (2, 2));
        assert!(content.recipe(stone_furnace.recipe).is_some());
    }

    #[test]
    fn recipe_inputs_resolve_to_item_indices() {
        let content = Content::load();
        let gear = content.recipe(content.recipe_index["gear"]).unwrap();
        assert_eq!(gear.inputs.len(), 2);
        assert_eq!(gear.inputs[0], (content.item_index["metal_plate"], 1));
        assert_eq!(gear.inputs[1], (content.item_index["copper_plate"], 1));
        assert_eq!(gear.time_ticks, 20);
        for recipe in content.recipes.iter() {
            assert!(!recipe.inputs.is_empty(), "{} has no inputs", recipe.id);
            assert!(!recipe.outputs.is_empty(), "{} has no outputs", recipe.id);
        }
    }

    #[test]
    fn technologies_load_in_sorted_order() {
        let content = Content::load();
        let ids: Vec<&str> = content.technologies.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["assembler", "burner", "lab", "offshore_pump", "stone_furnace"]);
        assert_eq!(content.technologies[0].cost, 100);
        assert_eq!(content.technologies[1].cost, 0);
        assert_eq!(content.technology_index["lab"], 2);
        assert_eq!(content.technologies[2].prerequisites, vec!["stone_furnace".to_string()]);
    }

    #[test]
    fn inserters_are_ordered_by_prototype() {
        let content = Content::load();
        assert_eq!(content.inserters[0].id, "inserter");
        assert_eq!(content.inserters[1].id, "fast_inserter");
        assert_eq!(content.inserter_swing(1), 12);
        assert_eq!(content.belt_speed(), 30);
    }

    #[test]
    fn parses_nested_arrays() {
        let value = parse_value("[[\"a\", 1], [\"b\", 2]]").unwrap();
        let arr = value.as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }
}
