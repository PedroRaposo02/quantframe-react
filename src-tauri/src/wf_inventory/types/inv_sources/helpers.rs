use crate::wf_inventory::WarframeRootObject;
use serde_json::Value;
use utils::*;

pub fn block_on_async<F: std::future::Future>(future: F) -> F::Output {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(future))
    } else {
        tokio::runtime::Runtime::new().unwrap().block_on(future)
    }
}

pub fn parse_lastdata(raw: &str) -> Result<WarframeRootObject, Error> {
    let mut json = raw.trim_end_matches(|c| c != '}').to_string();

    let json_value = serde_json::from_str::<Value>(&json).map_err(|e| Error::from(e))?;
    if json_value.get("InventoryJson").is_some() {
        info(
            "WFInventory:ParseLastData",
            "Detected InventoryJson field, extracting inventory data",
            &LoggerOptions::default(),
        );
        json = json_value["InventoryJson"]
            .to_string()
            .replace("\\\"", "\"")
            .replace("\\\"", "\"");
        json = json[1..json.len() - 1].to_string();
    }
    let mut value: Value = serde_json::from_str(&json).map_err(|e| Error::from(e))?;
    if value.get("Results").is_some() && value.get("Stats").is_some() {
        info(
            "WFInventory:ParseLastData",
            "Detected Results and Stats fields, merging them into a single object",
            &LoggerOptions::default(),
        );
        let result = value["Results"].get(0).cloned().unwrap_or_default();
        let stats = value["Stats"].clone();
        if result.is_object() && stats.is_object() {
            let mut merged = result.as_object().unwrap().clone();
            for (key, val) in stats.as_object().unwrap() {
                merged.insert(key.clone(), val.clone());
            }
            value = Value::Object(merged);
        }
    }
    let equipped_upgrade_ids = extract_equipped_upgrade_ids(&value);

    let mut root: WarframeRootObject = serde_json::from_value(value).map_err(|e| Error::from(e))?;
    root.equipped_upgrade_ids = equipped_upgrade_ids;
    Ok(root)
}

/// Equipment categories whose `Configs[].Upgrades[]` arrays list equipped mod/arcane instance
/// IDs. Every loadout slot across every owned piece of equipment is checked, since switching
/// loadouts re-equips whatever's saved there - a mod saved in any config counts as equipped.
const EQUIPMENT_KEYS: &[&str] = &[
    "Suits",
    "LongGuns",
    "Pistols",
    "Melee",
    "Sentinels",
    "SentinelWeapons",
    "MechSuits",
    "OperatorSuits",
    "SpaceGuns",
    "SpaceMelee",
];

fn extract_equipped_upgrade_ids(value: &Value) -> std::collections::HashSet<String> {
    let mut ids = std::collections::HashSet::new();

    for key in EQUIPMENT_KEYS {
        let Some(items) = value.get(*key).and_then(|v| v.as_array()) else {
            continue;
        };
        for item in items {
            let Some(configs) = item.get("Configs").and_then(|v| v.as_array()) else {
                continue;
            };
            for config in configs {
                let Some(upgrades) = config.get("Upgrades").and_then(|v| v.as_array()) else {
                    continue;
                };
                for id in upgrades {
                    if let Some(id_str) = id.as_str() {
                        if !id_str.is_empty() {
                            ids.insert(id_str.to_string());
                        }
                    }
                }
            }
        }
    }

    ids
}
