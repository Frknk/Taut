// Class recipes of the Taut UI components, shared by the Svelte components and plain markup.

export type ButtonVariant =
  | 'primary'
  | 'secondary'
  | 'outline'
  | 'ghost'
  | 'destructive'
  | 'secondary-destructive';

export type ButtonSize = 'xs' | 'sm' | 'base' | 'lg';

export type Tone = 'neutral' | 'inverted' | 'outline' | 'success' | 'warning' | 'danger' | 'info';

/** Keyboard focus ring: 2px of `focus`, 2px away from the element. */
export const focusRing =
  'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus';

const BUTTON_BASE = [
  'inline-flex shrink-0 cursor-pointer items-center justify-center font-medium whitespace-nowrap select-none',
  focusRing,
  'disabled:cursor-not-allowed disabled:bg-tint disabled:text-inactive disabled:shadow-none disabled:ring-0',
].join(' ');

const BUTTON_VARIANTS: Record<ButtonVariant, string> = {
  primary: 'bg-brand text-on-brand ring ring-line shadow-xs hover:bg-brand-hover',
  secondary: 'bg-control text-default ring ring-line shadow-xs hover:bg-tint',
  outline: 'text-default ring ring-line hover:bg-tint',
  ghost: 'text-default hover:bg-tint',
  destructive: 'bg-danger text-on-danger ring ring-line shadow-xs hover:bg-danger-hover',
  'secondary-destructive': 'bg-control text-danger ring ring-line shadow-xs hover:bg-tint',
};

// Heights match Kumo: 20, 26, 36 and 40px. The bottom padding centers text optically.
const BUTTON_SIZES: Record<ButtonSize, string> = {
  xs: 'h-5 gap-1 rounded-sm px-1.5 pb-px text-xs',
  sm: 'h-[26px] gap-1 rounded-md px-2 pb-px text-xs',
  base: 'h-9 gap-1.5 rounded-lg px-3 pb-0.5 text-sm',
  lg: 'h-10 gap-2 rounded-lg px-4 pb-0.5 text-sm',
};

const SQUARE_SIZES: Record<ButtonSize, string> = {
  xs: 'w-5 p-0',
  sm: 'w-[26px] p-0',
  base: 'w-9 p-0',
  lg: 'w-10 p-0',
};

/** Classes of a button (or a link that looks like one). */
export function buttonClass(
  variant: ButtonVariant = 'secondary',
  size: ButtonSize = 'base',
  square = false,
): string {
  return [
    BUTTON_BASE,
    BUTTON_VARIANTS[variant],
    BUTTON_SIZES[size],
    square ? SQUARE_SIZES[size] : '',
  ]
    .filter(Boolean)
    .join(' ');
}

const TONES: Record<Tone, string> = {
  neutral: 'bg-fill text-default',
  inverted: 'bg-contrast text-inverse',
  outline: 'text-subtle ring ring-inset ring-line',
  success: 'bg-success-tint text-success',
  warning: 'bg-warning-tint text-warning',
  danger: 'bg-danger-tint text-danger',
  info: 'bg-info-tint text-info',
};

/** Classes of a badge: one or two words of status beside a name. */
export function badgeClass(tone: Tone = 'neutral'): string {
  return `inline-flex w-fit shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium whitespace-nowrap ${TONES[tone]}`;
}

/**
 * Classes of a banner: a sentence or two about the current page, with its icon. `large` is the
 * overall state at the top of the public status page.
 */
export function bannerClass(
  tone: Exclude<Tone, 'neutral' | 'inverted' | 'outline'>,
  large = false,
): string {
  const size = large ? 'px-5 py-4 text-base font-medium' : 'px-4 py-3 text-sm';
  return `flex items-start gap-2 rounded-lg ${size} ${TONES[tone]}`;
}

export const label = 'block text-sm font-medium text-default';

/** The field itself, without the 6px gap to its label. */
export const inputBox = [
  'block h-9 w-full rounded-lg bg-control px-3 text-sm text-default ring ring-line',
  'placeholder:text-placeholder focus:ring-[1.5px] focus:ring-focus focus:outline-none',
  'aria-invalid:ring-[1.5px] aria-invalid:ring-danger',
  'disabled:cursor-not-allowed disabled:bg-tint disabled:text-inactive',
].join(' ');

/** A text input 6px below its label. */
export const input = `mt-1.5 ${inputBox}`;

export const help = 'mt-1.5 text-xs text-subtle';

export const fieldError = 'mt-1.5 text-xs text-danger';

export const link = `rounded-sm text-link underline decoration-1 underline-offset-2 hover:decoration-2 ${focusRing}`;

/** A back link above a page title: «← Monitores». */
export const backLink = `inline-flex items-center gap-1 rounded-sm text-sm text-subtle hover:text-default ${focusRing}`;

/** Outer shell of a LayerCard and of plain lists that stand for one. */
export const card = 'overflow-hidden rounded-lg bg-base ring ring-hairline';

/** Table recipe: 12px cells, semibold headers, zebra rows. */
export const table = {
  root: 'w-full border-separate border-spacing-0 text-left text-sm text-default',
  th: 'border-b border-fill bg-base p-3 font-semibold whitespace-nowrap text-strong',
  td: 'p-3 align-middle',
  row: 'even:bg-elevated',
};

/** Dropdown menu recipe: the floating panel, an item, a group heading and a separator. */
export const menu = {
  panel: 'absolute z-20 min-w-full rounded-lg bg-base p-1 shadow-lg ring ring-line',
  item: [
    'flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-default',
    'hover:bg-tint focus-visible:bg-tint focus-visible:outline-none',
  ].join(' '),
  heading: 'px-2 pt-1.5 pb-1 text-xs text-subtle',
  separator: 'my-1 h-px bg-hairline',
};

/** Modal dialog recipe: the scrim behind it and the centred panel. */
export const dialog = {
  overlay: 'fixed inset-0 z-50 bg-scrim',
  content: [
    'fixed top-1/2 left-1/2 z-50 max-h-[calc(100dvh-2rem)] w-[min(100%-2rem,440px)] -translate-1/2 overflow-y-auto',
    'rounded-xl bg-base px-6 py-5 text-sm text-default shadow-lg ring ring-line focus-visible:outline-none',
  ].join(' '),
};

/** A link of the sidebar; `bg-tint` and medium weight mark the current page. */
export function navItemClass(current: boolean): string {
  const state = current
    ? 'bg-tint font-medium text-default'
    : 'text-subtle hover:bg-tint hover:text-default';
  return `flex h-8 items-center gap-2 rounded-md px-2 text-sm ${focusRing} ${state}`;
}
