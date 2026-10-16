<script lang="ts">
  import { resolve } from '$app/paths';

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
  }

  let { incidents, tenantId, showMonitor = true }: Props = $props();

  let expanded = $state<string | null>(null);
  let detail = $state<IncidentDetail | null>(null);
  let detailError = $state('');

  async function toggle(incident: Incident) {
    if (expanded === incident.id) {
      expanded = null;
      return;
    }
    expanded = incident.id;
    detail = null;
    detailError = '';
    try {
      detail = await request<IncidentDetail>('GET', `/v1/incidents/${incident.id}`, {
        tenant: tenantId,
      });
    } catch (err) {
      detailError = describeError(err);
    }
  }
</script>

<ul class={[card, 'divide-y divide-hairline']}>
  {#each incidents as incident (incident.id)}
    <li class="px-4 py-3">
      <button
        class={[
          'flex w-full cursor-pointer flex-wrap items-center gap-x-4 gap-y-1 rounded-sm text-left',
          focusRing,
        ]}
        aria-expanded={expanded === incident.id}
        onclick={() => toggle(incident)}
      >
        <Badge tone={incident.resolved_at ? 'neutral' : 'danger'}>
          {incident.resolved_at ? 'Resuelto' : 'Abierto'}
        </Badge>
        {#if showMonitor}
          <a
            href={resolve('/(app)/monitors/[id]', { id: incident.monitor_id })}
            class="font-medium text-default hover:underline"
            onclick={(event) => event.stopPropagation()}
          >
            {incident.monitor_name}
          </a>
        {/if}
        <span class="min-w-0 flex-1 truncate text-subtle">{incident.cause}</span>
        <span class="text-xs text-subtle tabular-nums">
          {formatDateTime(incident.started_at)} · {incident.resolved_at ? 'duró' : 'lleva'}
          {formatSeconds(incident.duration_s)}
        </span>
      </button>
      {#if expanded === incident.id}
        <div class="mt-3 rounded-md bg-recessed px-3 py-2 text-xs text-subtle">
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
                    {delivery.status === 'sent' ? 'enviada' : `falló (${delivery.last_error})`}
                  </span>
                  {#if delivery.attempts > 1}· {delivery.attempts} intentos{/if}
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
    </li>
  {/each}
</ul>
