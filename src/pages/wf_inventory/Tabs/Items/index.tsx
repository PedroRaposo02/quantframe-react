import { Badge, Box, Button, Card, Divider, Group, Image, ScrollArea, SimpleGrid, Stack, Text } from "@mantine/core";
import { PaginationFooter } from "@components/Shared/PaginationFooter";
import { useQueries } from "./queries";
import { useMutations } from "./mutations";
import { useHasAlert } from "@hooks/useHasAlert.hook";
import classes from "../../WFInventory.module.css";
import { useTranslatePages } from "@hooks/useTranslate.hook";
import { useLocalStorage } from "@mantine/hooks";
import { TauriTypes } from "$types";
import { SearchField } from "@components/Forms/SearchField";
import { modals } from "@mantine/modals";
import { QuickSellModal } from "@components/Modals/QuickSell";
import { ExpandableButton } from "@components/Shared/ExpandableButton";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { faInfinity, faPuzzlePiece, faFlask, faCube, faScrewdriverWrench, faBoxOpen } from "@fortawesome/free-solid-svg-icons";

interface ItemsPanelProps {
  isActive: boolean;
}

type WFInvItem = TauriTypes.WFInvItemBase<{ icon: string; tags: string[]; is_in_stock: boolean }>;

// "__other__" is a reserved marker the backend treats as "none of the known category tags" -
// see OTHER_CATEGORY_MARKER in src-tauri/src/wf_inventory/modules/item.rs.
const CATEGORIES: { key: string; icon: any; tags: string[] }[] = [
  { key: "mods", icon: faPuzzlePiece, tags: ["mod"] },
  { key: "arcanes", icon: faFlask, tags: ["arcane_enhancement", "arcane_helmet"] },
  { key: "relics", icon: faCube, tags: ["relic"] },
  { key: "parts", icon: faScrewdriverWrench, tags: ["component", "blueprint"] },
  { key: "other", icon: faBoxOpen, tags: ["__other__"] },
];

export const ItemsPanel = ({ isActive }: ItemsPanelProps) => {
  const [queryData, setQueryData] = useLocalStorage<TauriTypes.WFItemControllerGetListParams>({
    key: "wf_inventory_items_query_key",
    getInitialValueInEffect: false,
    defaultValue: { page: 1, limit: 50 },
  });

  const { itemsQuery, refetchQueries } = useQueries({ queryData, isActive });
  const { createMutation } = useMutations({ refetchQueries, setLoadingRows: () => {} });

  const useTranslate = (key: string, context?: { [key: string]: any }) => useTranslatePages(`wf_inventory.tabs.items.${key}`, { ...context });

  const handleSell = (item: WFInvItem) => {
    const id = modals.open({
      size: "lg",
      withCloseButton: true,
      children: (
        <QuickSellModal
          kind="item"
          item={item}
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

  return (
    <Box p="md">
      <SearchField value={queryData.query || ""} onChange={(text) => setQueryData((prev) => ({ ...prev, query: text }))} />
      <Group mt="md" gap={3}>
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
      <ScrollArea mt="md" className={classes.veiledRivens} data-has-alert={useHasAlert()}>
        <SimpleGrid cols={{ base: 4 }} spacing="sm">
          {results.map((item, i) => (
              <Card key={i} withBorder padding="sm">
                <Stack gap={4}>
                  <Group justify="space-between" wrap="nowrap">
                    <Group gap={6} wrap="nowrap">
                      <Image
                        src={item.properties?.icon ? `https://cdn.warframestat.us/img/${item.properties.icon}` : "/question.png"}
                        fallbackSrc="/question.png"
                        w={32}
                        h={32}
                        fit="contain"
                      />
                      <Text size="sm" fw={500} lineClamp={1}>
                        {item.name}
                      </Text>
                    </Group>
                    {item.properties?.is_in_stock && (
                      <Badge size="xs" color="green">
                        {useTranslate("item_card.in_stock")}
                      </Badge>
                    )}
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
                  <Button size="xs" fullWidth onClick={() => handleSell(item)}>
                    {useTranslate("item_card.sell_button")}
                  </Button>
                </Stack>
              </Card>
            ))}
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
    </Box>
  );
};
