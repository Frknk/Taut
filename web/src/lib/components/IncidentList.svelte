<script lang="ts">
  import { resolve } from '$app/paths';
  import { Accordion } from 'bits-ui';
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon';

  import Badge from '#lib/components/Badge.svelte';
  import { describeError, request } from '#lib/api.js';
  import { CHANNEL_LABELS, formatDateTime, formatSeconds } from '#lib/format.js';
  import type { Incident, IncidentDetail } from '#lib/types.js';
  import { card, focusRing } from '#lib/ui.js';

  interface Props {
    incidents: Incident[];
    tenantId: string;
    /** Hide the monitor name (inside a monitor's own page). */
    showMonitor?: boolean;
    /** Each incident is a heading one level below the section that holds the list. */
    headingLevel?: 2 | 3;
  }

  /**
   * Incidents as a bits-ui accordion: one opens at a time and loads the alerts it sent. The
   * monitor link sits beside the trigger, never inside it.
   */
  let { incidents, tenantId, showMonitor = true, headingLevel = 3 }: Props = $props();

  let expanded = $state('');
  let detail = $state<IncidentDetail | null>(null);
  let detailError = $state('');

  async function load(id: string) {
    detail = null;
    detailError = '';
    if (!id) return;
    try {
      const loaded = await request<IncidentDetail>('GET', `/v1/incidents/${id}`, {
        tenant: tenantId,
      });
      // A quicker click on another incident wins.
      if (expanded === id) detail = loaded;
    } catch (err) {
      if (expanded === id) detailError = describeError(err);
    }
  }
</script>

<Accordion.Root type="single" bind:value={expanded} onValueChange={(id) => void load(id)}>
  {#snippet child({ props })}
    <ul {...props} class={[card, 'divide-y divide-hairline']}>
      {#each incidents as incident (incident.id)}
        <Accordion.Item value={incident.id}>
          {#snippet child({ props: itemProps })}
            <li {...itemProps} class="px-4 py-3">
              <Accordion.Header
                level={headingLevel}
                class="flex flex-wrap items-center gap-x-4 gap-y-1"
              >
                <Badge tone={incident.resolved_at ? 'neutral' : 'danger'}>
                  {incident.resolved_at ? 'Resuelto' : 'Abierto'}
                </Badge>
                {#if showMonitor}
                  <a
                    href={resolve('/(app)/monitors/[id]', { id: incident.monitor_id })}
                    class={['rounded-sm font-medium text-default hover:underline', focusRing]}
                  >
                    {incident.monitor_name}
                  </a>
                {/if}
                <!-- bits-ui leaves the trigger and its panel unlinked; the ids tie them. -->
                <Accordion.Trigger
                  id="incident-{incident.id}"
                  aria-controls={expanded === incident.id
                    ? `incident-${incident.id}-panel`
                    : undefined}
                  class={[
                    'group flex min-w-0 flex-1 cursor-pointer flex-wrap items-center gap-x-4 gap-y-1 rounded-sm text-left',
                    focusRing,
                  ]}
                >
                  <span class="min-w-0 flex-1 truncate text-subtle">{incident.cause}</span>
                  <span class="text-xs text-subtle tabular-nums">
                    {formatDateTime(incident.started_at)} · {incident.resolved_at
                      ? 'duró'
                      : 'lleva'}
                    {formatSeconds(incident.duration_s)}
                  </span>
                  <CaretDownIcon
                    size={14}
                    class="shrink-0 text-subtle group-data-[state=open]:rotate-180"
                    aria-hidden="true"
                  />
                </Accordion.Trigger>
              </Accordion.Header>
              <Accordion.Content
                id="incident-{incident.id}-panel"
                role="region"
                aria-labelledby="incident-{incident.id}"
                class="mt-3 rounded-md bg-recessed px-3 py-2 text-xs text-subtle"
              >
                {#if detailError}
                  <p class="text-danger">{detailError}</p>
                {:else if !detail}
                  <p>Cargando…</p>
                {:else if detail.deliveries.length === 0}
                  <p>No se envió ninguna alerta (el tenant no tenía canales activos).</p>
                {:else}
                  <p class="font-medium text-default">Alertas enviadas</p>
                  <ul class="mt-1 space-y-1">
                    {#each detail.deliveries as delivery (delivery.channel_id + delivery.event)}
                      <li>
                        {delivery.event === 'opened' ? 'Apertura' : 'Cierre'} →
                        {delivery.channel_name} ({CHANNEL_LABELS[delivery.channel_type]}):
                        <span class={delivery.status === 'sent' ? 'text-success' : 'text-danger'}>
                          {delivery.status === 'sent'
                            ? 'enviada'
                            : `falló (${delivery.last_error})`}
                        </span>
                        {#if delivery.attempts > 1}· {delivery.attempts} intentos{/if}
                      </li>
                    {/each}
                  </ul>
                {/if}
              </Accordion.Content>
            </li>
          {/snippet}
        </Accordion.Item>
      {/each}
    </ul>
  {/snippet}
</Accordion.Root>
