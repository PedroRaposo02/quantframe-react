use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use utils::{get_location, info, read_json_file_optional, Error, LoggerOptions, MultiKeyMap};

use crate::cache::{
    client::CacheState,
    modules::LanguageModule,
    types::{CacheSetGroup, CacheTradableItem},
};

#[derive(Debug)]
pub struct TradableItemModule {
    path: PathBuf,
    items: Mutex<Vec<CacheTradableItem>>,

    // Lookup maps
    item_lookup: Mutex<MultiKeyMap<CacheTradableItem>>,
    set_groups: Mutex<Vec<CacheSetGroup>>,
}

impl TradableItemModule {
    pub fn new(client: Arc<CacheState>) -> Arc<Self> {
        Arc::new(Self {
            path: client.base_path.join("items/TradableItems.json"),
            items: Mutex::new(Vec::new()),
            item_lookup: Mutex::new(MultiKeyMap::new()),
            set_groups: Mutex::new(Vec::new()),
        })
    }
    pub fn get_items(&self) -> Result<Vec<CacheTradableItem>, Error> {
        let items = self
            .items
            .lock()
            .expect("Failed to lock items mutex")
            .clone();
        Ok(items)
    }
    pub fn load(&self, language: &LanguageModule) -> Result<(), Error> {
        match read_json_file_optional::<Vec<CacheTradableItem>>(&self.path) {
            Ok(mut items) => {
                let mut item_lookup = self.item_lookup.lock().unwrap();
                for item in items.iter_mut() {
                    item.translate(&language);

                    let mut keys = vec![
                        item.wfm_id.clone(),
                        item.name.clone(),
                        item.wfm_url.clone(),
                        item.unique_name.clone(),
                    ];
                    if !item.variant_to_unique_name.is_empty() {
                        keys.extend(item.variant_to_unique_name.values().cloned());
                    }

                    item_lookup.insert_value(item.clone(), keys);
                }

                *self.set_groups.lock().unwrap() = build_set_groups(&items);

                let mut items_lock = self.items.lock().unwrap();
                *items_lock = items;
                info(
                    "Cache:TradableItem:load",
                    format!("Loaded {} tradable items from cache", items_lock.len()),
                    &LoggerOptions::default(),
                );
            }
            Err(e) => return Err(e.with_location(get_location!())),
        }
        Ok(())
    }
    /* -------------------------------------------------------------
        Lookup Functions
    ------------------------------------------------------------- */
    /// Get a tradable item by various identifiers
    ///  # Arguments
    /// - `item_id`: The identifier to search for (name, url, unique name, or id)
    ///
    pub fn get_by(&self, item_id: impl Into<String>) -> Result<CacheTradableItem, Error> {
        let item_id: String = item_id.into();
        let item_lookup = self.item_lookup.lock().unwrap();
        if let Some(item) = item_lookup.get(&item_id) {
            Ok(item.clone())
        } else {
            Err(Error::new(
                "Cache:TradableItem:GetBy",
                format!("Tradable item not found for id '{}'", item_id),
                get_location!(),
            ))
        }
    }

    /* -------------------------------------------------------------
        Set Detection
    ------------------------------------------------------------- */
    /// Given a map of unique_name -> owned quantity (e.g. everything received/offered in a
    /// trade), finds every part group (e.g. Chassis/Neuroptics/Systems/Blueprint) that is fully
    /// present and returns how many complete sets can be formed, along with how much of each
    /// part gets consumed per set item.
    pub fn find_matching_sets(
        &self,
        owned_counts: &HashMap<String, i64>,
    ) -> Vec<(CacheTradableItem, i64, Vec<(String, i64)>)> {
        let groups = self.set_groups.lock().unwrap();
        let mut matches = Vec::new();

        for group in groups.iter() {
            let mut sets_formed = i64::MAX;
            for part in &group.parts {
                let owned = owned_counts.get(&part.unique_name).copied().unwrap_or(0);
                if owned <= 0 {
                    sets_formed = 0;
                    break;
                }
                sets_formed = sets_formed.min(owned);
            }

            if sets_formed > 0 {
                let consumed = group
                    .parts
                    .iter()
                    .map(|part| (part.unique_name.clone(), sets_formed))
                    .collect();
                matches.push((group.set_item.clone(), sets_formed, consumed));
            }
        }

        matches
    }

    /// For every part that belongs to a known set group, reports whether that set is currently
    /// complete (every part, including the main blueprint, is owned) given the provided owned
    /// counts. Parts that aren't part of any known set are simply absent from the result.
    pub fn set_completeness(&self, owned_counts: &HashMap<String, i64>) -> HashMap<String, bool> {
        let groups = self.set_groups.lock().unwrap();
        let mut result = HashMap::new();

        for group in groups.iter() {
            let is_complete = group
                .parts
                .iter()
                .all(|part| owned_counts.get(&part.unique_name).copied().unwrap_or(0) > 0);

            for part in &group.parts {
                if owned_counts.contains_key(&part.unique_name) {
                    let entry = result.entry(part.unique_name.clone()).or_insert(false);
                    *entry = *entry || is_complete;
                }
            }
        }

        result
    }
}

