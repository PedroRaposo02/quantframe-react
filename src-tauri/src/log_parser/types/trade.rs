use std::{collections::HashMap, fmt::Display, vec};

use crate::enums::TradeItemType;
use crate::{log_parser::*, utils::modules::states};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use utils::Properties;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PlayerTrade {
    #[serde(rename = "playerName")]
    pub player_name: String,
    #[serde(rename = "tradeTime")]
    pub trade_time: DateTime<chrono::Utc>,
    #[serde(rename = "type")]
    pub trade_type: TradeClassification,
    #[serde(rename = "platinum")]
    pub platinum: i64,
    #[serde(rename = "credits")]
    pub credits: i64,
    #[serde(rename = "offeredItems")]
    pub offered_items: Vec<TradeItem>,
    #[serde(rename = "receivedItems")]
    pub received_items: Vec<TradeItem>,

    #[serde(flatten)]
    pub properties: Properties,
}

impl Default for PlayerTrade {
    fn default() -> Self {
        PlayerTrade {
            player_name: "".to_string(),
            trade_time: chrono::Utc::now(),
            trade_type: TradeClassification::Unknown,
            platinum: 0,
            credits: 0,
            offered_items: vec![],
            received_items: vec![],
            properties: Properties::default(),
        }
    }
}

impl PlayerTrade {
    pub fn get_received_plat(&self) -> i64 {
        self.received_items
            .iter()
            .filter(|p| p.item_type == TradeItemType::Platinum)
            .map(|p| p.quantity)
            .sum::<i64>()
    }
    pub fn get_offered_plat(&self) -> i64 {
        self.offered_items
            .iter()
            .filter(|p| p.item_type == TradeItemType::Platinum)
            .map(|p| p.quantity)
            .sum::<i64>()
    }
    pub fn get_valid_items(
        &self,
        trade_type: &TradeClassification,
        mut exclude_types: Vec<TradeItemType>,
    ) -> Vec<TradeItem> {
        exclude_types.push(TradeItemType::Unknown);
        let items = match trade_type.clone() {
            TradeClassification::Purchase => &self.offered_items,
            TradeClassification::Sale => &self.received_items,
            TradeClassification::Any => &self
                .received_items
                .iter()
                .chain(&self.offered_items)
                .cloned()
                .collect::<Vec<TradeItem>>(),
            _ => &vec![],
        };
        items
            .iter()
            .filter(|p| !exclude_types.contains(&p.item_type))
            .cloned()
            .collect()
    }
    pub fn finalize_trade(&mut self) {
        let offer_plat = self.get_offered_plat();
        let receive_plat = self.get_received_plat();

        self.credits = self
            .offered_items
            .iter()
            .filter(|p| p.item_type == TradeItemType::Credits)
            .map(|p| p.quantity)
            .sum();

        // platinum resolution (last non-zero wins)
        if offer_plat > 0 {
            self.platinum = offer_plat;
        }
        if receive_plat > 0 {
            self.platinum = receive_plat;
        }

        let offered_items: Vec<_> = self
            .get_valid_items(&TradeClassification::Purchase, vec![])
            .into_iter()
            .filter(|p| p.item_type != TradeItemType::Credits)
            .collect();

        let received_items: Vec<_> = self
            .get_valid_items(&TradeClassification::Sale, vec![])
            .into_iter()
            .filter(|p| p.item_type != TradeItemType::Credits)
            .collect();

        self.trade_type = TradeClassification::classify_trade(
            offer_plat,
            receive_plat,
            offered_items.len(),
            received_items.len(),
        );
    }
    pub fn finalize_items(&mut self) {
        self.is_set(TradeClassification::Purchase);
        self.is_set(TradeClassification::Sale);
    }

