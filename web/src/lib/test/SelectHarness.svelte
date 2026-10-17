<script lang="ts">
  import Select from '#lib/components/Select.svelte';

  /** A labelled select over numbers, as the monitor interval. */
  let { onchange }: { onchange?: (value: number) => void } = $props();
  let value = $state(60);
</script>

<label for="harness-interval">Frecuencia</label>
<Select
  id="harness-interval"
  bind:value
  {onchange}
  items={[
    { value: 30, label: 'Cada 30 s' },
    { value: 60, label: 'Cada 1 min' },
    { value: 300, label: 'Cada 5 min', disabled: true },
    { value: 600, label: 'Cada 10 min' },
  ]}
/>
<output>{value}</output>