/// Resolves which tradable items make up a crafted "Set" (e.g. Volt Prime Set) using the naming
/// convention Warframe data follows: a part is named "<SetBaseName> <PartName>" and tagged
/// "component" and/or "blueprint", while the finished set is tagged "set" and named
/// "<SetBaseName> Set". Built dynamically from the cache so it works for every tradable item
/// rather than a hardcoded list - needed because the server-provided recipe/ingredients data
/// (Recipes.json) doesn't currently carry ingredient info for any item.
fn build_set_groups(items: &[CacheTradableItem]) -> Vec<CacheSetGroup> {
    let mut groups = Vec::new();

    for set_item in items.iter().filter(|i| i.tags.iter().any(|t| t == "set")) {
        let Some(base_name) = set_item.name.strip_suffix(" Set") else {
            continue;
        };
        let prefix = format!("{} ", base_name);

        let parts: Vec<CacheTradableItem> = items
            .iter()
            .filter(|i| i.unique_name != set_item.unique_name)
            .filter(|i| i.name.starts_with(&prefix))
            .filter(|i| i.tags.iter().any(|t| t == "component" || t == "blueprint"))
            .cloned()
            .collect();

        if !parts.is_empty() {
            groups.push(CacheSetGroup {
                set_item: set_item.clone(),
                parts,
            });
        }
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tradable_item(name: &str, unique_name: &str, tags: &[&str]) -> CacheTradableItem {
        CacheTradableItem {
            name: name.to_string(),
            unique_name: unique_name.to_string(),
            wfm_id: format!("id_{unique_name}"),
            wfm_url: format!("url_{unique_name}"),
            trade_tax: 2000,
            mr_requirement: 0,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            icon: String::new(),
            bulk_tradable: false,
            sub_type: None,
            variant_to_unique_name: HashMap::new(),
        }
    }

    fn volt_prime_items() -> Vec<CacheTradableItem> {
        vec![
            tradable_item("Volt Prime Set", "VoltPrime", &["set", "prime", "warframe"]),
            tradable_item(
                "Volt Prime Blueprint",
                "VoltPrimeBlueprint",
                &["blueprint", "prime", "warframe"],
            ),
            tradable_item(
                "Volt Prime Chassis Blueprint",
                "VoltPrimeChassisBlueprint",
                &["component", "blueprint", "prime", "warframe"],
            ),
            tradable_item(
                "Volt Prime Neuroptics Blueprint",
                "VoltPrimeHelmetBlueprint",
                &["component", "blueprint", "prime", "warframe"],
            ),
            tradable_item(
                "Volt Prime Systems Blueprint",
                "VoltPrimeSystemsBlueprint",
                &["component", "blueprint", "prime", "warframe"],
            ),
        ]
    }

    #[test]
    fn detects_a_complete_set() {
        let groups = build_set_groups(&volt_prime_items());
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].parts.len(), 4);

        let owned: HashMap<String, i64> = [
            ("VoltPrimeBlueprint".to_string(), 1),
            ("VoltPrimeChassisBlueprint".to_string(), 1),
            ("VoltPrimeHelmetBlueprint".to_string(), 1),
            ("VoltPrimeSystemsBlueprint".to_string(), 1),
        ]
        .into_iter()
        .collect();

        let module = TradableItemModule {
            path: PathBuf::new(),
            items: Mutex::new(vec![]),
            item_lookup: Mutex::new(MultiKeyMap::new()),
            set_groups: Mutex::new(groups),
        };

        let matches = module.find_matching_sets(&owned);
        assert_eq!(matches.len(), 1);
        let (set_item, sets_formed, consumed) = &matches[0];
        assert_eq!(set_item.unique_name, "VoltPrime");
        assert_eq!(*sets_formed, 1);
        assert_eq!(consumed.len(), 4);
    }

    #[test]
    fn does_not_match_a_partial_set() {
        let groups = build_set_groups(&volt_prime_items());

        // Missing the main Blueprint - should not fold into a Set.
        let owned: HashMap<String, i64> = [
            ("VoltPrimeChassisBlueprint".to_string(), 1),
            ("VoltPrimeHelmetBlueprint".to_string(), 1),
            ("VoltPrimeSystemsBlueprint".to_string(), 1),
        ]
        .into_iter()
        .collect();

        let module = TradableItemModule {
            path: PathBuf::new(),
            items: Mutex::new(vec![]),
            item_lookup: Mutex::new(MultiKeyMap::new()),
            set_groups: Mutex::new(groups),
        };

        assert!(module.find_matching_sets(&owned).is_empty());
    }

    #[test]
    fn multiple_sets_in_one_trade_keep_leftover_parts() {
        let groups = build_set_groups(&volt_prime_items());

        // 2 complete sets plus one spare Chassis Blueprint.
        let owned: HashMap<String, i64> = [
            ("VoltPrimeBlueprint".to_string(), 2),
            ("VoltPrimeChassisBlueprint".to_string(), 3),
            ("VoltPrimeHelmetBlueprint".to_string(), 2),
            ("VoltPrimeSystemsBlueprint".to_string(), 2),
        ]
        .into_iter()
        .collect();

        let module = TradableItemModule {
            path: PathBuf::new(),
            items: Mutex::new(vec![]),
            item_lookup: Mutex::new(MultiKeyMap::new()),
            set_groups: Mutex::new(groups),
        };

        let matches = module.find_matching_sets(&owned);
        assert_eq!(matches.len(), 1);
        let (_, sets_formed, consumed) = &matches[0];
        assert_eq!(*sets_formed, 2);
        // Only 2 of the 3 owned Chassis Blueprints get consumed - 1 survives as leftover.
        let chassis_consumed = consumed
            .iter()
            .find(|(name, _)| name == "VoltPrimeChassisBlueprint")
            .unwrap();
        assert_eq!(chassis_consumed.1, 2);
    }
}
