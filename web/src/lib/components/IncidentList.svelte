<script lang="ts">
  import { resolve } from '$app/paths';

  import { describeError, request } from '#lib/api.js';
  import { CHANNEL_LABELS, formatDateTime, formatSeconds } from '#lib/format.js';
  import type { Incident, IncidentDetail } from '#lib/types.js';

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

<ul class="divide-y divide-neutral-100 rounded-xl border border-neutral-200 bg-white">
  {#each incidents as incident (incident.id)}
    <li class="px-4 py-3 text-sm">
      <button
        class="flex w-full flex-wrap items-center gap-x-4 gap-y-1 text-left"
        onclick={() => toggle(incident)}
      >
        <span
          class={[
            'rounded-full px-2.5 py-0.5 text-xs font-medium',
            incident.resolved_at ? 'bg-neutral-100 text-neutral-600' : 'bg-red-50 text-red-700',
          ]}
        >
          {incident.resolved_at ? 'Resuelto' : 'Abierto'}
        </span>
        {#if showMonitor}
          <a
            href={resolve('/(app)/monitors/[id]', { id: incident.monitor_id })}
            class="font-medium text-neutral-900 hover:underline"
            onclick={(event) => event.stopPropagation()}
          >
            {incident.monitor_name}
          </a>
        {/if}
        <span class="min-w-0 flex-1 truncate text-neutral-600">{incident.cause}</span>
        <span class="text-xs text-neutral-500">
          {formatDateTime(incident.started_at)} · {incident.resolved_at ? 'duró' : 'lleva'}
          {formatSeconds(incident.duration_s)}
        </span>
      </button>
      {#if expanded === incident.id}
        <div class="mt-3 rounded-lg bg-neutral-50 px-3 py-2 text-xs text-neutral-600">
          {#if detailError}
            <p class="text-red-700">{detailError}</p>
          {:else if !detail}
            <p>Cargando…</p>
          {:else if detail.deliveries.length === 0}
            <p>No se envió ninguna alerta (el tenant no tenía canales activos).</p>
          {:else}
            <p class="font-medium text-neutral-700">Alertas enviadas</p>
            <ul class="mt-1 space-y-1">
              {#each detail.deliveries as delivery (delivery.channel_id + delivery.event)}
                <li>
                  {delivery.event === 'opened' ? 'Apertura' : 'Cierre'} →
                  {delivery.channel_name} ({CHANNEL_LABELS[delivery.channel_type]}):
                  <span class={delivery.status === 'sent' ? 'text-emerald-700' : 'text-red-700'}>
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
