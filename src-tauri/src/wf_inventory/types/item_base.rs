use serde::{Deserialize, Serialize};
use utils::{Error, Properties, SubType};

use crate::cache::{CacheState, CacheSyndicateTitle};
use crate::wf_inventory::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WFInvItemBase {
    #[serde(rename = "id", default)]
    pub id: String,

    #[serde(rename = "name", default)]
    pub name: String,

    #[serde(rename = "unique_name", default)]
    pub unique_name: String,

    #[serde(rename = "wfm_url", default)]
    pub wfm_url: String,

    #[serde(rename = "quantity", default)]
    pub quantity: i64,

    #[serde(rename = "sub_type", skip_serializing_if = "Option::is_none")]
    pub sub_type: Option<SubType>,

    // Extra properties that may be needed for specific item types, stored as a JSON object
    #[serde(flatten)]
    pub properties: Properties,
}

impl WFInvItemBase {}

impl Default for WFInvItemBase {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            unique_name: String::new(),
            wfm_url: String::new(),
            quantity: 1,
            sub_type: None,
            properties: Properties::default(),
        }
    }
}

//
// Conversions
//
impl WFInvItemBase {
    pub fn from_affiliation(
        affiliation: &WFInvAffiliation,
        cache: &CacheState,
    ) -> Result<Self, Error> {
        let syndicate = cache.syndicate().get_by(affiliation.id.clone())?;
        let title = syndicate
            .get_title_by_level(affiliation.rank as i32)
            .unwrap_or(CacheSyndicateTitle::default());
        let mut item = WFInvItemBase::default();

        let standing = affiliation.standing;
        let min_standing = if affiliation.rank < 0 {
            title.max_standing
        } else {
            title.min_standing
        };
        item.id = affiliation.id.clone();
        item.name = syndicate.name.clone();
        item.unique_name = syndicate.unique_name.clone();
        item.sub_type = Some(SubType::rank(affiliation.rank as i64));

        item.quantity = standing - min_standing;
        item.properties
            .set_property_value("total".to_string(), standing);
        item.properties
            .set_property_value("min_standing".to_string(), title.min_standing);
        item.properties
            .set_property_value("max_standing".to_string(), title.max_standing);
        item.properties
            .set_property_value("can_select".to_string(), syndicate.can_select);
        item.properties.set_property_value(
            "background_colour".to_string(),
            syndicate.background_colour.clone(),
        );
        item.properties
            .set_property_value("colour".to_string(), syndicate.icon_colour.clone());
        Ok(item)
    }

    /// Builds an owned-inventory item (a non-riven mod or a regular tradable item like a prime
    /// part/relic/resource) from its unique_name and a pre-aggregated owned quantity.
    pub fn from_raw(
        unique_name: &str,
        quantity: i64,
        sub_type: Option<SubType>,
        cache: &CacheState,
    ) -> Result<Self, Error> {
        let info = cache.tradable_item().get_by(unique_name)?;
        let mut item = WFInvItemBase::default();

        item.id = info.wfm_id.clone();
        item.name = info.name.clone();
        item.unique_name = info.unique_name.clone();
        item.wfm_url = info.wfm_url.clone();
        item.quantity = quantity;
        item.sub_type = sub_type;

        item.properties
            .set_property_value("icon".to_string(), info.icon.clone());
        item.properties
            .set_property_value("tags".to_string(), info.tags.clone());
        item.properties
            .set_property_value("bulk_tradable".to_string(), info.bulk_tradable);

        Ok(item)
    }
}
