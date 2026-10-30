export interface Selection {
  selected: Set<string>;
  anchor: string | null;
  focus: string | null;
}
export const emptySelection = (): Selection => ({
  selected: new Set(),
  anchor: null,
  focus: null,
});

export function selectRow(
  state: Selection,
  paths: string[],
  path: string,
  extend = false,
  toggle = false,
): Selection {
  if (!paths.includes(path)) return state;
  if (extend) {
    const anchor =
      state.anchor && paths.includes(state.anchor) ? state.anchor : path;
    const start = paths.indexOf(anchor);
    const end = paths.indexOf(path);
    const range = paths.slice(Math.min(start, end), Math.max(start, end) + 1);
    return {
      selected: new Set(toggle ? [...state.selected, ...range] : range),
      anchor,
      focus: path,
    };
  }
  const selected = toggle ? new Set(state.selected) : new Set<string>();
  if (toggle && selected.has(path)) selected.delete(path);
  else selected.add(path);
  return { selected, anchor: path, focus: path };
}

export function moveSelection(
  state: Selection,
  paths: string[],
  direction: -1 | 1,
  extend = false,
): Selection {
  if (!paths.length) return emptySelection();
  const index = state.focus ? paths.indexOf(state.focus) : -1;
  const next =
    index < 0
      ? direction === 1
        ? 0
        : paths.length - 1
      : Math.max(0, Math.min(paths.length - 1, index + direction));
  return selectRow(state, paths, paths[next], extend);
}

export function removeSelection(state: Selection, paths: string[]): Selection {
  const remaining = paths.filter((path) => !state.selected.has(path));
  if (!remaining.length) return emptySelection();
  const index = Math.max(0, paths.indexOf(state.focus ?? ""));
  const before = paths
    .slice(0, index)
    .filter((path) => !state.selected.has(path)).length;
  const focus = remaining[Math.min(before, remaining.length - 1)];
  return { selected: new Set([focus]), focus, anchor: focus };
}
