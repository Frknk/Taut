<script lang="ts">
  import { onMount } from 'svelte';
  import { resolve } from '$app/paths';
  import ArrowRightIcon from 'phosphor-svelte/lib/ArrowRightIcon';
  import BellRingingIcon from 'phosphor-svelte/lib/BellRingingIcon';
  import GlobeIcon from 'phosphor-svelte/lib/GlobeIcon';
  import PulseIcon from 'phosphor-svelte/lib/PulseIcon';
  import SirenIcon from 'phosphor-svelte/lib/SirenIcon';

  import Button from '#lib/components/Button.svelte';
  import Logo from '#lib/components/Logo.svelte';
  import StatusBadge from '#lib/components/StatusBadge.svelte';
  import { prefersReducedMotion } from '#lib/motion.js';
  import type { MonitorStatus } from '#lib/types.js';
  import { focusRing } from '#lib/ui.js';

  /**
   * The public front page, for visitors without a session. The hero text enters word by word and
   * the card beside it tells the product's story once, in three seconds: the checks come in (the
   * latency line draws itself), the monitor turns operational, its history fills up and the team
   * gets the alert. Everything below the hero stays still: the story is the one entrance.
   */

  const HEADLINE = 'Entérate antes que tus usuarios.';
  const WORDS = HEADLINE.split(' ');

  /** When each beat of the story starts (ms after mount); each one waits for the previous. */
  const BEATS = { card: 250, line: 600, status: 1900, bars: 2000, toast: 2900 };

  // 30 days of history with one short outage, the one the toast talks about.
  const DAYS = Array.from({ length: 30 }, (_, index) => (index === 18 ? 'critical' : 'good'));

  // A latency trace in a 320×80 box: steady around 45, one spike at the outage.
  const LATENCY =
    'M0 46 L14 44 L28 48 L42 43 L56 47 L70 45 L84 50 L98 44 L112 46 L126 42 L140 47 L154 45 ' +
    'L168 49 L182 43 L196 24 L203 9 L210 27 L224 44 L238 46 L252 43 L266 48 L280 44 L294 47 ' +
    'L308 45 L320 46';

  const FEATURES = [
    {
      icon: PulseIcon,
      title: 'Chequeos cada pocos segundos',
      text: 'HTTP, palabra clave, puerto TCP y heartbeat, con intervalos de 20 segundos a 5 minutos.',
    },
    {
      icon: SirenIcon,
      title: 'Incidentes con criterio',
      text: 'Una falla se confirma antes de abrir un incidente y se cierra sola cuando el servicio vuelve. Sin alarmas por un parpadeo.',
    },
    {
      icon: BellRingingIcon,
      title: 'Avisos donde está tu equipo',
      text: 'Correo, Slack, Discord o un webhook firmado con HMAC. Un aviso al caer y otro al recuperarse.',
    },
    {
      icon: GlobeIcon,
      title: 'Página de estado pública',
      text: 'Cada organización publica la suya, con 90 días de historia por monitor y sin exponer destinos ni causas.',
    },
  ];

  // With reduced motion the story is already told: every beat is in its final state, with no
  // delayed fades (the hero text keeps its quick ones, which layout.css turns into fades).
  const still = prefersReducedMotion();
  const beat = (delay: number) => (still ? undefined : `${delay}ms`);
  let status = $state<MonitorStatus>(still ? 'up' : 'pending');
  let alerted = $state(still);

  onMount(() => {
    if (still) return;
    const timers = [
      setTimeout(() => (status = 'up'), BEATS.status),
      setTimeout(() => (alerted = true), BEATS.toast),
    ];
    return () => timers.forEach(clearTimeout);
  });

  const footerLink = `rounded-sm text-subtle hover:text-default ${focusRing}`;
</script>

