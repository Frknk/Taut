<script lang="ts">
  import Logo from '#lib/components/Logo.svelte';

  /**
   * The full screen shown while the session or the first data loads: the mark builds itself piece
   * by piece and then breathes. «Cargando…» stays as text for screen readers and as a caption.
   */
</script>

<div
  class="flex min-h-screen flex-col items-center justify-center gap-4 text-sm text-subtle"
  role="status"
>
  <Logo class="w-10 text-strong" animated />
  Cargando…
</div>
