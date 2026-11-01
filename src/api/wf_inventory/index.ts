import { TauriTypes } from "$types";
import { useMutation } from "@tanstack/react-query";
import { TauriClient } from "..";
export class WfInventoryModule {
  constructor(private readonly client: TauriClient) {}
  async getRivensPagination(query: TauriTypes.WFItemControllerGetListParams): Promise<TauriTypes.WFInvRivenControllerGetListData> {
    return await this.client.sendInvoke<TauriTypes.WFInvRivenControllerGetListData>("wf_inventory_get_rivens", {
      query: this.client.convertToTauriQuery(query),
    });
  }
  async getSyndicatesPagination(query: TauriTypes.WFItemControllerGetListParams): Promise<TauriTypes.WFInvSyndicateControllerGetListData> {
    return await this.client.sendInvoke<TauriTypes.WFInvSyndicateControllerGetListData>("wf_inventory_get_syndicates", {
      query: this.client.convertToTauriQuery(query),
    });
  }
  async getItemsPagination(query: TauriTypes.WFItemControllerGetListParams): Promise<TauriTypes.WFInvItemControllerGetListData> {
    return await this.client.sendInvoke<TauriTypes.WFInvItemControllerGetListData>("wf_inventory_get_items", {
      query: this.client.convertToTauriQuery(query),
    });
  }
  async getItemMarket(wfm_url: string, sub_type?: TauriTypes.SubType): Promise<TauriTypes.WFItemMarketResult> {
    return await this.client.sendInvoke<TauriTypes.WFItemMarketResult>("wf_inventory_get_item_market", { wfm_url, sub_type });
  }
  async getRivenMarket(weapon_url: string, positive_attributes: string[], negative_attribute?: string): Promise<TauriTypes.WFRivenMarketResult> {
    return await this.client.sendInvoke<TauriTypes.WFRivenMarketResult>("wf_inventory_get_riven_market", {
      weapon_url,
      positive_attributes,
      negative_attribute,
    });
  }
  update() {
    return useMutation({
      mutationFn: () => this.client.sendInvoke<void>("wf_inventory_update"),
    });
  }
}
