use entity::stock_item::StockItemPaginationQueryDto;
use entity::stock_riven::StockRivenPaginationQueryDto;
use qf_api::enums::app_events::ApplicationEvent as EventType;
use serde::Serialize;
use serde_json::{json, Value};
use service::{StockItemQuery, StockRivenQuery};
use std::sync::{Arc, Mutex};
use utils::{get_location, Error, SubType};
use wf_market::{
    enums::{AuctionType, StatusType},
    types::{AuctionFilter, AuctionWithOwner, OrderWithUser, TopOrdersFilters},
};

use crate::utils::ErrorFromExt;
use crate::utils::modules::states;
use crate::utils::{wfm_request_queue, RequestPriority};
use crate::wf_inventory::WFInventoryState;
use crate::wf_inventory::WFItemPaginationDto;
use crate::{track_event, DATABASE};

#[tauri::command]
pub async fn wf_inventory_get_rivens(
    query: WFItemPaginationDto,
    wf_inventory: tauri::State<'_, Mutex<Arc<WFInventoryState>>>,
) -> Result<Value, Error> {
    let wf_inventory = wf_inventory.lock()?.clone();
    let conn = DATABASE.get().unwrap();
    let rivens = StockRivenQuery::get_all(conn, StockRivenPaginationQueryDto::new(1, -1)).await?;
    let uuids: Vec<String> = rivens.results.iter().map(|r| r.uuid.clone()).collect();
    let mut veiled = wf_inventory.riven().get_rivens(query)?;
    for item in veiled.results.iter_mut() {
        item.base
            .properties
            .set_property_value("is_in_stock", uuids.contains(&item.base.uuid));
    }

    Ok(json!(veiled))
}

#[tauri::command]
pub async fn wf_inventory_get_items(
    query: WFItemPaginationDto,
    wf_inventory: tauri::State<'_, Mutex<Arc<WFInventoryState>>>,
) -> Result<Value, Error> {
    let wf_inventory = wf_inventory.lock()?.clone();
    let conn = DATABASE.get().unwrap();
    let stock_items = StockItemQuery::get_all(conn, StockItemPaginationQueryDto::new(1, -1)).await?;
    let tracked: std::collections::HashSet<(String, Option<SubType>)> = stock_items
        .results
        .iter()
        .map(|item| (item.wfm_url.clone(), item.sub_type.clone()))
        .collect();

    let mut items = wf_inventory.item().get_items(query)?;
    for item in items.results.iter_mut() {
        let is_in_stock = tracked.contains(&(item.wfm_url.clone(), item.sub_type.clone()));
        item.properties.set_property_value("is_in_stock", is_in_stock);
    }

    Ok(json!(items))
}

#[derive(Debug, Serialize)]
pub struct WFItemMarketResult {
    pub buy: Vec<OrderWithUser>,
    pub sell: Vec<OrderWithUser>,
}

/// Fetches the top 5 live sell orders (and top 5 buy orders) for a regular tradable item,
/// restricted to users who were online within the last 7 days - used to prefill the quick-sell
/// price when listing an owned item from the inventory window.
#[tauri::command]
pub async fn wf_inventory_get_item_market(
    wfm_url: String,
    sub_type: Option<SubType>,
) -> Result<WFItemMarketResult, Error> {
    let app = states::app_state()?;

    let mut filters = TopOrdersFilters::new();
    if let Some(rank) = sub_type.and_then(|s| s.rank) {
        filters = filters.with_rank(rank as u32);
    }

    // High priority: the user is actively waiting on this one result, so it jumps ahead of
    // whatever the live scraper has queued rather than waiting its turn behind it.
    let item_url = wfm_url.clone();
    let top = wfm_request_queue()
        .submit(RequestPriority::High, move || async move {
            app.wfm_client
                .order()
                .get_top_orders_by_item(&item_url, Some(filters))
                .await
        })
        .await
        .map_err(|e| {
            Error::from_wfm(
                "WFInventory:GetItemMarket",
                format!("Failed to get live orders for item {}", wfm_url),
                e,
                get_location!(),
            )
        })?;

    Ok(WFItemMarketResult {
        buy: top.buy,
        sell: top.sell,
    })
}

#[derive(Debug, Serialize)]
pub struct WFRivenMarketResult {
    /// Top 5 cheapest similar riven auctions overall.
    pub top: Vec<AuctionWithOwner>,
    /// Top 5 cheapest similar riven auctions from sellers currently online in-game.
    pub top_ingame: Vec<AuctionWithOwner>,
}

/// Fetches similar riven auctions (overall, and restricted to sellers currently online
/// in-game) for a given weapon/stat combination - used to prefill the quick-sell price when
/// listing an unveiled riven from the inventory window.
#[tauri::command]
pub async fn wf_inventory_get_riven_market(
    weapon_url: String,
    positive_attributes: Vec<String>,
    negative_attribute: Option<String>,
) -> Result<WFRivenMarketResult, Error> {
    let app = states::app_state()?;

    let mut filter = AuctionFilter::new(AuctionType::Riven, &weapon_url)
        .with_buyout_policy("direct")
        .with_sort_by("price_asc");
    if !positive_attributes.is_empty() {
        filter = filter.with_positive_stats(positive_attributes);
    }
    if let Some(negative) = negative_attribute {
        filter = filter.with_negative_stats(vec![negative]);
    }

    // A single search covers both lists: each auction already carries the seller's online
    // status, so "in-game only" is a local filter on this one response instead of a second
    // network request (every call here was eating into warframe.market's rate limit twice).
    // High priority: the user is actively waiting on this, so it jumps ahead of the live
    // scraper's queued background requests instead of waiting its turn behind them.
    let mut overall = wfm_request_queue()
        .submit(RequestPriority::High, move || async move {
            app.wfm_client.auction().search_auctions(filter).await
        })
        .await
        .map_err(|e| {
            Error::from_wfm(
                "WFInventory:GetRivenMarket",
                format!("Failed to search auctions for weapon {}", weapon_url),
                e,
                get_location!(),
            )
        })?;
    overall.sort_by_platinum();

    let mut ingame = overall.clone();
    ingame.filter_user_status(StatusType::InGame, false);

    Ok(WFRivenMarketResult {
        top: overall.take_top(5),
        top_ingame: ingame.take_top(5),
    })
}
#[tauri::command]
pub async fn wf_inventory_get_syndicates(
    query: WFItemPaginationDto,
    wf_inventory: tauri::State<'_, Mutex<Arc<WFInventoryState>>>,
) -> Result<Value, Error> {
    let wf_inventory = wf_inventory.lock()?.clone();
    Ok(json!(&wf_inventory.syndicate().get_syndicates(query)?))
}

#[tauri::command]
pub async fn wf_inventory_update(
    wf_inventory: tauri::State<'_, Mutex<Arc<WFInventoryState>>>,
) -> Result<(), Error> {
    let wf_inventory = wf_inventory.lock()?.clone();
    wf_inventory.update().map_err(|e| {
        track_event!(
            EventType::WFInventoryUpdate,
            [
                ("success", "false".to_string()),
                ("error_type", "update_failed".to_string()),
            ]
        );
        e
    })?;
    track_event!(EventType::WFInventoryUpdate, [("success", "true".to_string())]);
    Ok(())
}
