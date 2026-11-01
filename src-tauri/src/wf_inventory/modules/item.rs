use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use entity::{dto::PaginatedResult, enums::FieldChange};
use utils::{SortDirection, SubType};
use utils::Error;
use wf_market::enums::OrderType;

use crate::{
    app::AppState,
    cache::client::CacheState,
    helper::paginate,
    utils::{modules::states, SubTypeExt},
    wf_inventory::{item_base::WFInvItemBase, *},
};

/// Sentinel passed as `item_types` to mean "none of the known category tags" (the "Other"
/// bucket on the frontend), rather than an inclusion match.
pub const OTHER_CATEGORY_MARKER: &str = "__other__";

/// Every tag used by a specific frontend category filter, kept in sync with
/// `src/pages/wf_inventory/Tabs/Items/index.tsx`'s `CATEGORIES` list.
const KNOWN_CATEGORY_TAGS: &[&str] = &[
    "mod",
    "arcane_enhancement",
    "arcane_helmet",
    "relic",
    "component",
    "blueprint",
];

#[derive(Debug)]
pub struct ItemModule {
    client: Weak<WFInventoryState>,
}

impl ItemModule {
    /**
     * Creates a new `ItemModule`.
     * The `client` parameter is an `Arc<WFInventoryState>` that allows the module
     * to access the wf_inventory state.
     */
    pub fn new(client: Arc<WFInventoryState>) -> Arc<Self> {
        Arc::new(Self {
            client: Arc::downgrade(&client),
        })
    }

    /// Lists owned non-riven mods and regular tradable items (prime parts, relics, resources,
    /// etc.), grouped by unique_name (and rank for mods).
    pub fn get_items(&self, query: WFItemPaginationDto) -> Result<PaginatedResult<WFInvItemBase>, Error> {
        let client = self.client.upgrade().expect("Client should not be dropped");
        let root = client.get_root();
        let cache = states::cache_client()?;

        // Non-riven mods, grouped by (unique_name, rank) since each rank is priced separately.
        // Owned totals here are the TRUE total - equipped copies are tracked separately and
        // only subtracted when computing the sellable count, so equipped items stay visible
        // (and filterable) instead of silently disappearing.
        let mut owned_mods: HashMap<(String, i64), i64> = HashMap::new();
        let mut equipped_mods: HashMap<(String, i64), i64> = HashMap::new();
        for raw in root.raw_upgrades.iter().chain(root.upgrades.iter()) {
            if raw.is_riven() {
                continue;
            }
            let rank = raw.get_upgrade_fingerprint().mod_rank;
            let key = (raw.unique_name.clone(), rank);
            let is_equipped = raw
                .id
                .id
                .as_ref()
                .is_some_and(|id| root.equipped_upgrade_ids.contains(id));
            if is_equipped {
                *equipped_mods.entry(key.clone()).or_insert(0) += 1;
            }
            *owned_mods.entry(key).or_insert(0) += raw.quantity.max(1);
        }

        // Regular tradable items (prime parts, relics, resources, etc.), no sub_type.
        let mut owned_misc: HashMap<String, i64> = HashMap::new();
        for raw in root.misc_items.iter() {
            *owned_misc.entry(raw.unique_name.clone()).or_insert(0) += raw.quantity;
        }

        // Unique_name -> total owned (mods and misc together, ignoring rank) - what the set
        // completeness check and the "part of a set" lookups key off.
        let mut owned_counts: HashMap<String, i64> = HashMap::new();
        for ((unique_name, _), quantity) in owned_mods.iter() {
            *owned_counts.entry(unique_name.clone()).or_insert(0) += quantity;
        }
        for (unique_name, quantity) in owned_misc.iter() {
            *owned_counts.entry(unique_name.clone()).or_insert(0) += quantity;
        }
        let set_completeness = cache.tradable_item().set_completeness(&owned_counts);

        let app = states::app_state()?;

        // Global "always keep N copies for myself" reserve, same mechanism as equipped copies:
        // held back from the sellable count rather than hiding the item outright.
        let reserve_quantity = match &query.properties {
            FieldChange::Value(properties) => {
                properties.get_property_value::<i64>("reserve_quantity", 0)
            }
            _ => 0,
        };

        let mut items: Vec<WFInvItemBase> = vec![];
        for ((unique_name, rank), owned) in owned_mods {
            let equipped = equipped_mods.get(&(unique_name.clone(), rank)).copied().unwrap_or(0);
            if let Ok(mut item) =
                WFInvItemBase::from_raw(&unique_name, owned, Some(SubType::rank(rank)), &cache)
            {
                apply_market_properties(&mut item, &cache, &app, &set_completeness);
                apply_ownership_properties(&mut item, owned, equipped, reserve_quantity);
                apply_mod_effect(&mut item, &cache, rank);
                items.push(item);
            }
        }
        for (unique_name, owned) in owned_misc {
            if let Ok(mut item) = WFInvItemBase::from_raw(&unique_name, owned, None, &cache) {
                apply_market_properties(&mut item, &cache, &app, &set_completeness);
                apply_ownership_properties(&mut item, owned, 0, reserve_quantity);
                items.push(item);
            }
        }

        if let FieldChange::Value(search) = &query.query {
            let search = search.to_lowercase();
            items.retain(|item| item.name.to_lowercase().contains(&search));
        }

        // `item_types` doubles as a category filter here: each entry is a tag (e.g. "mod",
        // "relic", "component") and an item matches if it carries any of them. The reserved
        // "__other__" marker instead keeps items that match none of the known category tags.
        if let FieldChange::Value(item_types) = &query.item_types {
            if item_types.iter().any(|t| t == OTHER_CATEGORY_MARKER) {
                items.retain(|item| {
                    let tags = item.properties.get_property_value::<Vec<String>>("tags", vec![]);
                    !KNOWN_CATEGORY_TAGS.iter().any(|t| tags.contains(&t.to_string()))
                });
            } else {
                items.retain(|item| {
                    let tags = item.properties.get_property_value::<Vec<String>>("tags", vec![]);
                    item_types.iter().any(|t| tags.contains(t))
                });
            }
        }

        if let FieldChange::Value(properties) = &query.properties {
            if let Some(min_quantity) = properties.get_property_value::<Option<i64>>("min_quantity", None) {
                items.retain(|item| item.quantity >= min_quantity);
            }
            if let Some(min_price) = properties.get_property_value::<Option<f64>>("min_price", None) {
                items.retain(|item| {
                    item.properties.get_property_value::<Option<f64>>("price", None).unwrap_or(0.0) >= min_price
                });
            }
            if let Some(is_listed) = properties.get_property_value::<Option<bool>>("is_listed", None) {
                items.retain(|item| item.properties.get_property_value("is_listed", false) == is_listed);
            }
            match properties
                .get_property_value::<String>("set_status", String::new())
                .as_str()
            {
                "complete" => items.retain(|item| item.properties.get_property_value("set_complete", false)),
                "incomplete" => items.retain(|item| {
                    item.properties.has_property("set_complete")
                        && !item.properties.get_property_value("set_complete", false)
                }),
                _ => {}
            }
            match properties
                .get_property_value::<String>("equipped_status", String::new())
                .as_str()
            {
                "equipped" => items.retain(|item| item.properties.get_property_value("is_equipped", false)),
                "not_equipped" => items.retain(|item| !item.properties.get_property_value("is_equipped", false)),
                _ => {}
            }
        }

        if let FieldChange::Value(sort_by) = &query.sort_by {
            let dir = match &query.sort_direction {
                FieldChange::Value(dir) => dir.clone(),
                _ => SortDirection::Asc,
            };
            match sort_by.as_str() {
                "quantity" => items.sort_by(|a, b| match dir {
                    SortDirection::Asc => a.quantity.cmp(&b.quantity),
                    SortDirection::Desc => b.quantity.cmp(&a.quantity),
                }),
                "name" => items.sort_by(|a, b| match dir {
                    SortDirection::Asc => a.name.cmp(&b.name),
                    SortDirection::Desc => b.name.cmp(&a.name),
                }),
                "price" => items.sort_by(|a, b| {
                    let pa = a.properties.get_property_value::<f64>("price", 0.0);
                    let pb = b.properties.get_property_value::<f64>("price", 0.0);
                    let ord = pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal);
                    match dir {
                        SortDirection::Asc => ord,
                        SortDirection::Desc => ord.reverse(),
                    }
                }),
                _ => {}
            }
        }