    pub fn is_set(&mut self, trade_type: TradeClassification) {
        let items = self.get_valid_items(&trade_type, vec![]);

        if items.len() <= 1 {
            return;
        }

        let cache = match states::cache_client() {
            Ok(cache) => cache,
            Err(_) => return,
        };

        let mut owned_counts: HashMap<String, i64> = HashMap::new();
        for item in &items {
            *owned_counts.entry(item.unique_name.clone()).or_insert(0) += item.quantity;
        }

        let matches = cache.tradable_item().find_matching_sets(&owned_counts);
        if matches.is_empty() {
            return;
        }

        let target_items = match trade_type {
            TradeClassification::Purchase => &mut self.offered_items,
            TradeClassification::Sale => &mut self.received_items,
            _ => return,
        };

        for (set_item, sets_formed, consumed) in matches {
            let consumed_counts: HashMap<&String, i64> =
                consumed.iter().map(|(name, qty)| (name, *qty)).collect();

            // Consume only as many parts as the number of complete sets found, so leftover
            // parts from a partial extra set (e.g. 2 complete sets + 1 spare Chassis) survive
            // as their own line item instead of being discarded.
            for item in target_items.iter_mut() {
                if let Some(qty) = consumed_counts.get(&item.unique_name) {
                    item.quantity -= qty;
                }
            }
            target_items.retain(|item| item.quantity > 0);

            let mut set_trade_item = TradeItem::new(
                &set_item.unique_name,
                sets_formed,
                TradeItemType::Set,
                None,
            );

            set_trade_item
                .properties
                .set_property_value("tags", vec!["set".to_string()]);

            set_trade_item
                .properties
                .set_property_value("item_name", set_item.name.clone());

            target_items.push(set_trade_item);
        }
    }

    pub fn get_notify_variables(&self) -> HashMap<String, String> {
        let offered_items = self
            .get_valid_items(&TradeClassification::Purchase, vec![])
            .iter()
            .map(|x| format!("{} X{}", x.item_name(), x.quantity))
            .collect::<Vec<String>>()
            .join("\n");

        let received_items = self
            .get_valid_items(&TradeClassification::Sale, vec![])
            .iter()
            .map(|x| format!("{} X{}", x.item_name(), x.quantity))
            .collect::<Vec<String>>()
            .join("\n");

        return HashMap::from([
            ("<TR_TYPE>".to_string(), self.trade_type.to_string()),
            ("<PLAYER_NAME>".to_string(), self.player_name.clone()),
            (
                "<OF_COUNT>".to_string(),
                self.offered_items.len().to_string(),
            ),
            ("<OF_ITEMS>".to_string(), offered_items),
            (
                "<RE_COUNT>".to_string(),
                self.received_items.len().to_string(),
            ),
            (
                "<TIME>".to_string(),
                self.trade_time
                    .with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
            ),
            ("<RE_ITEMS>".to_string(), received_items),
            ("<TOTAL_PLAT>".to_string(), self.platinum.to_string()),
        ]);
    }

    pub fn set_time(&mut self, time: DateTime<chrono::Utc>) -> PlayerTrade {
        self.trade_time = time;
        self.clone()
    }
}
impl Display for PlayerTrade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.player_name.is_empty() {
            write!(f, "Player Name: Not provided | ")?;
        } else {
            write!(f, "Player Name: {} | ", self.player_name)?;
        }
        write!(f, "Trade Time: {} | ", self.trade_time)?;
        write!(f, "Type: {:?} | ", self.trade_type)?;
        write!(f, "Platinum: {} | ", self.platinum)?;
        write!(f, "Offered Items: {} | ", self.offered_items.len())?;
        write!(f, "Received Items: {} | ", self.received_items.len())?;
        if self.trade_type == TradeClassification::Purchase && self.received_items.len() == 1 {
            write!(
                f,
                "Item: {}",
                self.received_items.first().unwrap().item_name()
            )?;
        } else if self.trade_type == TradeClassification::Sale && self.offered_items.len() == 1 {
            write!(
                f,
                "Item: {}",
                self.offered_items.first().unwrap().item_name()
            )?;
        }
        Ok(())
    }
}
