import { Box, Button, Divider, Group, NumberInput, ScrollArea, Stack, Table, Text, Title } from "@mantine/core";
import { useTranslateModals } from "@hooks/useTranslate.hook";
import { DisplayPlatinum } from "@components/DataDisplay/DisplayPlatinum";
import { RivenAttribute } from "@components/DataDisplay/RivenAttribute";
import { TauriTypes, ItemRiven } from "$types";
import api from "@api/index";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";

const useTranslate = (key: string, context?: { [key: string]: any }) => useTranslateModals(`quick_sell.${key}`, { ...context });

export type QuickSellModalProps =
  | {
      kind: "item";
      item: TauriTypes.WFInvItemBase;
      onConfirm: (price: number, quantity: number) => Promise<void>;
    }
  | {
      kind: "riven";
      riven: ItemRiven;
      onConfirm: (price: number) => Promise<void>;
    };

export function QuickSellModal(props: QuickSellModalProps) {
  const [price, setPrice] = useState<number>(0);
  const [quantity, setQuantity] = useState<number>(props.kind === "item" ? props.item.quantity : 1);
  const [priceLocked, setPriceLocked] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  const itemMarketQuery = useQuery({
    queryKey: ["wf_inventory_get_item_market", props.kind === "item" ? props.item.wfm_url : null, props.kind === "item" ? props.item.sub_type : null],
    queryFn: () => api.wf_inventory.getItemMarket((props as any).item.wfm_url, (props as any).item.sub_type),
    enabled: props.kind === "item",
  });

  const rivenMarketQuery = useQuery({
    queryKey: ["wf_inventory_get_riven_market", props.kind === "riven" ? props.riven.wfm_url : null],
    queryFn: () => {
      const riven = (props as any).riven as ItemRiven;
      const positives = riven.attributes.filter((a) => a.positive).map((a) => a.wfmUrl);
      const negative = riven.attributes.find((a) => !a.positive)?.wfmUrl;
      return api.wf_inventory.getRivenMarket(riven.wfm_url, positives, negative);
    },
    enabled: props.kind === "riven",
  });

  // Prefill the price with the cheapest comparable listing once it loads, unless the user has
  // already started editing it.
  useEffect(() => {
    if (priceLocked) return;
    if (props.kind === "item" && itemMarketQuery.data) {
      const lowest = itemMarketQuery.data.sell[0]?.platinum;
      if (lowest !== undefined) setPrice(lowest);
    }
    if (props.kind === "riven" && rivenMarketQuery.data) {
      const lowest = rivenMarketQuery.data.top[0]?.buyout_price ?? rivenMarketQuery.data.top[0]?.starting_price;
      if (lowest !== undefined) setPrice(lowest);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [itemMarketQuery.data, rivenMarketQuery.data]);

  const handleConfirm = async () => {
    setSubmitting(true);
    try {
      if (props.kind === "item") {
        await props.onConfirm(price, quantity);
      } else {
        await props.onConfirm(price);
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Stack p="md" gap="sm">
      {props.kind === "item" && (
        <Group justify="space-between">
          <Title order={4}>{props.item.name}</Title>
          {props.item.sub_type?.rank !== undefined && <Text c="dimmed">{useTranslate("labels.quantity")}: Rank {props.item.sub_type.rank}</Text>}
        </Group>
      )}
      {props.kind === "riven" && (
        <>
          <Title order={4}>
            {props.riven.name} <Text span c="blue">{props.riven.mod_name}</Text>
          </Title>
          <Group gap={4}>
            {props.riven.attributes.map((attr) => (
              <RivenAttribute key={attr.url_name} i18nKey="full" value={attr} hideDetails centered hideGrade />
            ))}
          </Group>
        </>
      )}

      <Group grow align="flex-end">
        <NumberInput
          label={useTranslate("labels.price")}
          value={price}
          min={1}
          onChange={(value) => {
            setPriceLocked(true);
            setPrice(Number(value) || 0);
          }}
        />
        {props.kind === "item" && (
          <NumberInput label={useTranslate("labels.quantity")} value={quantity} min={1} max={props.item.quantity} onChange={(value) => setQuantity(Number(value) || 1)} />
        )}
      </Group>

      <Divider />

      {props.kind === "item" && (
        <Box>
          <Title order={6} mb={4}>
            {useTranslate("labels.top_orders")}
          </Title>
          <ScrollArea.Autosize mah={180}>
            <Table>
              <Table.Tbody>
                {itemMarketQuery.data?.sell?.map((order, i) => (
                  <Table.Tr key={i}>
                    <Table.Td>{order.user?.ingame_name}</Table.Td>
                    <Table.Td>
                      <DisplayPlatinum value={order.platinum} size="sm" />
                    </Table.Td>
                  </Table.Tr>
                ))}
                {itemMarketQuery.data && itemMarketQuery.data.sell.length === 0 && (
                  <Table.Tr>
                    <Table.Td c="dimmed">{useTranslate("labels.no_results")}</Table.Td>
                  </Table.Tr>
                )}
              </Table.Tbody>
            </Table>
          </ScrollArea.Autosize>
        </Box>
      )}

      {props.kind === "riven" && (
        <Group grow align="flex-start">
          <Box>
            <Title order={6} mb={4}>
              {useTranslate("labels.top_auctions")}
            </Title>
            <ScrollArea.Autosize mah={180}>
              <Table>
                <Table.Tbody>
                  {rivenMarketQuery.data?.top?.map((auction, i) => (
                    <Table.Tr key={i}>
                      <Table.Td>{auction.owner?.ingame_name}</Table.Td>
                      <Table.Td>
                        <DisplayPlatinum value={auction.buyout_price ?? auction.starting_price} size="sm" />
                      </Table.Td>
                    </Table.Tr>
                  ))}
                  {rivenMarketQuery.data && rivenMarketQuery.data.top.length === 0 && (
                    <Table.Tr>
                      <Table.Td c="dimmed">{useTranslate("labels.no_results")}</Table.Td>
                    </Table.Tr>
                  )}
                </Table.Tbody>
              </Table>
            </ScrollArea.Autosize>
          </Box>
          <Box>
            <Title order={6} mb={4}>
              {useTranslate("labels.top_auctions_ingame")}
            </Title>
            <ScrollArea.Autosize mah={180}>
              <Table>
                <Table.Tbody>
                  {rivenMarketQuery.data?.top_ingame?.map((auction, i) => (
                    <Table.Tr key={i}>
                      <Table.Td>{auction.owner?.ingame_name}</Table.Td>
                      <Table.Td>
                        <DisplayPlatinum value={auction.buyout_price ?? auction.starting_price} size="sm" />
                      </Table.Td>
                    </Table.Tr>
                  ))}
                  {rivenMarketQuery.data && rivenMarketQuery.data.top_ingame.length === 0 && (
                    <Table.Tr>
                      <Table.Td c="dimmed">{useTranslate("labels.no_results")}</Table.Td>
                    </Table.Tr>
                  )}
                </Table.Tbody>
              </Table>
            </ScrollArea.Autosize>
          </Box>
        </Group>
      )}

      <Group justify="flex-end" mt="md">
        <Button loading={submitting} onClick={handleConfirm}>
          {useTranslate("buttons.confirm")}
        </Button>
      </Group>
    </Stack>
  );
}
