// Page layout: the order, width and visibility of the blocks below the
// tiles. Saved in this browser only, like the theme.

/** Columns of a 12-column grid. */
export type Width = 3 | 4 | 6 | 12;

export const WIDTHS: { w: Width; label: string; name: string }[] = [
  { w: 3, label: '1/4', name: 'Quarter width' },
  { w: 4, label: '1/3', name: 'Third width' },
  { w: 6, label: '1/2', name: 'Half width' },
  { w: 12, label: 'Full', name: 'Full width' },
];

export interface BlockDef {
  id: string;
  title: string;
  width: Width;
}

export interface Block extends BlockDef {
  hidden: boolean;
}

interface Saved {
  order: string[];
  hidden: string[];
  width: Record<string, Width>;
}

const KEY = 'halogen-layout';
const EMPTY: Saved = { order: [], hidden: [], width: {} };

function read(): Saved {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? 'null') as Partial<Saved> | null;
    if (v && typeof v === 'object') {
      return {
        order: Array.isArray(v.order) ? v.order.filter((x) => typeof x === 'string') : [],
        hidden: Array.isArray(v.hidden) ? v.hidden.filter((x) => typeof x === 'string') : [],
        width: v.width && typeof v.width === 'object' ? v.width : {},
      };
    }
  } catch {
    /* storage can be blocked or hold old data */
  }
  return EMPTY;
}

class Layout {
  arranging = $state(false);
  private saved = $state.raw<Saved>(read());
  private defaults: BlockDef[] = [];

  /** The blocks of the page, in the default order and size. Call once at start. */
  setDefaults(defs: BlockDef[]): void {
    this.defaults = defs;
  }

  /** Blocks in display order. A block that is new since the layout was saved goes after its default neighbour. */
  get blocks(): Block[] {
    const s = this.saved;
    const known = new Map(this.defaults.map((d) => [d.id, d]));
    const order = s.order.filter((id, i) => known.has(id) && s.order.indexOf(id) === i);
    this.defaults.forEach((d, i) => {
      if (order.includes(d.id)) return;
      let at = 0;
      for (let j = i - 1; j >= 0; j--) {
        const k = order.indexOf(this.defaults[j]!.id);
        if (k >= 0) {
          at = k + 1;
          break;
        }
      }
      order.splice(at, 0, d.id);
    });
    return order.map((id) => {
      const d = known.get(id)!;
      const w = s.width[id];
      return { ...d, width: WIDTHS.some((x) => x.w === w) ? w! : d.width, hidden: s.hidden.includes(id) };
    });
  }

  get changed(): boolean {
    const s = this.saved;
    return s.order.length > 0 || s.hidden.length > 0 || Object.keys(s.width).length > 0;
  }

  /** Moves a block to the place of another block. */
  moveTo(id: string, target: string): void {
    const order = this.blocks.map((b) => b.id);
    const from = order.indexOf(id);
    const to = order.indexOf(target);
    if (from < 0 || to < 0 || from === to) return;
    order.splice(from, 1);
    order.splice(to, 0, id);
    this.save({ ...this.saved, order });
  }

  /** Moves a block one place earlier (-1) or later (+1). */
  step(id: string, delta: -1 | 1): void {
    const order = this.blocks.map((b) => b.id);
    const target = order[order.indexOf(id) + delta];
    if (target) this.moveTo(id, target);
  }

  setWidth(id: string, w: Width): void {
    this.save({ ...this.saved, width: { ...this.saved.width, [id]: w } });
  }

  toggle(id: string): void {
    const hidden = this.saved.hidden.includes(id)
      ? this.saved.hidden.filter((x) => x !== id)
      : [...this.saved.hidden, id];
    this.save({ ...this.saved, hidden });
  }

  reset(): void {
    this.save(EMPTY);
  }

  private save(s: Saved): void {
    this.saved = s;
    try {
      if (s === EMPTY) localStorage.removeItem(KEY);
      else localStorage.setItem(KEY, JSON.stringify(s));
    } catch {
      /* storage can be blocked */
    }
  }
}

export const layout = new Layout();
