use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use entity::{dto::PaginatedResult, enums::FieldChange};
use utils::{SortDirection, SubType};
use utils::Error;

use crate::{
    helper::paginate,
    utils::modules::states,
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
        let mut owned_mods: HashMap<(String, i64), i64> = HashMap::new();
        for raw in root.raw_upgrades.iter().chain(root.upgrades.iter()) {
            if raw.is_riven() {
                continue;
            }
            let rank = raw.get_upgrade_fingerprint().mod_rank;
            *owned_mods
                .entry((raw.unique_name.clone(), rank))
                .or_insert(0) += raw.quantity.max(1);
        }

        // Regular tradable items (prime parts, relics, resources, etc.), no sub_type.
        let mut owned_misc: HashMap<String, i64> = HashMap::new();
        for raw in root.misc_items.iter() {
            *owned_misc.entry(raw.unique_name.clone()).or_insert(0) += raw.quantity;
        }

        let mut items: Vec<WFInvItemBase> = vec![];
        for ((unique_name, rank), quantity) in owned_mods {
            if let Ok(item) =
                WFInvItemBase::from_raw(&unique_name, quantity, Some(SubType::rank(rank)), &cache)
            {
                items.push(item);
            }
        }
        for (unique_name, quantity) in owned_misc {
            if let Ok(item) = WFInvItemBase::from_raw(&unique_name, quantity, None, &cache) {
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
                _ => {}
            }
        }

        Ok(paginate(&items, query.pagination.page, query.pagination.limit))
    }
}
