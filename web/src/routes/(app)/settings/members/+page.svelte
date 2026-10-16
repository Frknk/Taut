<script lang="ts">
  import { onMount } from 'svelte';

  import Badge from '#lib/components/Badge.svelte';
  import Banner from '#lib/components/Banner.svelte';
  import LayerCard from '#lib/components/LayerCard.svelte';
  import { describeError, request } from '#lib/api.js';
  import { ROLE_LABELS, timeAgo } from '#lib/format.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { Member } from '#lib/types.js';
  import { table } from '#lib/ui.js';

  const tenantId = $derived(selectedTenant.current?.id ?? '');

  let members = $state<Member[]>([]);
  let error = $state('');

  onMount(async () => {
    try {
      members = await request<Member[]>('GET', '/v1/members', { tenant: tenantId });
    } catch (err) {
      error = describeError(err);
    }
  });
</script>

<svelte:head>
  <title>Miembros · taut</title>
</svelte:head>

{#if error}<Banner tone="danger">{error}</Banner>{/if}

<LayerCard title="Miembros de {selectedTenant.current?.name}" flush>
  <table class={table.root}>
    <thead>
      <tr>
        <th class={table.th}>Nombre</th>
        <th class={table.th}>Correo</th>
        <th class={table.th}>Rol</th>
        <th class={table.th}>Se unió</th>
      </tr>
    </thead>
    <tbody>
      {#each members as member (member.user_id)}
        <tr class={table.row}>
          <td class={[table.td, 'font-medium text-strong']}>{member.name}</td>
          <td class={table.td}>{member.email}</td>
          <td class={table.td}>
            <Badge tone={member.role === 'owner' ? 'inverted' : 'outline'}>
              {ROLE_LABELS[member.role]}
            </Badge>
          </td>
          <td class={[table.td, 'text-subtle']}>{timeAgo(member.joined_at)}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  <p class="border-t border-hairline px-3 py-2 text-xs text-subtle">
    Las invitaciones y los cambios de rol llegan en la siguiente entrega.
  </p>
</LayerCard>
