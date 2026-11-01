import {
  Badge,
  Box,
  Button,
  Card,
  Divider,
  Group,
  Image,
  Modal,
  NumberInput,
  ScrollArea,
  SegmentedControl,
  SimpleGrid,
  Slider,
  Stack,
  Table,
  Text,
  Tooltip,
} from "@mantine/core";
import { PaginationFooter } from "@components/Shared/PaginationFooter";
import { useQueries } from "./queries";
import { useMutations } from "./mutations";
import { useHasAlert } from "@hooks/useHasAlert.hook";
import classes from "../../WFInventory.module.css";
import { useTranslatePages, useTranslateCommon } from "@hooks/useTranslate.hook";
import { useLocalStorage } from "@mantine/hooks";
import { TauriTypes } from "$types";
import { SearchField } from "@components/Forms/SearchField";
import { modals } from "@mantine/modals";
import { QuickSellModal } from "@components/Modals/QuickSell";
import { ExpandableButton } from "@components/Shared/ExpandableButton";
import { CompactSortSelect } from "@components/Shared/CompactSortSelect";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { faInfinity, faPuzzlePiece, faFlask, faCube, faScrewdriverWrench, faBoxOpen, faMagnifyingGlassPlus } from "@fortawesome/free-solid-svg-icons";
import { useState } from "react";

interface ItemsPanelProps {
  isActive: boolean;
}

type WFInvItem = TauriTypes.WFInvItemBase<{
  icon: string;
  tags: string[];
  is_in_stock: boolean;
  price?: number;
  is_listed: boolean;
  set_complete?: boolean;
  equipped: number;
  sellable: number;
  is_equipped: boolean;
  mod_effect?: string;
  mod_ranks?: { rank: number; formatted_value: string; label: string }[];
}>;

interface ItemFilterProperties {
  min_quantity?: number;
  min_price?: number;
  is_listed?: boolean;
  set_status?: "complete" | "incomplete";
  equipped_status?: "equipped" | "not_equipped";
  reserve_quantity?: number;
}

// "__other__" is a reserved marker the backend treats as "none of the known category tags" -
// see OTHER_CATEGORY_MARKER in src-tauri/src/wf_inventory/modules/item.rs.
const CATEGORIES: { key: string; icon: any; tags: string[] }[] = [
  { key: "mods", icon: faPuzzlePiece, tags: ["mod"] },
  { key: "arcanes", icon: faFlask, tags: ["arcane_enhancement", "arcane_helmet"] },
  { key: "relics", icon: faCube, tags: ["relic"] },
  { key: "parts", icon: faScrewdriverWrench, tags: ["component", "blueprint"] },
  { key: "other", icon: faBoxOpen, tags: ["__other__"] },
];

const MAX_PRICE_SLIDER = 500;