<div class="min-h-screen">
  <header class="mx-auto flex max-w-5xl items-center justify-between px-6 py-5">
    <Logo class="w-7 text-strong" label="Taut" />
    <nav class="flex items-center gap-2" aria-label="Cuenta">
      <Button variant="ghost" href={resolve('/(auth)/login')}>Entrar</Button>
      <Button variant="primary" href={resolve('/(auth)/register')}>Crear cuenta</Button>
    </nav>
  </header>

  <main>
    <section
      class="mx-auto grid max-w-5xl items-center gap-12 px-6 pt-12 pb-20 lg:grid-cols-[1.1fr_1fr] lg:gap-16 lg:pt-20"
    >
      <div>
        <p
          class="flex animate-rise-up items-center gap-2 text-xs font-medium text-subtle [animation-fill-mode:backwards]"
        >
          <span class="size-1.5 rounded-full bg-success" aria-hidden="true"></span>
          Monitoreo de disponibilidad
        </p>
        <h1 class="mt-4 text-4xl leading-[1.1] font-semibold text-strong sm:text-5xl">
          {#each WORDS as word, index (index)}
            <span
              class="inline-block animate-rise-up [animation-fill-mode:backwards]"
              style:animation-delay="{80 + index * 40}ms">{word}</span
            >{#if index < WORDS.length - 1}{' '}{/if}
          {/each}
        </h1>
        <p
          class="mt-5 max-w-[46ch] animate-rise-up text-base text-subtle [animation-fill-mode:backwards]"
          style:animation-delay="320ms"
        >
          Taut revisa tus sitios, APIs y servidores cada pocos segundos. Cuando algo falla, abre un
          incidente y avisa a tu equipo por correo, Slack, Discord o webhook. Cuando vuelve,
          también.
        </p>
        <div
          class="mt-8 flex animate-rise-up flex-wrap items-center gap-3 [animation-fill-mode:backwards]"
          style:animation-delay="420ms"
        >
          <Button variant="primary" size="lg" href={resolve('/(auth)/register')}>
            Crear cuenta
            <ArrowRightIcon size={16} aria-hidden="true" />
          </Button>
          <Button size="lg" href={resolve('/(auth)/login')}>Entrar</Button>
          <p class="text-xs text-subtle sm:ml-1">Plan gratuito, sin tarjeta.</p>
        </div>
      </div>

      <!-- The story. One picture for assistive tech: its text is an illustration, not content. -->
      <div
        class={[
          'relative mb-5 rounded-xl bg-elevated p-1 shadow-md ring ring-line',
          !still && 'animate-pop-in [animation-fill-mode:backwards]',
        ]}
        style:animation-delay={beat(BEATS.card)}
        role="img"
        aria-label="Ejemplo: un monitor HTTP operativo, con su latencia, 30 días de historia y el aviso de una caída de 3 minutos ya resuelta."
      >
        <div class="rounded-lg bg-base px-5 py-4 ring ring-fill">
          <div class="flex items-start justify-between gap-3">
            <div>
              <p class="font-medium text-strong">api.ejemplo.com</p>
              <p class="text-xs text-subtle">HTTP · cada 30 s</p>
            </div>
            <StatusBadge {status} label={status === 'up' ? 'Operativo' : 'Sin datos aún'} />
          </div>

          <div class="mt-5 flex items-baseline justify-between text-xs text-subtle">
            <p>Latencia</p>
            <p class="tabular-nums">184 ms</p>
          </div>
          <svg class="mt-2 h-20 w-full" viewBox="0 0 320 80" preserveAspectRatio="none">
            {#each [20, 40, 60] as y (y)}
              <line
                x1="0"
                x2="320"
                y1={y}
                y2={y}
                stroke="var(--viz-grid)"
                vector-effect="non-scaling-stroke"
              />
            {/each}
            <!-- pathLength=1 and a dash of 1: the line draws from the first check to the last. -->
            <path
              d={LATENCY}
              fill="none"
              stroke="var(--viz-series-1)"
              stroke-width="1.5"
              stroke-linejoin="round"
              stroke-linecap="round"
              vector-effect="non-scaling-stroke"
              pathLength="1"
              class={[!still && 'animate-draw [stroke-dasharray:1] [animation-duration:1200ms]']}
              style:animation-delay={beat(BEATS.line)}
            />
          </svg>

          <div class="mt-5 flex items-baseline justify-between text-xs text-subtle">
            <p>Últimos 30 días</p>
            <p class="tabular-nums">99,6 % disponible</p>
          </div>
          <div class="mt-2 flex gap-0.5">
            {#each DAYS as level, index (index)}
              <span
                class={[
                  'h-6 flex-1 rounded-xs',
                  !still && 'animate-pop-in [animation-fill-mode:backwards]',
                ]}
                style:background="var(--status-{level})"
                style:animation-delay={beat(BEATS.bars + index * 20)}
              ></span>
            {/each}
          </div>
        </div>

        {#if alerted}
          <div
            class={[
              'absolute inset-x-4 -bottom-5 flex items-start gap-2 rounded-lg bg-base px-3 py-2 text-xs text-subtle shadow-md ring ring-line',
              !still && 'animate-rise-up',
            ]}
          >
            <span class="flex h-lh items-center text-strong">
              <BellRingingIcon size={14} aria-hidden="true" />
            </span>
            <p>
              <span class="font-medium text-default">Caída de 3 min, resuelta.</span> El equipo lo supo
              por Slack y correo.
            </p>
          </div>
        {/if}
      </div>
    </section>

    <section class="mx-auto max-w-2xl px-6 py-16 text-center">
      <p class="font-mono text-xs text-subtle">taut /tɔːt/ · adjetivo</p>
      <p class="mt-3 text-2xl font-semibold text-strong sm:text-3xl">Tenso, firme, sin holgura.</p>
      <p class="mt-3 text-subtle">
        Como una cuerda bien tensa: en cuanto algo se afloja, lo notas.
      </p>
    </section>

    <section class="mx-auto max-w-5xl px-6 pb-20" aria-labelledby="features">
      <h2 id="features" class="sr-only">Qué hace Taut</h2>
      <ul class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {#each FEATURES as feature (feature.title)}
          <li class="rounded-lg bg-base px-5 py-4 ring ring-hairline">
            <span
              class="flex size-8 items-center justify-center rounded-md bg-tint text-strong"
              aria-hidden="true"
            >
              <feature.icon size={16} />
            </span>
            <h3 class="mt-4 font-semibold text-strong">{feature.title}</h3>
            <p class="mt-1 text-subtle">{feature.text}</p>
          </li>
        {/each}
      </ul>
    </section>
  </main>

  <footer class="border-t border-hairline">
    <div
      class="mx-auto flex max-w-5xl flex-wrap items-center justify-between gap-4 px-6 py-8 text-xs text-subtle"
    >
      <p class="flex items-center gap-2">
        <Logo class="w-4" />
        Taut · proyecto del curso IS04E104
      </p>
      <nav class="flex gap-4" aria-label="Pie de página">
        <a class={footerLink} href={resolve('/(auth)/register')}>Crear cuenta</a>
        <a class={footerLink} href={resolve('/(auth)/login')}>Entrar</a>
        <a class={footerLink} href="https://github.com/Frknk/Taut" rel="noreferrer">GitHub</a>
      </nav>
    </div>
  </footer>
</div>
