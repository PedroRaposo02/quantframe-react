import { useQuery } from "@tanstack/react-query";
import api from "@api/index";
import { TauriTypes } from "$types";
import { useTauriEvent } from "@hooks/useTauriEvent.hook";

interface QueriesHooks {
  queryData: TauriTypes.WFItemControllerGetListParams;
  isActive?: boolean;
}

// Fallback polling interval for sources that don't push updates on their own (e.g. Profile).
const FALLBACK_REFRESH_INTERVAL_MS = 5 * 60 * 1000;

export const useQueries = ({ queryData, isActive }: QueriesHooks) => {
  const itemsQuery = useQuery({
    queryKey: ["wf_inventory_get_items", queryData],
    queryFn: () => api.wf_inventory.getItemsPagination(queryData),
    retry: false,
    enabled: isActive,
    refetchInterval: isActive ? FALLBACK_REFRESH_INTERVAL_MS : false,
  });
  const refetchQueries = () => {
    itemsQuery.refetch();
  };

  // Backend pushes this event as soon as the inventory source (e.g. AlecaFrame's file watcher)
  // detects new/changed data, so the list updates near-instantly instead of waiting on the poll.
  useTauriEvent(TauriTypes.Events.RefreshWfInventory, refetchQueries, [refetchQueries]);

  return {
    itemsQuery,
    refetchQueries,
  };
};