export const ItemsPanel = ({ isActive }: ItemsPanelProps) => {
  const [queryData, setQueryData] = useLocalStorage<TauriTypes.WFItemControllerGetListParams<ItemFilterProperties>>({
    key: "wf_inventory_items_query_key",
    getInitialValueInEffect: false,
    defaultValue: { page: 1, limit: 50 },
  });
  const [minPriceDraft, setMinPriceDraft] = useState(queryData.properties?.min_price ?? 0);
  const [zoomedItem, setZoomedItem] = useState<WFInvItem | null>(null);

  const { itemsQuery, refetchQueries } = useQueries({ queryData, isActive });
  const { createMutation } = useMutations({ refetchQueries, setLoadingRows: () => {} });

  const useTranslate = (key: string, context?: { [key: string]: any }) => useTranslatePages(`wf_inventory.tabs.items.${key}`, { ...context });

  const setProperty = <K extends keyof ItemFilterProperties>(key: K, value: ItemFilterProperties[K]) =>
    setQueryData((prev) => ({ ...prev, page: 1, properties: { ...prev.properties, [key]: value } }));

  const handleSell = (item: WFInvItem) => {
    const sellable = item.properties?.sellable ?? item.quantity;
    const id = modals.open({
      size: "lg",
      withCloseButton: true,
      children: (
        <QuickSellModal
          kind="item"
          item={{ ...item, quantity: sellable }}
          onConfirm={async (price, quantity) => {
            await createMutation.mutateAsync({
              raw: item.wfm_url,
              quantity,
              bought: 0,
              sub_type: item.sub_type,
              min_price: price,
            } as any);
            modals.close(id);
          }}
        />
      ),
    });
  };

  const results = (itemsQuery.data?.results ?? []) as WFInvItem[];
  const iconUrl = (icon?: string) => (icon ? `https://warframe.market/static/assets/${icon}` : "/question.png");

  return (
    <Box p="md">
      <SearchField value={queryData.query || ""} onChange={(text) => setQueryData((prev) => ({ ...prev, query: text }))} />
      <Group mt="md" gap={3} justify="space-between">
        <Group gap={3}>
          <ExpandableButton
            p={3}
            collapsedWidth={32}
            icon={<FontAwesomeIcon icon={faInfinity} size="xl" />}
            selected={queryData.item_types === undefined}
            onClick={() => setQueryData((prev) => ({ ...prev, page: 1, item_types: undefined }))}
          >
            <Text>{useTranslate("filters.all")}</Text>
          </ExpandableButton>
          {CATEGORIES.map((category) => (
            <ExpandableButton
              key={category.key}
              p={3}
              collapsedWidth={32}
              icon={<FontAwesomeIcon icon={category.icon} size="xl" />}
              selected={JSON.stringify(queryData.item_types) === JSON.stringify(category.tags)}
              onClick={() => setQueryData((prev) => ({ ...prev, page: 1, item_types: category.tags }))}
            >
              <Text>{useTranslate(`filters.${category.key}`)}</Text>
            </ExpandableButton>
          ))}
        </Group>
        <CompactSortSelect
          value={queryData.sort_by || "name"}
          direction={queryData.sort_direction === "desc" ? "desc" : "asc"}
          onChange={(value) => setQueryData((prev) => ({ ...prev, sort_by: value }))}
          onDirectionChange={(direction) => setQueryData((prev) => ({ ...prev, sort_direction: direction }))}
          data={["name", "quantity", "price"].map((key) => ({ label: useTranslateCommon(`sort_by.${key}`), value: key }))}
        />
      </Group>
      <Group mt="sm" gap="lg" align="flex-end" wrap="wrap">
        <NumberInput
          label={useTranslate("filters.min_quantity")}
          w={140}
          min={0}
          value={queryData.properties?.min_quantity ?? ""}
          onChange={(value) => setProperty("min_quantity", value === "" ? undefined : Number(value))}
        />
        <NumberInput
          label={useTranslate("filters.reserve_quantity")}
          description={useTranslate("filters.reserve_quantity_description")}
          w={160}
          min={0}
          value={queryData.properties?.reserve_quantity ?? 0}
          onChange={(value) => setProperty("reserve_quantity", value === "" ? undefined : Number(value))}
        />
        <Box w={220}>
          <Text size="sm" mb={2}>
            {useTranslate("filters.min_price", { price: minPriceDraft })}
          </Text>
          <Slider
            min={0}
            max={MAX_PRICE_SLIDER}
            value={minPriceDraft}
            onChange={setMinPriceDraft}
            onChangeEnd={(value) => setProperty("min_price", value || undefined)}
          />
        </Box>
        <SegmentedControl
          value={queryData.properties?.is_listed === undefined ? "all" : queryData.properties.is_listed ? "listed" : "not_listed"}
          onChange={(value) => setProperty("is_listed", value === "all" ? undefined : value === "listed")}
          data={[
            { label: useTranslate("filters.listed_all"), value: "all" },
            { label: useTranslate("filters.listed"), value: "listed" },
            { label: useTranslate("filters.not_listed"), value: "not_listed" },
          ]}
        />
        <SegmentedControl
          value={queryData.properties?.set_status ?? "all"}
          onChange={(value) => setProperty("set_status", value === "all" ? undefined : (value as "complete" | "incomplete"))}
          data={[
            { label: useTranslate("filters.sets_all"), value: "all" },
            { label: useTranslate("filters.sets_complete"), value: "complete" },
            { label: useTranslate("filters.sets_incomplete"), value: "incomplete" },
          ]}
        />
        <SegmentedControl
          value={queryData.properties?.equipped_status ?? "all"}
          onChange={(value) => setProperty("equipped_status", value === "all" ? undefined : (value as "equipped" | "not_equipped"))}
          data={[
            { label: useTranslate("filters.equipped_all"), value: "all" },
            { label: useTranslate("filters.equipped"), value: "equipped" },
            { label: useTranslate("filters.not_equipped"), value: "not_equipped" },
          ]}
        />
      </Group>
      <ScrollArea mt="md" className={classes.veiledRivens} data-has-alert={useHasAlert()}>
        <SimpleGrid cols={{ base: 4 }} spacing="sm">
          {results.map((item, i) => {
            const sellable = item.properties?.sellable ?? item.quantity;
            const canZoom = item.properties?.tags?.includes("mod") ?? false;
            return (
              <Card key={i} withBorder padding="sm">
                <Stack gap={4}>
                  <Group justify="space-between" wrap="nowrap">
                    <Group gap={6} wrap="nowrap">
                      <Box pos="relative" style={{ cursor: canZoom ? "zoom-in" : undefined }} onClick={() => canZoom && setZoomedItem(item)}>
                        <Image src={iconUrl(item.properties?.icon)} fallbackSrc="/question.png" w={32} h={32} fit="contain" />
                      </Box>
                      <Text size="sm" fw={500} lineClamp={1}>
                        {item.name}
                      </Text>
                    </Group>
                    <Group gap={4} wrap="nowrap">
                      {item.properties?.is_equipped && (
                        <Badge size="xs" color="red">
                          {useTranslate("item_card.equipped", { count: item.properties.equipped })}
                        </Badge>
                      )}
                      {item.properties?.set_complete !== undefined && (
                        <Badge size="xs" color={item.properties.set_complete ? "green" : "yellow"}>
                          {useTranslate(item.properties.set_complete ? "item_card.set_complete" : "item_card.set_incomplete")}
                        </Badge>
                      )}
                      {item.properties?.is_in_stock && (
                        <Badge size="xs" color="green">
                          {useTranslate("item_card.in_stock")}
                        </Badge>
                      )}
                    </Group>
                  </Group>
                  <Group justify="space-between">
                    <Text size="xs" c="dimmed">
                      {useTranslate("item_card.owned", { quantity: item.quantity })}
                    </Text>
                    {item.sub_type?.rank !== undefined && (
                      <Text size="xs" c="dimmed">
                        {useTranslate("item_card.rank", { rank: item.sub_type.rank })}
                      </Text>
                    )}
                  </Group>
                  {item.properties?.price !== undefined && (
                    <Text size="xs" c="dimmed">
                      {useTranslate("item_card.price", { price: Math.round(item.properties.price) })}
                    </Text>
                  )}
                  <Group gap={4} wrap="nowrap">
                    <Tooltip label={sellable <= 0 ? useTranslate("item_card.nothing_sellable") : undefined} disabled={sellable > 0}>
                      <Button size="xs" fullWidth disabled={sellable <= 0} onClick={() => handleSell(item)}>
                        {useTranslate("item_card.sell_button", { count: sellable })}
                      </Button>
                    </Tooltip>
                    {canZoom && (
                      <ActionZoomButton onClick={() => setZoomedItem(item)} />
                    )}
                  </Group>
                </Stack>
              </Card>
            );
          })}
        </SimpleGrid>
      </ScrollArea>
      <Divider mt="md" />
      <PaginationFooter
        page={queryData.page}
        limit={queryData.limit || 50}
        total={itemsQuery.data?.total || 0}
        onPageChange={(page) => setQueryData((prev) => ({ ...prev, page }))}
        onLimitChange={(limit) => setQueryData((prev) => ({ ...prev, page: 1, limit }))}
      />

      <Modal opened={!!zoomedItem} onClose={() => setZoomedItem(null)} title={zoomedItem?.name} size="md">
        {zoomedItem && (
          <Stack align="center" gap="sm">
            <Image src={iconUrl(zoomedItem.properties?.icon)} fallbackSrc="/question.png" w={160} h={160} fit="contain" />
            {zoomedItem.properties?.mod_effect && (
              <Text fw={500}>{useTranslate("zoom.current_effect", { effect: zoomedItem.properties.mod_effect })}</Text>
            )}
            {zoomedItem.properties?.mod_ranks && zoomedItem.properties.mod_ranks.length > 0 && (
              <Table w="100%">
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>{useTranslate("zoom.rank")}</Table.Th>
                    <Table.Th>{useTranslate("zoom.effect")}</Table.Th>
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {zoomedItem.properties.mod_ranks.map((entry) => (
                    <Table.Tr key={entry.rank} bg={entry.rank === zoomedItem.sub_type?.rank ? "var(--mantine-color-blue-light)" : undefined}>
                      <Table.Td>{entry.rank}</Table.Td>
                      <Table.Td>{entry.formatted_value}</Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            )}
          </Stack>
        )}
      </Modal>
    </Box>
  );
};

const ActionZoomButton = ({ onClick }: { onClick: () => void }) => (
  <Button size="xs" variant="light" px={8} onClick={onClick}>
    <FontAwesomeIcon icon={faMagnifyingGlassPlus} />
  </Button>
);
