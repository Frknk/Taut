<script lang="ts">
  import TabList from '#lib/components/TabList.svelte';
  import TabPanel from '#lib/components/TabPanel.svelte';
  import Tabs from '#lib/components/Tabs.svelte';

  /** Tabs whose list and panel sit apart, as in the incidents page. */
  let { onchange }: { onchange?: (value: string) => void } = $props();
  let range = $state<'24h' | '7d' | '30d'>('24h');
</script>

<Tabs bind:value={range} {onchange}>
  <TabList
    label="Rango"
    options={[
      { value: '24h', label: '24 h' },
      { value: '7d', label: '7 días' },
      { value: '30d', label: '30 días' },
    ]}
  />
  <p>Entre medio</p>
  <TabPanel value={range}>Datos de {range}</TabPanel>
</Tabs>
