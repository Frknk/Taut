<script lang="ts">
  import { Tooltip as TooltipPrimitive } from 'bits-ui';

  import Tooltip from '#lib/components/Tooltip.svelte';

  /** A focusable element before the tooltip, to reach it with Tab as a user would. */
</script>

<TooltipPrimitive.Provider delayDuration={0}>
  <button type="button">Antes</button>
  <p>Creada <Tooltip content="16/10/26, 21:09">hace 2 min</Tooltip></p>
</TooltipPrimitive.Provider>