        Ok(paginate(&items, query.pagination.page, query.pagination.limit))
    }
}

/// Attaches market-derived properties used for filtering/sorting: a reference sell price (from
/// the cached price history), whether the item currently has a live sell order posted, and
/// whether the set it belongs to (if any) is currently complete.
fn apply_market_properties(
    item: &mut WFInvItemBase,
    cache: &CacheState,
    app: &AppState,
    set_completeness: &HashMap<String, bool>,
) {
    if let Ok(Some(price_info)) = cache.item_price().find_by(&item.wfm_url, item.sub_type.clone()) {
        let price = if price_info.min_price > 0.0 {
            price_info.min_price
        } else {
            price_info.avg_price
        };
        item.properties.set_property_value("price", price);
    }

    let is_listed = app
        .wfm_client
        .order()
        .cache_orders()
        .find_order(&item.id, &SubTypeExt::from_entity(item.sub_type.clone()), OrderType::Sell)
        .is_some();
    item.properties.set_property_value("is_listed", is_listed);

    if let Some(&complete) = set_completeness.get(&item.unique_name) {
        item.properties.set_property_value("set_complete", complete);
    }
}

/// Attaches the owned/equipped/sellable breakdown. `item.quantity` stays the true total owned
/// (so "Owned: N" is always accurate); `sellable` is what's actually safe to list, after holding
/// back equipped copies and the user's configured reserve.
fn apply_ownership_properties(item: &mut WFInvItemBase, owned: i64, equipped: i64, reserve: i64) {
    let sellable = (owned - equipped - reserve).max(0);
    item.properties.set_property_value("equipped", equipped);
    item.properties.set_property_value("sellable", sellable);
    item.properties.set_property_value("is_equipped", equipped > 0);
}

/// For mods, attaches the current rank's stat text plus the full rank progression, so the
/// frontend can show "what does this mod actually do" rather than just an icon.
fn apply_mod_effect(item: &mut WFInvItemBase, cache: &CacheState, rank: i64) {
    let Ok(mod_data) = cache.mods().get_by(&item.unique_name) else {
        return;
    };

    if let Some(entry) = mod_data.upgrade_entries.get(rank as usize) {
        item.properties
            .set_property_value("mod_effect", entry.formatted_value.clone());
    }

    let ranks: Vec<serde_json::Value> = mod_data
        .upgrade_entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            serde_json::json!({
                "rank": i,
                "formatted_value": entry.formatted_value,
                "label": entry.label,
            })
        })
        .collect();
    if !ranks.is_empty() {
        item.properties.set_property_value("mod_ranks", ranks);
    }
}
